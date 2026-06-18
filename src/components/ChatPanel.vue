<script setup lang="ts">
import { computed, nextTick, onMounted, ref, watch } from 'vue';
import { ElMessage } from 'element-plus';
import { chatWithLLM, generateMongoCommand, getLLMStatus, type LLMSchema } from '../api/llm';
import { sampleDocuments } from '../api/mongo';
import { useChat } from '../composables/useChat';

const props = defineProps<{
  uri: string | null;
  database: string | null;
  collection: string | null;
}>();

const emit = defineEmits<{
  (e: 'use-command', cmd: string): void;
  (e: 'run-command', cmd: string): void;
}>();

const chat = useChat();
const input = ref('');
const loading = ref(false);
const includeSample = ref(true);
const llmAvailable = ref<boolean | null>(null);
const llmInfo = ref<string>('');

const scrollAreaRef = ref<HTMLElement | null>(null);

const QUICK_PROMPTS = [
  '统计每个 role 的用户数量',
  '查找最近 7 天注册的活跃用户，按 createdAt 倒序',
  '导出 status 为 pending 且 priority >= 3 的工单',
  '聚合每天的下单总额（最近 30 天）',
];

onMounted(async () => {
  try {
    const r = await getLLMStatus();
    llmAvailable.value = r.available;
    if (r.available) {
      const oai = r.providers?.openai;
      const cur = r.providers?.cursor;
      if (oai) llmInfo.value = `OpenAI · ${oai.model}`;
      else if (cur) llmInfo.value = `cursor-agent · ${cur.model || 'default'}`;
    } else {
      llmInfo.value = '未配置 LLM。设置 OPENAI_API_KEY 后重启即可启用';
    }
  } catch {
    llmAvailable.value = false;
    llmInfo.value = '无法读取 LLM 状态';
  }
  scrollToBottom();
});

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

async function buildSchema(): Promise<LLMSchema | undefined> {
  if (!props.database) return undefined;
  const schema: LLMSchema = {
    database: props.database,
    collection: props.collection,
  };
  if (includeSample.value && props.uri && props.collection) {
    try {
      const r = await sampleDocuments(props.uri, props.database, props.collection, 2);
      if (r.ok && Array.isArray(r.docs)) {
        schema.sampleDocs = r.docs;
      }
    } catch {
      /* sample 失败不阻塞 */
    }
  }
  return schema;
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
    const r = await chatWithLLM(history, schema);
    if (r.ok && r.reply) {
      chat.addAssistant(r.reply);
    } else {
      chat.addAssistant(`(LLM 返回失败: ${r.error || '未知错误'})`);
      ElMessage.error(r.error || 'LLM 调用失败');
    }
  } catch (e: any) {
    chat.addAssistant(`(请求异常: ${e?.message || String(e)})`);
    ElMessage.error(e?.message || String(e));
  } finally {
    loading.value = false;
  }
}

async function generateFromPrompt() {
  const text = input.value.trim();
  if (!text) {
    ElMessage.warning('请输入需求');
    return;
  }
  if (loading.value) return;
  chat.addUser(`/generate ${text}`);
  input.value = '';
  loading.value = true;
  try {
    const schema = await buildSchema();
    const r = await generateMongoCommand(text, schema);
    if (r.ok && r.command) {
      chat.addAssistant(`生成的命令：\n\n\`\`\`js\n${r.command}\n\`\`\``);
    } else {
      chat.addAssistant(`(生成失败: ${r.error || '未知错误'})`);
      ElMessage.error(r.error || 'LLM 调用失败');
    }
  } catch (e: any) {
    chat.addAssistant(`(请求异常: ${e?.message || String(e)})`);
    ElMessage.error(e?.message || String(e));
  } finally {
    loading.value = false;
  }
}

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
      <span class="title">🤖 AI 助手</span>
      <span :class="['status', llmAvailable ? 'ok' : 'off']" :title="llmInfo">
        {{ llmAvailable === null ? '...' : llmAvailable ? llmInfo : '未启用' }}
      </span>
      <div class="spacer" />
      <button class="ic-btn" :disabled="!hasMessages" title="清空对话" @click="chat.clear()">
        🗑
      </button>
    </div>

    <div ref="scrollAreaRef" class="messages">
      <div v-if="!hasMessages" class="welcome">
        <p class="hi">问 AI 帮你生成 MongoDB 查询</p>
        <p class="sub">
          说出你想要的数据，例如：「找出今天注册且未付费的用户」。
          回车发送（自由对话）；⌘/Ctrl + 回车（强约束生成命令）。
        </p>
        <div class="quick-list">
          <button v-for="(p, i) in QUICK_PROMPTS" :key="i" class="quick" @click="pickQuick(p)">
            {{ p }}
          </button>
        </div>
        <div class="ctx-info">
          <div>当前数据库: <span class="mono">{{ database || '(未选择)' }}</span></div>
          <div>默认集合: <span class="mono">{{ collection || '(未选择)' }}</span></div>
        </div>
      </div>

      <template v-else>
        <div
          v-for="m in chat.messages.value"
          :key="m.id"
          :class="['msg', m.role]"
        >
          <div class="role-line">
            <span class="role-tag">{{ m.role === 'user' ? '你' : 'AI' }}</span>
          </div>
          <div class="bubble" v-html="renderContent(m.content)" />
          <div v-if="m.role === 'assistant' && m.command" class="cmd-actions">
            <span class="cmd-label">检测到可执行命令:</span>
            <button class="cmd-btn" @click="emit('use-command', m.command!)">填入编辑器</button>
            <button class="cmd-btn primary" @click="emit('run-command', m.command!)">直接执行</button>
          </div>
        </div>
        <div v-if="loading" class="msg assistant">
          <div class="role-line">
            <span class="role-tag">AI</span>
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
          采样文档作为 schema 提示
        </label>
      </div>
      <textarea
        v-model="input"
        class="input mono"
        placeholder="说出你想查询的数据……  Enter 对话，⌘/Ctrl + Enter 直接生成命令"
        rows="3"
        @keydown="onKeydown"
      />
      <div class="actions">
        <button class="t-btn" :disabled="loading || !input.trim()" @click="sendMessage()">
          💬 对话
        </button>
        <button class="t-btn primary" :disabled="loading || !input.trim()" @click="generateFromPrompt">
          ⚡ 生成命令
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
