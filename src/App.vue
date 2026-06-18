<script setup lang="ts">
import { computed, ref } from 'vue';
import { ElMessage } from 'element-plus';
import SidebarMenu from './components/SidebarMenu.vue';
import QueryEditor from './components/QueryEditor.vue';
import ResultPanel from './components/ResultPanel.vue';
import ChatPanel from './components/ChatPanel.vue';
import ThemeSwitcher from './components/ThemeSwitcher.vue';
import { useConnections } from './composables/useConnections';
import { useHistory, type HistoryItem } from './composables/useHistory';
import { executeMongoCommand, type ExecuteResult } from './api/mongo';

const conns = useConnections();
const history = useHistory();

const currentDatabase = ref<string | null>(null);
const currentCollection = ref<string | null>(null);

const command = ref<string>('// 选择左侧的集合即可自动填入示例\n// 然后按 Cmd/Ctrl + Enter 执行');
const loading = ref(false);
const result = ref<ExecuteResult | null>(null);
let abortController: AbortController | null = null;

const hasConnection = computed(() => !!conns.active.value);

const headerInfo = computed(() => {
  const c = conns.active.value;
  if (!c) return '未连接';
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
    ElMessage.warning('请先在左侧选择或新建一个连接');
    return;
  }
  if (!currentDatabase.value) {
    ElMessage.warning('请先在左侧选择数据库');
    return;
  }
  const cmd = command.value.trim();
  if (!cmd) {
    ElMessage.warning('命令不能为空');
    return;
  }

  if (abortController) abortController.abort();
  const controller = new AbortController();
  abortController = controller;
  loading.value = true;

  try {
    const r = await executeMongoCommand(
      active.uri,
      currentDatabase.value,
      cmd,
      1000,
      controller.signal
    );
    if (abortController !== controller) return;
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
    if (!r.ok) ElMessage.error(r.error || '查询失败');
  } catch (e: any) {
    if (e?.name === 'AbortError') return;
    if (abortController !== controller) return;
    const msg = e?.message || String(e);
    ElMessage.error(`请求失败: ${msg}`);
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
    if (abortController === controller) {
      abortController = null;
      loading.value = false;
    }
  }
}

function handleStop() {
  if (!abortController) return;
  abortController.abort();
  abortController = null;
  loading.value = false;
}

function handleUseGenerated(cmd: string) {
  command.value = cmd;
  ElMessage.success('已填入编辑器，按 ⌘/Ctrl + Enter 执行');
}

async function handleRunGenerated(cmd: string) {
  command.value = cmd;
  await handleRun();
}
</script>

<template>
  <div class="app-shell">
    <header class="app-header">
      <div class="brand">
        <span class="brand-icon">🍃</span>
        <span class="brand-title">MongoDB Runner</span>
        <span class="brand-sub">{{ headerInfo }}</span>
      </div>
      <div class="header-meta">
        <a
          class="repo-link"
          href="https://www.mongodb.com/docs/manual/reference/operator/query/"
          target="_blank"
          rel="noopener noreferrer"
          >query operators ↗</a
        >
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
        <QueryEditor
          v-model="command"
          :loading="loading"
          :can-run="hasConnection && !!currentDatabase"
          @run="handleRun"
          @stop="handleStop"
        />
        <ResultPanel :result="result" :loading="loading" />
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
</style>
