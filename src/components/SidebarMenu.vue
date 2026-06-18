<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import { ElMessage, ElMessageBox } from 'element-plus';
import ConnectionDialog from './ConnectionDialog.vue';
import { useConnections, type MongoConnection } from '../composables/useConnections';
import { useHistory, type HistoryItem } from '../composables/useHistory';
import { listCollections, listDatabases, type MongoCollection, type MongoDatabase } from '../api/mongo';

const props = defineProps<{
  currentDatabase: string | null;
  currentCollection: string | null;
}>();

const emit = defineEmits<{
  (e: 'pick-database', database: string): void;
  (e: 'pick-collection', payload: { database: string; collection: string }): void;
  (e: 'pick-history', item: HistoryItem): void;
}>();

const conns = useConnections();
const history = useHistory();

type TabKey = 'connections' | 'history';
const activeTab = ref<TabKey>('connections');

const dialogVisible = ref(false);
const editing = ref<MongoConnection | null>(null);

const loadingDb = ref(false);
const databases = ref<MongoDatabase[]>([]);
const expandedDb = ref<string | null>(null);
const collections = ref<Record<string, MongoCollection[]>>({}); // db -> collections

watch(
  () => conns.active.value?.id,
  async () => {
    databases.value = [];
    collections.value = {};
    expandedDb.value = null;
    if (!conns.active.value) return;
    await refreshDatabases();
    if (conns.active.value?.defaultDatabase) {
      await toggleDatabase(conns.active.value.defaultDatabase);
    }
  }
);

async function refreshDatabases() {
  const c = conns.active.value;
  if (!c) return;
  loadingDb.value = true;
  try {
    const r = await listDatabases(c.uri);
    if (r.ok) {
      databases.value = (r.databases || []).filter((d) => !['admin', 'config', 'local'].includes(d.name));
      // 也允许展示系统库，但放在末尾
      const sys = (r.databases || []).filter((d) => ['admin', 'config', 'local'].includes(d.name));
      databases.value = [...databases.value, ...sys];
    } else {
      ElMessage.error(`列库失败: ${r.error || '未知错误'}`);
    }
  } catch (e: any) {
    ElMessage.error(`列库失败: ${e?.message || String(e)}`);
  } finally {
    loadingDb.value = false;
  }
}

async function toggleDatabase(dbName: string) {
  if (expandedDb.value === dbName) {
    expandedDb.value = null;
    return;
  }
  expandedDb.value = dbName;
  emit('pick-database', dbName);
  if (collections.value[dbName]) return;
  const c = conns.active.value;
  if (!c) return;
  try {
    const r = await listCollections(c.uri, dbName);
    if (r.ok) {
      collections.value[dbName] = (r.collections || []).filter((c) => !c.name.startsWith('system.'));
    } else {
      ElMessage.error(`列集合失败: ${r.error || '未知错误'}`);
    }
  } catch (e: any) {
    ElMessage.error(`列集合失败: ${e?.message || String(e)}`);
  }
}

function pickCollection(db: string, col: string) {
  emit('pick-collection', { database: db, collection: col });
}

function openAdd() {
  editing.value = null;
  dialogVisible.value = true;
}

function openEdit(c: MongoConnection) {
  editing.value = c;
  dialogVisible.value = true;
}

function onSave(payload: { id?: string; name: string; uri: string; defaultDatabase?: string }) {
  if (payload.id) {
    conns.update(payload.id, {
      name: payload.name,
      uri: payload.uri,
      defaultDatabase: payload.defaultDatabase,
    });
    ElMessage.success('已保存');
  } else {
    const c = conns.add({
      name: payload.name,
      uri: payload.uri,
      defaultDatabase: payload.defaultDatabase,
    });
    conns.setActive(c.id);
    ElMessage.success('已新增并激活');
  }
}

async function removeConnection(c: MongoConnection) {
  try {
    await ElMessageBox.confirm(`删除连接「${c.name}」？此操作不可恢复。`, '确认删除', {
      type: 'warning',
    });
    conns.remove(c.id);
    ElMessage.success('已删除');
  } catch {
    /* user cancelled */
  }
}

function pickConnection(c: MongoConnection) {
  conns.setActive(c.id);
}

const historyShown = computed(() => history.items.value.slice(0, 80));

function clickHistory(item: HistoryItem) {
  // 切换到对应连接（若与当前不同）
  if (item.connectionId && item.connectionId !== conns.activeId.value) {
    conns.setActive(item.connectionId);
  }
  emit('pick-history', item);
}

function formatTime(ts: number): string {
  const d = new Date(ts);
  const pad = (n: number) => n.toString().padStart(2, '0');
  return `${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`;
}
</script>

<template>
  <div class="sidebar">
    <div class="tabs">
      <button :class="['tab', { active: activeTab === 'connections' }]" @click="activeTab = 'connections'">
        🔌 连接
      </button>
      <button :class="['tab', { active: activeTab === 'history' }]" @click="activeTab = 'history'">
        🕘 历史 <span v-if="history.count.value" class="badge">{{ history.count.value }}</span>
      </button>
    </div>

    <!-- 连接管理 + DB/Collection 浏览 -->
    <div v-if="activeTab === 'connections'" class="content">
      <div class="section-head">
        <span class="section-title">连接</span>
        <button class="mini-btn" @click="openAdd">+ 新增</button>
      </div>

      <div v-if="conns.items.value.length === 0" class="empty">
        <p>还没有连接</p>
        <el-button type="primary" size="small" @click="openAdd">添加第一个连接</el-button>
      </div>

      <ul class="conn-list">
        <li
          v-for="c in conns.items.value"
          :key="c.id"
          :class="['conn', { active: c.id === conns.activeId.value }]"
          @click="pickConnection(c)"
        >
          <div class="conn-name">
            <span class="dot" />
            <span class="text">{{ c.name }}</span>
          </div>
          <div class="conn-uri" :title="c.uri">{{ c.uri }}</div>
          <div class="conn-actions" @click.stop>
            <button class="link" @click="openEdit(c)">编辑</button>
            <button class="link danger" @click="removeConnection(c)">删除</button>
          </div>
        </li>
      </ul>

      <!-- DB / collection tree -->
      <div v-if="conns.active.value" class="section-head with-margin">
        <span class="section-title">数据库 / 集合</span>
        <button class="mini-btn" :disabled="loadingDb" @click="refreshDatabases">
          {{ loadingDb ? '加载中...' : '刷新' }}
        </button>
      </div>

      <ul v-if="conns.active.value" class="db-list">
        <li v-for="d in databases" :key="d.name" class="db-node">
          <button :class="['db-toggle', { picked: props.currentDatabase === d.name }]" @click="toggleDatabase(d.name)">
            <span class="caret">{{ expandedDb === d.name ? '▾' : '▸' }}</span>
            <span class="db-name">{{ d.name }}</span>
            <span v-if="d.sizeOnDisk" class="db-size">{{ Math.round(d.sizeOnDisk / 1024 / 1024) }}MB</span>
          </button>
          <ul v-if="expandedDb === d.name" class="col-list">
            <li v-if="!collections[d.name]" class="loading-col">加载中...</li>
            <li v-else-if="collections[d.name].length === 0" class="loading-col">(空)</li>
            <li
              v-for="col in collections[d.name]"
              :key="col.name"
              :class="[
                'col-item',
                { active: props.currentDatabase === d.name && props.currentCollection === col.name },
              ]"
              @click="pickCollection(d.name, col.name)"
            >
              📄 {{ col.name }}
            </li>
          </ul>
        </li>
      </ul>
    </div>

    <!-- 历史记录 -->
    <div v-else class="content">
      <div class="section-head">
        <span class="section-title">执行历史</span>
        <button class="mini-btn" :disabled="!historyShown.length" @click="history.clear()">
          清空
        </button>
      </div>
      <div v-if="historyShown.length === 0" class="empty">
        <p>暂无历史</p>
      </div>
      <ul class="hist-list">
        <li
          v-for="h in historyShown"
          :key="h.id"
          class="hist-item"
          :class="{ failed: !h.ok }"
          @click="clickHistory(h)"
        >
          <div class="hist-cmd mono" :title="h.command">{{ h.command }}</div>
          <div class="hist-meta">
            <span :class="['hist-status', h.ok ? 'ok' : 'err']">{{ h.ok ? '✓' : '✗' }}</span>
            <span v-if="h.database" class="hist-db">{{ h.connectionName }}/{{ h.database }}</span>
            <span class="hist-time">{{ formatTime(h.timestamp) }}</span>
            <span v-if="typeof h.count === 'number'" class="hist-count">{{ h.count }} docs</span>
            <button class="fav" @click.stop="history.toggleFavorite(h.id)">
              {{ h.favorite ? '★' : '☆' }}
            </button>
          </div>
        </li>
      </ul>
    </div>

    <ConnectionDialog
      v-model="dialogVisible"
      :connection="editing"
      @save="onSave"
    />
  </div>
</template>

<style scoped>
.sidebar {
  display: flex;
  flex-direction: column;
  height: 100%;
  border-right: 0;
}

.tabs {
  display: flex;
  border-bottom: 1px solid var(--border);
  background: var(--panel-2);
  flex-shrink: 0;
}
.tab {
  flex: 1;
  padding: 10px 8px;
  background: transparent;
  border: 0;
  cursor: pointer;
  color: var(--text-dim);
  font-size: 12px;
  font-weight: 500;
  border-bottom: 2px solid transparent;
  transition: all 0.15s;
}
.tab:hover {
  color: var(--text);
  background: var(--hover);
}
.tab.active {
  color: var(--accent);
  border-bottom-color: var(--accent);
  background: var(--panel);
}
.badge {
  font-size: 10px;
  background: var(--accent);
  color: white;
  padding: 1px 5px;
  border-radius: 9px;
  margin-left: 4px;
  font-weight: 700;
}

.content {
  flex: 1;
  overflow: auto;
  padding: 8px 10px 16px;
}

.section-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 4px 2px;
}
.section-head.with-margin {
  margin-top: 16px;
}
.section-title {
  font-size: 11px;
  text-transform: uppercase;
  letter-spacing: 0.5px;
  font-weight: 700;
  color: var(--text-mute);
}
.mini-btn {
  background: transparent;
  border: 1px solid var(--border);
  border-radius: 4px;
  padding: 2px 8px;
  font-size: 11px;
  color: var(--text-dim);
  cursor: pointer;
  transition: all 0.15s;
}
.mini-btn:hover:not(:disabled) {
  background: var(--hover);
  color: var(--text);
  border-color: var(--border-strong);
}
.mini-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.empty {
  text-align: center;
  color: var(--text-mute);
  font-size: 12px;
  padding: 24px 12px;
}

/* 连接列表 */
.conn-list {
  list-style: none;
  padding: 0;
  margin: 4px 0 0;
}
.conn {
  padding: 8px 10px;
  border: 1px solid var(--border);
  border-radius: 6px;
  margin-bottom: 6px;
  cursor: pointer;
  background: var(--panel);
  transition: all 0.15s;
}
.conn:hover {
  background: var(--hover);
  border-color: var(--border-strong);
}
.conn.active {
  background: var(--active);
  border-color: var(--active-border);
}
.conn-name {
  display: flex;
  align-items: center;
  gap: 6px;
  font-weight: 600;
  font-size: 13px;
  color: var(--text);
}
.dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background: var(--text-faint);
}
.conn.active .dot {
  background: var(--accent);
  box-shadow: 0 0 6px var(--accent);
}
.conn-uri {
  font-family: var(--mono);
  font-size: 10.5px;
  color: var(--text-mute);
  margin-top: 3px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.conn-actions {
  margin-top: 6px;
  display: flex;
  gap: 8px;
}
.link {
  background: transparent;
  border: 0;
  padding: 0;
  font-size: 11px;
  color: var(--text-dim);
  cursor: pointer;
}
.link:hover {
  color: var(--accent);
  text-decoration: underline;
}
.link.danger:hover {
  color: var(--danger);
}

/* DB tree */
.db-list {
  list-style: none;
  padding: 0;
  margin: 4px 0 0;
}
.db-node {
  margin-bottom: 1px;
}
.db-toggle {
  display: flex;
  align-items: center;
  gap: 4px;
  width: 100%;
  padding: 4px 6px;
  background: transparent;
  border: 0;
  border-radius: 4px;
  cursor: pointer;
  color: var(--text);
  font-size: 12.5px;
  text-align: left;
  font-family: var(--mono);
}
.db-toggle:hover {
  background: var(--hover);
}
.db-toggle.picked {
  background: var(--active);
  color: var(--accent);
}
.caret {
  width: 10px;
  font-size: 9px;
  color: var(--text-dim);
  flex-shrink: 0;
}
.db-name {
  flex: 1;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.db-size {
  font-size: 10px;
  color: var(--text-faint);
}
.col-list {
  list-style: none;
  padding: 2px 0 4px 18px;
  margin: 0;
  border-left: 1px dashed var(--border);
  margin-left: 8px;
}
.col-item {
  padding: 3px 6px;
  border-radius: 3px;
  cursor: pointer;
  color: var(--text-dim);
  font-size: 12px;
  font-family: var(--mono);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.col-item:hover {
  background: var(--hover);
  color: var(--text);
}
.col-item.active {
  background: var(--active);
  color: var(--accent);
  font-weight: 600;
}
.loading-col {
  padding: 3px 6px;
  font-size: 11px;
  color: var(--text-mute);
  font-style: italic;
}

/* 历史 */
.hist-list {
  list-style: none;
  padding: 0;
  margin: 4px 0 0;
}
.hist-item {
  padding: 6px 8px;
  border: 1px solid var(--border);
  border-radius: 5px;
  margin-bottom: 4px;
  cursor: pointer;
  background: var(--panel);
  transition: all 0.15s;
}
.hist-item:hover {
  background: var(--hover);
  border-color: var(--border-strong);
}
.hist-item.failed {
  border-left: 3px solid var(--danger);
}
.hist-cmd {
  font-size: 11.5px;
  color: var(--text);
  overflow: hidden;
  text-overflow: ellipsis;
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
  word-break: break-all;
  line-height: 1.4;
}
.hist-meta {
  margin-top: 4px;
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 10px;
  color: var(--text-mute);
}
.hist-status.ok {
  color: var(--accent);
  font-weight: 700;
}
.hist-status.err {
  color: var(--danger);
  font-weight: 700;
}
.hist-db {
  font-family: var(--mono);
}
.hist-count {
  margin-left: auto;
}
.hist-time {
  font-family: var(--mono);
}
.fav {
  background: transparent;
  border: 0;
  padding: 0;
  color: var(--text-faint);
  cursor: pointer;
  font-size: 13px;
}
.fav:hover {
  color: var(--warn);
}
</style>
