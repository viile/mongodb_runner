<script setup lang="ts">
import { computed, nextTick, onMounted, ref, watch } from 'vue';
import { ElMessage, ElMessageBox } from 'element-plus';
import { useI18n } from 'vue-i18n';
import {
  chatWithLLM,
  generateMongoCommand,
  getLLMStatus,
  type LLMSchema,
  type LLMResultPreview,
} from '../api/llm';
import {
  estimateImpact,
  sampleDocuments,
  type ExecuteResult,
  type ImpactInfo,
} from '../api/mongo';
import { useChat } from '../composables/useChat';
import { useLLMProfiles } from '../composables/useLLMProfiles';
import { formatMongoCommand } from '../utils/formatMongo';

const { t, locale } = useI18n();
const currentLocale = computed(() => String(locale.value));
const llm = useLLMProfiles();

const props = defineProps<{
  uri: string | null;
  database: string | null;
  collection: string | null;
  connectionName?: string | null;
  /** 编辑器当前内容（包含占位注释时也会进来） */
  currentCommand?: string | null;
  /** App.vue 里 i18n 占位文本，用来判断是不是「用户还没动过编辑器」 */
  placeholderCommand?: string | null;
  /** 上一次执行结果（包含 error / data 等） */
  lastResult?: ExecuteResult | null;
  /** 上一次「实际跑过去」的命令文本，用于精确判断结果归属 */
  lastExecutedCommand?: string | null;
}>();

const emit = defineEmits<{
  (e: 'use-command', cmd: string): void;
  (e: 'run-command', cmd: string): void;
}>();

const chat = useChat();
const input = ref('');
const loading = ref(false);
const includeSample = ref(true);
const includeCommand = ref(true);
const includeResult = ref(true);
/** LLM 生成的命令执行失败时是否自动让 LLM 给出修复建议 */
const autoFixOnError = ref(true);
const llmAvailable = ref<boolean | null>(null);
const llmInfo = ref<string>('');

/**
 * 用户点了某条 assistant 消息里的「运行」按钮后，记录我们在等什么结果。
 * 之后 props.lastResult 变化时，结合 props.lastExecutedCommand 判断这次结果是不是我们触发的，
 * 是失败的话就自动让 LLM 分析并给出修复方案。
 */
const pendingFix = ref<{ command: string } | null>(null);

/** 上一次结果是否值得作为上下文（成功有 data 或失败有 error 都算） */
const hasMeaningfulResult = computed(() => {
  const r = props.lastResult;
  if (!r) return false;
  if (r.ok) return r.data !== undefined && r.data !== null;
  return !!r.error;
});

/** 编辑器里有没有用户实际输入的内容（去掉初始 placeholder / 注释） */
const hasUserCommand = computed(() => {
  const c = (props.currentCommand ?? '').trim();
  if (!c) return false;
  if (props.placeholderCommand && c === props.placeholderCommand.trim()) return false;
  return true;
});

const scrollAreaRef = ref<HTMLElement | null>(null);

const QUICK_PROMPT_KEYS = ['chat.quick1', 'chat.quick2', 'chat.quick3', 'chat.quick4'] as const;
const quickPrompts = computed(() => QUICK_PROMPT_KEYS.map((k) => t(k)));

async function refreshStatus() {
  try {
    const r = await getLLMStatus(llm.active.value);
    llmAvailable.value = r.available;
    if (r.available) {
      // 有 active profile 时优先显示 profile 名；fallback 显示 env 检测出的 provider
      if (llm.active.value) {
        const p = llm.active.value;
        const tag = p.providerKind === 'cursor' ? 'cursor-agent' : p.model || 'openai';
        llmInfo.value = `${p.name} · ${tag}`;
      } else {
        const oai = r.providers?.openai;
        const cur = r.providers?.cursor;
        if (oai) llmInfo.value = `OpenAI · ${oai.model}`;
        else if (cur) llmInfo.value = `cursor-agent · ${cur.model || 'default'}`;
      }
    } else {
      llmInfo.value = r.error || t('chat.statusNotConfigured');
    }
  } catch {
    llmAvailable.value = false;
    llmInfo.value = t('chat.statusCantRead');
  }
}

onMounted(async () => {
  await refreshStatus();
  scrollToBottom();
});

// active profile 切换时同步刷状态
watch(
  () => llm.active.value?.id,
  () => {
    refreshStatus();
  }
);

watch(
  () => chat.messages.value.length,
  () => scrollToBottom()
);

function scrollToBottom() {
  nextTick(() => {
    const el = scrollAreaRef.value;
    if (el) el.scrollTop = el.scrollHeight;
  });
}

/** 把 result.data 序列化成 LLM 看的 preview（控制大小） */
function buildResultPreview(r: ExecuteResult): LLMResultPreview {
  const PREVIEW_MAX_CHARS = 3000;
  const PREVIEW_MAX_DOCS = 5;
  let previewJson: string | null = null;
  if (r.ok && r.data !== undefined && r.data !== null) {
    try {
      let payload: unknown = r.data;
      // 文档列表：取头几条避免 prompt 爆
      if (Array.isArray(payload) && payload.length > PREVIEW_MAX_DOCS) {
        payload = payload.slice(0, PREVIEW_MAX_DOCS);
      }
      let s = JSON.stringify(payload, null, 2);
      if (s.length > PREVIEW_MAX_CHARS) {
        s = s.slice(0, PREVIEW_MAX_CHARS) + `\n... (truncated, original ${s.length} chars)`;
      }
      previewJson = s;
    } catch {
      previewJson = '[unserializable]';
    }
  }
  return {
    ok: r.ok,
    kind: r.kind ?? null,
    operation: r.operation ?? null,
    count: typeof r.count === 'number' ? r.count : null,
    elapsedMs: typeof r.elapsedMs === 'number' ? r.elapsedMs : null,
    truncated: r.truncated ?? null,
    error: r.error ?? null,
    previewJson,
  };
}

/** 把编辑器命令裁到一个合理上限，避免动辄上千行的 paste 把上下文吃光 */
function trimCommand(input: string): string {
  const MAX = 2000;
  const trimmed = input.trim();
  if (trimmed.length <= MAX) return trimmed;
  return trimmed.slice(0, MAX) + `\n... (truncated, original ${trimmed.length} chars)`;
}

async function buildSchema(): Promise<LLMSchema | undefined> {
  // 哪怕没选 db，连接名 / 命令 / 上一次结果也能给 LLM 提供有用上下文
  const schema: LLMSchema = {};
  if (props.connectionName) schema.connectionName = props.connectionName;
  if (props.database) schema.database = props.database;
  if (props.collection) schema.collection = props.collection;

  if (includeCommand.value && hasUserCommand.value) {
    schema.currentCommand = trimCommand(props.currentCommand!);
  }
  if (includeResult.value && hasMeaningfulResult.value && props.lastResult) {
    schema.lastResult = buildResultPreview(props.lastResult);
  }

  if (includeSample.value && props.uri && props.database && props.collection) {
    try {
      const r = await sampleDocuments(props.uri, props.database, props.collection, 2);
      if (r.ok && Array.isArray(r.docs)) {
        schema.sampleDocs = r.docs;
      }
    } catch {
      /* sample 失败不阻塞 */
    }
  }

  // 没有任何字段就当 undefined，让 Rust 走「无 schema 提示」分支
  const hasAny = Object.keys(schema).length > 0;
  return hasAny ? schema : undefined;
}

async function sendMessage(rawInput?: string) {
  const text = (rawInput ?? input.value).trim();
  if (!text) return;
  if (loading.value) return;
  chat.addUser(text);
  input.value = '';
  loading.value = true;
  try {
    const schema = await buildSchema();
    const history = chat.messages.value
      .filter((m) => m.role === 'user' || m.role === 'assistant')
      .map((m) => ({ role: m.role as 'user' | 'assistant', content: m.content }));
    const r = await chatWithLLM(history, schema, llm.active.value, currentLocale.value);
    if (r.ok && r.reply) {
      chat.addAssistant(r.reply);
    } else {
      const err = r.error || '';
      chat.addAssistant(t('chat.replyFailed', { error: err }));
      ElMessage.error(err || 'LLM error');
    }
  } catch (e: any) {
    const err = e?.message || String(e);
    chat.addAssistant(t('chat.requestException', { error: err }));
    ElMessage.error(err);
  } finally {
    loading.value = false;
  }
}

async function generateFromPrompt() {
  const text = input.value.trim();
  if (!text) {
    ElMessage.warning(t('chat.needInput'));
    return;
  }
  if (loading.value) return;
  chat.addUser(`/generate ${text}`);
  input.value = '';
  loading.value = true;
  try {
    const schema = await buildSchema();
    const r = await generateMongoCommand(text, schema, llm.active.value, currentLocale.value);
    if (r.ok && r.command) {
      chat.addAssistant(`${t('chat.generated')}\n\n\`\`\`js\n${r.command}\n\`\`\``);
    } else {
      const err = r.error || '';
      chat.addAssistant(t('chat.generateFailed', { error: err }));
      ElMessage.error(err || 'LLM error');
    }
  } catch (e: any) {
    const err = e?.message || String(e);
    chat.addAssistant(t('chat.requestException', { error: err }));
    ElMessage.error(err);
  } finally {
    loading.value = false;
  }
}

/** 用户点击 assistant 消息里的「运行」按钮：评估影响 → （写操作）二次确认 → 让 App.vue 去跑 */
async function runFromAssistant(cmd: string) {
  if (loading.value) return;
  const formatted = formatMongoCommand(cmd);

  // 没有连接 / 没选 db 时，跳过评估，让 App.vue 给出统一的提示
  if (!props.uri || !props.database) {
    pendingFix.value = { command: formatted };
    emit('run-command', cmd);
    return;
  }

  // 1. 预评估影响范围
  let impact: ImpactInfo | null = null;
  try {
    impact = await estimateImpact(props.uri, props.database, cmd);
  } catch (e: any) {
    chat.addSystem(t('chat.impactEstimateFailed', { error: e?.message || String(e) }));
  }

  if (impact) {
    chat.addSystem(buildImpactMessage(impact));

    // 2. 写操作 → 二次确认
    if (impact.isWrite && impact.dangerLevel !== 'safe') {
      try {
        await ElMessageBox.confirm(
          buildImpactConfirmBody(impact),
          t('chat.impactConfirmTitle'),
          {
            type: impact.dangerLevel === 'danger' ? 'warning' : 'info',
            confirmButtonText: t('chat.impactConfirmRun'),
            cancelButtonText: t('chat.impactConfirmCancel'),
            confirmButtonClass:
              impact.dangerLevel === 'danger' ? 'el-button--danger' : '',
            autofocus: false,
          }
        );
      } catch {
        chat.addSystem(t('chat.impactCancelled'));
        return;
      }
    }
  }

  pendingFix.value = { command: formatted };
  emit('run-command', cmd);
}

/** 一行简洁的影响评估描述，用于 chat 里的 system 卡片 */
function buildImpactMessage(i: ImpactInfo): string {
  const op = i.operation;
  const col = i.collection;
  const matched = i.matchedEstimate;
  switch (i.affectKind) {
    case 'read':
      return t('chat.impactRead', { op, col });
    case 'insert':
      if (op === 'insertOne') return t('chat.impactInsertOne', { op, col });
      return t('chat.impactInsertMany', {
        op,
        col,
        n: i.insertCount ?? '?',
      });
    case 'updateSingle':
    case 'deleteSingle':
    case 'replaceSingle':
      if (matched == null) return t('chat.impactSingleUnknown', { op, col });
      return t('chat.impactSingle', { op, col, matched });
    case 'updateMulti':
    case 'deleteMulti':
      if (i.emptyFilter) return t('chat.impactEmptyFilter', { op, col, n: matched ?? '?' });
      if (matched == null) return t('chat.impactMultiUnknown', { op, col });
      return t('chat.impactMulti', { op, col, matched });
    default:
      return `${op} on \`${col}\``;
  }
}

/** 确认框正文 */
function buildImpactConfirmBody(i: ImpactInfo): string {
  const lines: string[] = [buildImpactMessage(i)];
  if (i.filterPreview) {
    lines.push(`${t('chat.impactFilterLabel')}: ${i.filterPreview}`);
  }
  if (i.emptyFilter && (i.affectKind === 'deleteMulti' || i.affectKind === 'updateMulti')) {
    lines.push(t('chat.impactEmptyFilterWarn'));
  }
  return lines.join('\n');
}

/** 成功结果摘要：在 chat 里以 system 卡片形式呈现 */
function buildSuccessSummary(r: ExecuteResult): string {
  const op = r.operation ?? '?';
  const col = r.collection ?? '?';
  const ms = typeof r.elapsedMs === 'number' ? r.elapsedMs : 0;
  const trunc = r.truncated === true;
  const data: any = r.data;
  switch (r.kind) {
    case 'documents': {
      const n = r.count ?? (Array.isArray(data) ? data.length : 0);
      if (trunc) return t('chat.summaryDocsTrunc', { op, col, n, ms });
      return t('chat.summaryDocs', { op, col, n, ms });
    }
    case 'document':
      if (data == null) return t('chat.summaryNoDoc', { op, col, ms });
      return t('chat.summaryDoc', { op, col, ms });
    case 'scalar':
      return t('chat.summaryScalar', { op, col, value: data, ms });
    case 'writeResult': {
      // insert
      if (op === 'insertOne') {
        const id = data?.insertedId
          ? typeof data.insertedId === 'object'
            ? JSON.stringify(data.insertedId)
            : String(data.insertedId)
          : '?';
        return t('chat.summaryInsertOne', { op, col, id, ms });
      }
      if (op === 'insertMany') {
        const ids = data?.insertedIds ?? {};
        const n = typeof ids === 'object' && ids !== null ? Object.keys(ids).length : 0;
        return t('chat.summaryInsertMany', { op, col, n, ms });
      }
      // delete
      if (op === 'deleteOne' || op === 'deleteMany') {
        const n = data?.deletedCount ?? 0;
        return t('chat.summaryDelete', { op, col, n, ms });
      }
      // update / replace
      const matched = data?.matchedCount ?? 0;
      const modified = data?.modifiedCount ?? 0;
      const upsertedId = data?.upsertedId;
      const upserted = upsertedId != null && upsertedId !== false;
      if (upserted) return t('chat.summaryUpdateUpsert', { op, col, ms });
      if (op === 'replaceOne' || op === 'findOneAndReplace') {
        return t('chat.summaryReplace', { op, col, matched, modified, ms });
      }
      return t('chat.summaryUpdate', { op, col, matched, modified, ms });
    }
    default:
      return t('chat.summaryGeneric', { op, col, ms });
  }
}

/**
 * 让 LLM 基于「失败的命令 + 错误 + 当前上下文」给出诊断和修复方案。
 * 修复命令通过 ```js``` 代码块返回，会被 `extractCommand` 识别，
 * 现有的「使用 / 运行」按钮就能让用户选择是否继续执行。
 */
async function analyzeFailure(failedCommand: string, errorText: string) {
  if (loading.value) return;
  if (llmAvailable.value === false) return;
  // 合成的 user 提示，连同已有 history 一起送出
  const synthetic = t('chat.autoFixPrompt', {
    command: failedCommand,
    error: errorText || 'unknown error',
  });
  chat.addUser(synthetic);
  loading.value = true;
  try {
    const schema = await buildSchema();
    const history = chat.messages.value
      .filter((m) => m.role === 'user' || m.role === 'assistant')
      .map((m) => ({ role: m.role as 'user' | 'assistant', content: m.content }));
    const r = await chatWithLLM(history, schema, llm.active.value, currentLocale.value);
    if (r.ok && r.reply) {
      chat.addAssistant(r.reply);
    } else {
      const err = r.error || '';
      chat.addAssistant(t('chat.replyFailed', { error: err }));
      ElMessage.error(err || 'LLM error');
    }
  } catch (e: any) {
    const err = e?.message || String(e);
    chat.addAssistant(t('chat.requestException', { error: err }));
    ElMessage.error(err);
  } finally {
    loading.value = false;
  }
}

// 监听结果回来。仅当结果对应「我们刚才让它运行的那条命令」时才处理。
watch(
  () => props.lastResult,
  (newR) => {
    if (!newR) return;
    if (!pendingFix.value) return;
    const expected = pendingFix.value.command.trim();
    const actual = (props.lastExecutedCommand ?? '').trim();
    // 不是我们这次 chat-run 的结果（比如用户穿插点了编辑器的运行）就丢掉等待状态
    if (expected !== actual) {
      pendingFix.value = null;
      return;
    }
    const cmd = pendingFix.value.command;
    pendingFix.value = null;
    if (newR.ok) {
      chat.addSystem(buildSuccessSummary(newR));
      return;
    }
    if (!autoFixOnError.value) return;
    analyzeFailure(cmd, newR.error || '');
  }
);

function onKeydown(ev: KeyboardEvent) {
  if (ev.key === 'Enter' && !ev.shiftKey && !ev.isComposing) {
    ev.preventDefault();
    if (ev.metaKey || ev.ctrlKey) {
      generateFromPrompt();
    } else {
      sendMessage();
    }
  }
}

function pickQuick(p: string) {
  input.value = p;
}

const hasMessages = computed(() => chat.messages.value.length > 0);

async function startNewChat() {
  // 当前空着就不弹确认，直接重置输入并提示
  if (!hasMessages.value) {
    input.value = '';
    ElMessage.success(t('chat.newChatStarted'));
    return;
  }
  try {
    await ElMessageBox.confirm(
      t('chat.newChatConfirmBody'),
      t('chat.newChatConfirmTitle'),
      {
        confirmButtonText: t('chat.newChatConfirmOk'),
        cancelButtonText: t('chat.newChatConfirmCancel'),
        type: 'warning',
        autofocus: false,
      }
    );
  } catch {
    return;
  }
  chat.clear();
  input.value = '';
  ElMessage.success(t('chat.newChatStarted'));
}

function renderContent(text: string): string {
  // 简单 markdown 处理：```code``` 块 + 内联 `code`
  const esc = (s: string) => s.replace(/[&<>]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;' }[c]!));
  return esc(text)
    .replace(/```([a-z0-9_-]*)\n([\s\S]*?)```/gi, (_m, _lang, code) => `<pre class="cb mono">${code}</pre>`)
    .replace(/`([^`\n]+)`/g, '<code class="ic mono">$1</code>')
    .replace(/\n/g, '<br/>');
}
</script>

<template>
  <div class="chat-panel">
    <div class="chat-head">
      <span class="title">🤖 {{ t('chat.title') }}</span>
      <span :class="['status', llmAvailable ? 'ok' : 'off']" :title="llmInfo">
        {{ llmAvailable === null ? '...' : llmAvailable ? llmInfo : t('chat.disabled') }}
      </span>
      <span v-if="hasMessages" class="turn-badge" :title="t('chat.turnsTitle', { n: chat.messages.value.length })">
        {{ chat.messages.value.length }}
      </span>
      <div class="spacer" />
      <button class="new-chat-btn" :title="t('chat.newChatTitle')" @click="startNewChat">
        <span class="ic">✨</span>
        <span class="lbl">{{ t('chat.newChat') }}</span>
      </button>
    </div>

    <div ref="scrollAreaRef" class="messages">
      <div v-if="!hasMessages" class="welcome">
        <p class="hi">{{ t('chat.welcomeTitle') }}</p>
        <p class="sub">{{ t('chat.welcomeSub') }}</p>
        <div class="quick-list">
          <button v-for="(p, i) in quickPrompts" :key="i" class="quick" @click="pickQuick(p)">
            {{ p }}
          </button>
        </div>
        <div class="ctx-info">
          <div>{{ t('chat.ctxDb') }}: <span class="mono">{{ database || t('chat.ctxNone') }}</span></div>
          <div>{{ t('chat.ctxCol') }}: <span class="mono">{{ collection || t('chat.ctxNone') }}</span></div>
        </div>
      </div>

      <template v-else>
        <div
          v-for="m in chat.messages.value"
          :key="m.id"
          :class="['msg', m.role]"
        >
          <div v-if="m.role !== 'system'" class="role-line">
            <span class="role-tag">{{ m.role === 'user' ? t('chat.roleUser') : t('chat.roleAi') }}</span>
          </div>
          <div class="bubble" v-html="renderContent(m.content)" />
          <div v-if="m.role === 'assistant' && m.command" class="cmd-actions">
            <span class="cmd-label">{{ t('chat.cmdDetected') }}</span>
            <button class="cmd-btn" @click="emit('use-command', m.command!)">{{ t('chat.cmdUse') }}</button>
            <button class="cmd-btn primary" @click="runFromAssistant(m.command!)">{{ t('chat.cmdRun') }}</button>
          </div>
        </div>
        <div v-if="loading" class="msg assistant">
          <div class="role-line">
            <span class="role-tag">{{ t('chat.roleAi') }}</span>
          </div>
          <div class="bubble typing">
            <span /><span /><span />
          </div>
        </div>
      </template>
    </div>

    <div class="input-area">
      <div class="opts">
        <label class="opt">
          <input v-model="includeSample" type="checkbox" />
          {{ t('chat.optSampleSchema') }}
        </label>
        <label class="opt" :title="t('chat.optIncludeCommandTitle')">
          <input v-model="includeCommand" type="checkbox" :disabled="!hasUserCommand" />
          {{ t('chat.optIncludeCommand') }}
        </label>
        <label class="opt" :title="t('chat.optIncludeResultTitle')">
          <input v-model="includeResult" type="checkbox" :disabled="!hasMeaningfulResult" />
          {{ t('chat.optIncludeResult') }}
        </label>
        <label class="opt" :title="t('chat.optAutoFixTitle')">
          <input v-model="autoFixOnError" type="checkbox" />
          {{ t('chat.optAutoFix') }}
        </label>
      </div>
      <textarea
        v-model="input"
        class="input mono"
        :placeholder="t('chat.inputPh')"
        rows="3"
        @keydown="onKeydown"
      />
      <div class="actions">
        <button class="t-btn" :disabled="loading || !input.trim()" @click="sendMessage()">
          {{ t('chat.btnChat') }}
        </button>
        <button class="t-btn primary" :disabled="loading || !input.trim()" @click="generateFromPrompt">
          {{ t('chat.btnGenerate') }}
        </button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.chat-panel {
  display: flex;
  flex-direction: column;
  height: 100%;
  background: var(--panel);
  border-left: 1px solid var(--border);
}
.chat-head {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 8px 12px;
  background: var(--panel-2);
  border-bottom: 1px solid var(--border);
  flex-shrink: 0;
}
.title {
  font-weight: 700;
  font-size: 13px;
  color: var(--text);
}
.status {
  font-size: 10.5px;
  padding: 2px 6px;
  border-radius: 9px;
  font-weight: 600;
}
.status.ok {
  background: var(--active);
  color: var(--accent);
}
.status.off {
  background: var(--kbd-bg);
  color: var(--text-mute);
}
.spacer {
  flex: 1;
}
.ic-btn {
  background: transparent;
  border: 1px solid var(--border);
  border-radius: 4px;
  padding: 2px 8px;
  font-size: 12px;
  cursor: pointer;
  color: var(--text-dim);
}
.ic-btn:hover:not(:disabled) {
  background: var(--hover);
  color: var(--text);
}
.ic-btn:disabled {
  opacity: 0.4;
  cursor: not-allowed;
}
.turn-badge {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  min-width: 18px;
  height: 18px;
  padding: 0 5px;
  border-radius: 9px;
  background: var(--panel-3, var(--hover));
  border: 1px solid var(--border);
  color: var(--text-dim);
  font-size: 11px;
  line-height: 1;
  user-select: none;
}
.new-chat-btn {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  padding: 3px 9px;
  font-size: 12px;
  line-height: 1.4;
  background: transparent;
  color: var(--text);
  border: 1px solid var(--border);
  border-radius: 4px;
  cursor: pointer;
  transition: background-color 0.12s, border-color 0.12s;
}
.new-chat-btn:hover {
  background: var(--hover);
  border-color: var(--primary, var(--text-dim));
}
.new-chat-btn .ic {
  font-size: 13px;
  line-height: 1;
}
.new-chat-btn .lbl {
  font-weight: 500;
}

.messages {
  flex: 1;
  overflow: auto;
  padding: 10px 12px;
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.welcome {
  text-align: center;
  padding: 18px 6px;
  color: var(--text-dim);
}
.welcome .hi {
  font-size: 14px;
  font-weight: 700;
  color: var(--text);
  margin: 0 0 6px;
}
.welcome .sub {
  font-size: 11.5px;
  line-height: 1.55;
  margin: 0 0 16px;
}
.quick-list {
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.quick {
  background: var(--panel-2);
  border: 1px solid var(--border);
  border-radius: 6px;
  padding: 6px 10px;
  font-size: 11.5px;
  color: var(--text-dim);
  cursor: pointer;
  text-align: left;
}
.quick:hover {
  background: var(--hover);
  color: var(--text);
  border-color: var(--border-strong);
}
.ctx-info {
  margin-top: 16px;
  padding: 8px 10px;
  font-size: 11px;
  color: var(--text-mute);
  background: var(--panel-2);
  border-radius: 6px;
  text-align: left;
}
.ctx-info > div {
  margin-bottom: 2px;
}

.msg {
  display: flex;
  flex-direction: column;
}
.msg.user {
  align-items: flex-end;
}
.msg.assistant {
  align-items: flex-start;
}
.msg.system {
  align-items: stretch;
}
.role-line {
  font-size: 10px;
  color: var(--text-mute);
  margin-bottom: 3px;
  letter-spacing: 0.4px;
}
.role-tag {
  font-weight: 700;
  text-transform: uppercase;
}
.bubble {
  max-width: 92%;
  padding: 8px 11px;
  border-radius: 10px;
  font-size: 12.5px;
  line-height: 1.55;
  word-break: break-word;
  white-space: normal;
}
.msg.user .bubble {
  background: var(--accent);
  color: white;
  border-bottom-right-radius: 3px;
}
.msg.assistant .bubble {
  background: var(--panel-2);
  color: var(--text);
  border-bottom-left-radius: 3px;
  border: 1px solid var(--border);
}
.msg.system .bubble {
  background: var(--panel-2);
  color: var(--text-dim);
  border: 1px dashed var(--border);
  border-radius: 6px;
  font-size: 12px;
  padding: 6px 10px;
  white-space: pre-wrap;
  align-self: stretch;
  max-width: 100%;
}
.bubble :deep(.cb) {
  display: block;
  margin: 6px 0;
  padding: 8px 10px;
  background: rgba(0, 0, 0, 0.06);
  border-radius: 5px;
  font-size: 11.5px;
  white-space: pre-wrap;
  word-break: break-word;
  color: var(--tok-str);
}
.msg.user .bubble :deep(.cb) {
  background: rgba(255, 255, 255, 0.15);
  color: white;
}
.bubble :deep(.ic) {
  background: rgba(0, 0, 0, 0.06);
  padding: 1px 5px;
  border-radius: 3px;
  font-size: 11.5px;
}
.msg.user .bubble :deep(.ic) {
  background: rgba(255, 255, 255, 0.15);
}

.typing {
  display: flex;
  align-items: center;
  gap: 4px;
  height: 22px;
  padding: 0 14px;
}
.typing span {
  display: inline-block;
  width: 6px;
  height: 6px;
  background: var(--text-mute);
  border-radius: 50%;
  animation: typing 1s infinite;
}
.typing span:nth-child(2) {
  animation-delay: 0.15s;
}
.typing span:nth-child(3) {
  animation-delay: 0.3s;
}
@keyframes typing {
  0%, 60%, 100% {
    transform: translateY(0);
    opacity: 0.4;
  }
  30% {
    transform: translateY(-4px);
    opacity: 1;
  }
}

.cmd-actions {
  margin-top: 6px;
  display: flex;
  align-items: center;
  gap: 6px;
  flex-wrap: wrap;
}
.cmd-label {
  font-size: 10.5px;
  color: var(--text-mute);
}
.cmd-btn {
  background: var(--panel);
  border: 1px solid var(--accent);
  border-radius: 4px;
  padding: 3px 10px;
  font-size: 11px;
  cursor: pointer;
  color: var(--accent);
  font-weight: 600;
}
.cmd-btn:hover {
  background: var(--active);
}
.cmd-btn.primary {
  background: var(--accent);
  color: white;
}
.cmd-btn.primary:hover {
  background: var(--accent-2);
  border-color: var(--accent-2);
}

.input-area {
  flex-shrink: 0;
  border-top: 1px solid var(--border);
  background: var(--panel-2);
  padding: 8px 10px;
}
.opts {
  display: flex;
  font-size: 11px;
  color: var(--text-mute);
  margin-bottom: 6px;
}
.opt {
  display: flex;
  align-items: center;
  gap: 4px;
  cursor: pointer;
}
.opt input {
  cursor: pointer;
}
.input {
  width: 100%;
  border: 1px solid var(--border);
  border-radius: 6px;
  background: var(--panel);
  padding: 8px 10px;
  color: var(--text);
  font-size: 12.5px;
  font-family: var(--mono);
  resize: vertical;
  outline: none;
  transition: border-color 0.15s;
}
.input:focus {
  border-color: var(--accent);
}
.actions {
  margin-top: 6px;
  display: flex;
  gap: 6px;
  justify-content: flex-end;
}
.t-btn {
  background: var(--panel);
  border: 1px solid var(--border);
  border-radius: 4px;
  padding: 5px 14px;
  font-size: 12px;
  cursor: pointer;
  color: var(--text);
}
.t-btn:hover:not(:disabled) {
  background: var(--hover);
  border-color: var(--border-strong);
}
.t-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}
.t-btn.primary {
  background: var(--accent);
  border-color: var(--accent);
  color: white;
  font-weight: 600;
}
.t-btn.primary:hover:not(:disabled) {
  background: var(--accent-2);
  border-color: var(--accent-2);
}
</style>
