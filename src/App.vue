<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import { ElMessage } from 'element-plus';
import { useI18n } from 'vue-i18n';
import SidebarMenu from './components/SidebarMenu.vue';
import QueryEditor from './components/QueryEditor.vue';
import ResultPanel from './components/ResultPanel.vue';
import ChatPanel from './components/ChatPanel.vue';
import ThemeSwitcher from './components/ThemeSwitcher.vue';
import LanguageSwitcher from './components/LanguageSwitcher.vue';
import { currentElementLocale } from './i18n';
import { useConnections } from './composables/useConnections';
import { useHistory, type HistoryItem } from './composables/useHistory';
import { executeMongoCommand, type ExecuteResult } from './api/mongo';

const { t } = useI18n();
const conns = useConnections();
const history = useHistory();

const currentDatabase = ref<string | null>(null);
const currentCollection = ref<string | null>(null);

/**
 * 初始命令是「占位说明」。语言切换时若用户还没动过它（还等于旧 placeholder），
 * 就同步切到新语言；只要用户改过任何字符，就尊重用户输入，不再覆盖。
 */
const command = ref<string>(t('editor.placeholder'));
let lastPlaceholder = command.value;
watch(
  () => t('editor.placeholder'),
  (next) => {
    if (command.value === lastPlaceholder) command.value = next;
    lastPlaceholder = next;
  }
);
const loading = ref(false);
const result = ref<ExecuteResult | null>(null);
/**
 * Tauri 的 invoke 没有 AbortSignal；我们用一个递增的 token 来识别「最新的一次请求」，
 * 用户重复点击执行时，旧的结果会被新的覆盖（旧 promise 完成时发现 token 不匹配就丢弃）。
 */
let runToken = 0;

/* ---------- 中间栏上下拆分 ---------- */
const EDITOR_HEIGHT_KEY = 'mongodb-runner:editor-height';
const MIN_EDITOR_HEIGHT = 120;
const MAX_EDITOR_HEIGHT = 900;
const DEFAULT_EDITOR_HEIGHT = 260;

function loadEditorHeight(): number {
  try {
    const raw = localStorage.getItem(EDITOR_HEIGHT_KEY);
    const n = raw ? Number(raw) : NaN;
    if (Number.isFinite(n) && n >= MIN_EDITOR_HEIGHT) return n;
  } catch {
    /* ignore */
  }
  return DEFAULT_EDITOR_HEIGHT;
}

const editorHeight = ref<number>(loadEditorHeight());

function persistEditorHeight() {
  try {
    localStorage.setItem(EDITOR_HEIGHT_KEY, String(editorHeight.value));
  } catch {
    /* ignore */
  }
}

function onSplitterMouseDown(ev: MouseEvent) {
  ev.preventDefault();
  const startY = ev.clientY;
  const startH = editorHeight.value;
  function onMove(e: MouseEvent) {
    const delta = e.clientY - startY;
    const next = Math.max(MIN_EDITOR_HEIGHT, Math.min(MAX_EDITOR_HEIGHT, startH + delta));
    editorHeight.value = next;
  }
  function onUp() {
    document.removeEventListener('mousemove', onMove);
    document.removeEventListener('mouseup', onUp);
    document.body.style.userSelect = '';
    document.body.style.cursor = '';
    persistEditorHeight();
  }
  document.body.style.userSelect = 'none';
  document.body.style.cursor = 'row-resize';
  document.addEventListener('mousemove', onMove);
  document.addEventListener('mouseup', onUp);
}

/* ---------- LLM 命令写入编辑器后的闪烁提示 ---------- */
const editorFlash = ref(false);
let flashTimer: ReturnType<typeof setTimeout> | null = null;

function flashEditor() {
  editorFlash.value = false;
  if (flashTimer) clearTimeout(flashTimer);
  // 下一帧再 set true，确保动画能重新触发
  requestAnimationFrame(() => {
    editorFlash.value = true;
    flashTimer = setTimeout(() => {
      editorFlash.value = false;
    }, 800);
  });
}

const hasConnection = computed(() => !!conns.active.value);

const headerInfo = computed(() => {
  const c = conns.active.value;
  if (!c) return t('app.notConnected');
  const parts = [c.name];
  if (currentDatabase.value) parts.push(currentDatabase.value);
  if (currentCollection.value) parts.push(currentCollection.value);
  return parts.join(' / ');
});

function handlePickCollection(payload: { database: string; collection: string }) {
  currentDatabase.value = payload.database;
  currentCollection.value = payload.collection;
  command.value = `db.${payload.collection}.find({}).limit(50)`;
}

function handlePickDatabase(database: string) {
  currentDatabase.value = database;
}

function handlePickHistory(item: HistoryItem) {
  if (item.database) currentDatabase.value = item.database;
  command.value = item.command;
}

async function handleRun() {
  if (loading.value) return;
  const active = conns.active.value;
  if (!active) {
    ElMessage.warning(t('msg.pickConnFirst'));
    return;
  }
  if (!currentDatabase.value) {
    ElMessage.warning(t('msg.pickDbFirst'));
    return;
  }
  const cmd = command.value.trim();
  if (!cmd) {
    ElMessage.warning(t('msg.emptyCommand'));
    return;
  }

  const myToken = ++runToken;
  loading.value = true;

  try {
    const r = await executeMongoCommand(active.uri, currentDatabase.value, cmd, 1000);
    if (myToken !== runToken) return;
    result.value = r;
    history.record({
      connectionId: active.id,
      connectionName: active.name,
      database: currentDatabase.value,
      command: cmd,
      ok: r.ok,
      error: r.error,
      count: r.count,
      elapsedMs: r.elapsedMs,
    });
    if (!r.ok) ElMessage.error(r.error || t('msg.queryFailed'));
  } catch (e: any) {
    if (myToken !== runToken) return;
    const msg = e?.message || String(e);
    ElMessage.error(t('msg.requestFailed', { msg }));
    result.value = { ok: false, error: msg };
    history.record({
      connectionId: active.id,
      connectionName: active.name,
      database: currentDatabase.value,
      command: cmd,
      ok: false,
      error: msg,
    });
  } finally {
    if (myToken === runToken) loading.value = false;
  }
}

function handleStop() {
  /**
   * Tauri 的 invoke 无法真正中断已经发出的 IPC，但我们可以让 UI 立刻回到 idle，
   * 并通过 runToken 让正在跑的请求结果被丢弃。
   */
  runToken++;
  loading.value = false;
}

function handleUseGenerated(cmd: string) {
  command.value = cmd;
  flashEditor();
  ElMessage.success(t('msg.filled'));
}

async function handleRunGenerated(cmd: string) {
  // 先把命令写入编辑器并触发一次闪烁，让用户清楚看到「右侧的命令进了中间编辑器」
  command.value = cmd;
  flashEditor();
  await handleRun();
}
</script>

<template>
  <el-config-provider :locale="currentElementLocale">
    <div class="app-shell">
      <header class="app-header">
        <div class="brand">
          <span class="brand-icon">🍃</span>
          <span class="brand-title">{{ t('app.title') }}</span>
          <span class="brand-sub">{{ headerInfo }}</span>
        </div>
        <div class="header-meta">
          <a
            class="repo-link"
            href="https://www.mongodb.com/docs/manual/reference/operator/query/"
            target="_blank"
            rel="noopener noreferrer"
            >{{ t('app.docsLink') }} ↗</a
          >
          <LanguageSwitcher />
          <ThemeSwitcher />
        </div>
      </header>

      <main class="app-main">
        <aside class="pane pane-left">
          <SidebarMenu
            :current-database="currentDatabase"
            :current-collection="currentCollection"
            @pick-database="handlePickDatabase"
            @pick-collection="handlePickCollection"
            @pick-history="handlePickHistory"
          />
        </aside>

        <div class="divider" />

        <section class="pane pane-center">
          <div
            class="center-top"
            :class="{ 'is-flash': editorFlash }"
            :style="{ height: editorHeight + 'px' }"
          >
            <QueryEditor
              v-model="command"
              :loading="loading"
              :can-run="hasConnection && !!currentDatabase"
              @run="handleRun"
              @stop="handleStop"
            />
          </div>
          <div
            class="h-splitter"
            role="separator"
            aria-orientation="horizontal"
            @mousedown="onSplitterMouseDown"
            @dblclick="(editorHeight = DEFAULT_EDITOR_HEIGHT), persistEditorHeight()"
          >
            <span class="grip" />
          </div>
          <div class="center-bottom">
            <ResultPanel :result="result" :loading="loading" />
          </div>
        </section>

        <div class="divider" />

        <aside class="pane pane-right">
          <ChatPanel
            :database="currentDatabase"
            :collection="currentCollection"
            :uri="conns.active.value?.uri || null"
            @use-command="handleUseGenerated"
            @run-command="handleRunGenerated"
          />
        </aside>
      </main>
    </div>
  </el-config-provider>
</template>

<style scoped>
.app-shell {
  display: flex;
  flex-direction: column;
  height: 100%;
  background: var(--bg);
}

.app-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 10px 18px;
  border-bottom: 1px solid var(--border);
  background: linear-gradient(180deg, var(--header-from) 0%, var(--header-to) 100%);
}

.brand {
  display: flex;
  align-items: baseline;
  gap: 10px;
  min-width: 0;
}
.brand-icon {
  font-size: 18px;
}
.brand-title {
  font-weight: 700;
  font-size: 16px;
  letter-spacing: 0.3px;
  background: linear-gradient(90deg, var(--brand-from), var(--brand-to));
  -webkit-background-clip: text;
  background-clip: text;
  color: transparent;
}
.brand-sub {
  color: var(--text-dim);
  font-size: 12px;
  max-width: 480px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-family: var(--mono);
}

.header-meta {
  display: flex;
  align-items: center;
  gap: 12px;
}
.repo-link {
  color: var(--text-dim);
  text-decoration: none;
  font-size: 12px;
}
.repo-link:hover {
  color: var(--text);
}

.app-main {
  flex: 1;
  display: flex;
  min-height: 0;
}
.pane {
  display: flex;
  flex-direction: column;
  overflow: hidden;
  min-width: 0;
}
.pane-left {
  width: 280px;
  flex-shrink: 0;
  background: var(--panel);
}
.pane-center {
  flex: 1;
  min-width: 0;
}
.pane-right {
  width: 360px;
  flex-shrink: 0;
  background: var(--panel);
}
.divider {
  width: 1px;
  background: var(--border);
  flex-shrink: 0;
}

/* ---- 中间栏上下拆分 ---- */
.center-top {
  position: relative;
  flex-shrink: 0;
  display: flex;
  min-height: 0;
  overflow: hidden;
}
.center-top::after {
  content: '';
  position: absolute;
  inset: 0;
  pointer-events: none;
  border-radius: 0;
  box-shadow: inset 0 0 0 0 transparent;
  transition: box-shadow 0.18s ease;
}
.center-top.is-flash::after {
  animation: editor-flash 0.8s ease;
}
@keyframes editor-flash {
  0% { box-shadow: inset 0 0 0 2px var(--accent), 0 0 0 0 rgba(45, 186, 135, 0); }
  35% { box-shadow: inset 0 0 0 2px var(--accent), 0 0 18px 4px rgba(45, 186, 135, 0.35); }
  100% { box-shadow: inset 0 0 0 0 transparent, 0 0 0 0 rgba(45, 186, 135, 0); }
}
.center-bottom {
  flex: 1;
  display: flex;
  min-height: 0;
  overflow: hidden;
}

.h-splitter {
  height: 6px;
  flex-shrink: 0;
  cursor: row-resize;
  background: var(--panel-2);
  border-top: 1px solid var(--border);
  border-bottom: 1px solid var(--border);
  display: flex;
  align-items: center;
  justify-content: center;
  user-select: none;
  transition: background 0.15s;
}
.h-splitter:hover {
  background: var(--hover-strong);
}
.h-splitter:active {
  background: var(--active);
}
.h-splitter .grip {
  display: block;
  width: 32px;
  height: 2px;
  border-radius: 2px;
  background: var(--text-faint);
  transition: background 0.15s;
}
.h-splitter:hover .grip {
  background: var(--accent);
}
</style>
