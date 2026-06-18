<script setup lang="ts">
import { computed, nextTick, ref, watch } from 'vue';
import { ElMessage, ElMessageBox } from 'element-plus';
import { useI18n } from 'vue-i18n';
import ConnectionDialog from './ConnectionDialog.vue';
import LLMConfig from './LLMConfig.vue';
import { useConnections, type MongoConnection } from '../composables/useConnections';
import { useHistory, type HistoryItem } from '../composables/useHistory';
import { useLLMProfiles } from '../composables/useLLMProfiles';
import { useTreeKeyNav, type TreeFlatItem } from '../composables/useTreeKeyNav';
import { listCollections, listDatabases, type MongoCollection, type MongoDatabase } from '../api/mongo';

const { t } = useI18n();
const llmProfiles = useLLMProfiles();

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

type TabKey = 'connections' | 'history' | 'llm';
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
      const all = r.databases || [];
      // 普通库放前面、系统库（admin/config/local）放末尾；每一段内部按名字升序
      const byName = (a: MongoDatabase, b: MongoDatabase) =>
        a.name.localeCompare(b.name, undefined, { sensitivity: 'base' });
      const isSys = (d: MongoDatabase) => ['admin', 'config', 'local'].includes(d.name);
      const userDbs = all.filter((d) => !isSys(d)).sort(byName);
      const sysDbs = all.filter(isSys).sort(byName);
      databases.value = [...userDbs, ...sysDbs];
    } else {
      ElMessage.error(t('sidebar.listDbFailed', { error: r.error || '' }));
    }
  } catch (e: any) {
    ElMessage.error(t('sidebar.listDbFailed', { error: e?.message || String(e) }));
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
      collections.value[dbName] = (r.collections || [])
        .filter((c) => !c.name.startsWith('system.'))
        .sort((a, b) => a.name.localeCompare(b.name, undefined, { sensitivity: 'base' }));
    } else {
      ElMessage.error(t('sidebar.listColFailed', { error: r.error || '' }));
    }
  } catch (e: any) {
    ElMessage.error(t('sidebar.listColFailed', { error: e?.message || String(e) }));
  }
}

function pickCollection(db: string, col: string) {
  emit('pick-collection', { database: db, collection: col });
}

/* ---------- 键盘导航（type-to-jump、↑↓ / ←→ / Enter） ---------- */

const dbListRef = ref<HTMLElement | null>(null);

const flatTreeItems = computed<TreeFlatItem[]>(() => {
  const out: TreeFlatItem[] = [];
  for (const db of databases.value) {
    out.push({ key: `db:${db.name}`, name: db.name, kind: 'db', data: db });
    if (expandedDb.value === db.name) {
      const cols = collections.value[db.name];
      if (cols && cols.length > 0) {
        for (const c of cols) {
          out.push({
            key: `col:${db.name}/${c.name}`,
            name: c.name,
            kind: 'col',
            data: { db: db.name, col: c },
          });
        }
      }
    }
  }
  return out;
});

const tree = useTreeKeyNav({
  flatItems: flatTreeItems,
  container: dbListRef,
  pickInitial: () => {
    if (props.currentDatabase && props.currentCollection) {
      return `col:${props.currentDatabase}/${props.currentCollection}`;
    }
    if (props.currentDatabase) return `db:${props.currentDatabase}`;
    return null;
  },
  async onActivate(item) {
    if (item.kind === 'db') {
      await toggleDatabase(item.name);
    } else if (item.kind === 'col') {
      const d = (item.data as any)?.db as string | undefined;
      if (d) pickCollection(d, item.name);
    }
  },
  async onExpand(item) {
    if (item.kind === 'db') {
      if (expandedDb.value !== item.name) {
        await toggleDatabase(item.name); // 展开
      } else {
        // 已展开 → 把焦点移到第一个 collection
        const cols = collections.value[item.name];
        if (cols && cols.length > 0) {
          tree.setFocusedByKey(`col:${item.name}/${cols[0].name}`);
        }
      }
    }
  },
  onCollapse(item) {
    if (item.kind === 'col') {
      const d = (item.data as any)?.db as string | undefined;
      if (d) tree.setFocusedByKey(`db:${d}`);
    } else if (item.kind === 'db' && expandedDb.value === item.name) {
      expandedDb.value = null;
    }
  },
});

/** 鼠标点击 db / collection 时，同步焦点到该行（键盘继续从这里走） */
function focusDb(name: string) {
  tree.setFocusedByKey(`db:${name}`);
}
function focusCol(db: string, name: string) {
  tree.setFocusedByKey(`col:${db}/${name}`);
}

/** 当用户切回 connections tab 时尝试聚焦 db 列表，让键盘立即可用 */
watch(activeTab, async (next) => {
  if (next !== 'connections') return;
  await nextTick();
  if (databases.value.length > 0) dbListRef.value?.focus();
});

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
    ElMessage.success(t('sidebar.saved'));
  } else {
    const c = conns.add({
      name: payload.name,
      uri: payload.uri,
      defaultDatabase: payload.defaultDatabase,
    });
    conns.setActive(c.id);
    ElMessage.success(t('sidebar.addedActivated'));
  }
}

async function removeConnection(c: MongoConnection) {
  try {
    await ElMessageBox.confirm(
      t('sidebar.confirmDelete', { name: c.name }),
      t('sidebar.confirmTitle'),
      { type: 'warning' }
    );
    conns.remove(c.id);
    ElMessage.success(t('sidebar.deleted'));
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
        🔌 {{ t('sidebar.tabConnections') }}
      </button>
      <button :class="['tab', { active: activeTab === 'history' }]" @click="activeTab = 'history'">
        🕘 {{ t('sidebar.tabHistory') }} <span v-if="history.count.value" class="badge">{{ history.count.value }}</span>
      </button>
      <button :class="['tab', { active: activeTab === 'llm' }]" @click="activeTab = 'llm'">
        🤖 {{ t('sidebar.tabLLM') }}
        <span v-if="llmProfiles.count.value" class="badge">{{ llmProfiles.count.value }}</span>
      </button>
    </div>

    <!-- 连接管理 + DB/Collection 浏览 -->
    <div v-if="activeTab === 'connections'" class="content">
      <div class="section-head">
        <span class="section-title">{{ t('sidebar.sectionConnections') }}</span>
        <button class="mini-btn" @click="openAdd">{{ t('sidebar.add') }}</button>
      </div>

      <div v-if="conns.items.value.length === 0" class="empty">
        <p>{{ t('sidebar.emptyConnections') }}</p>
        <el-button type="primary" size="small" @click="openAdd">{{ t('sidebar.addFirst') }}</el-button>
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
            <button class="link" @click="openEdit(c)">{{ t('sidebar.edit') }}</button>
            <button class="link danger" @click="removeConnection(c)">{{ t('sidebar.remove') }}</button>
          </div>
        </li>
      </ul>

      <!-- DB / collection tree -->
      <div v-if="conns.active.value" class="section-head with-margin">
        <span class="section-title">{{ t('sidebar.sectionDbCol') }}</span>
        <button class="mini-btn" :disabled="loadingDb" @click="refreshDatabases">
          {{ loadingDb ? t('sidebar.loading') : t('sidebar.refresh') }}
        </button>
      </div>

      <ul
        v-if="conns.active.value"
        ref="dbListRef"
        class="db-list"
        :class="{ 'kb-focused': tree.treeFocused.value }"
        tabindex="0"
        :aria-label="t('sidebar.sectionDbCol')"
        :title="t('sidebar.kbHint')"
        @keydown="tree.onKeydown"
        @focus="tree.onFocus"
        @blur="tree.onBlur"
      >
        <li v-for="d in databases" :key="d.name" class="db-node">
          <button
            :data-tree-key="`db:${d.name}`"
            :class="[
              'db-toggle',
              {
                picked: props.currentDatabase === d.name,
                'kb-focus': tree.isFocused({ key: `db:${d.name}`, name: d.name, kind: 'db' }),
              },
            ]"
            tabindex="-1"
            @click="focusDb(d.name); toggleDatabase(d.name)"
          >
            <span class="caret">{{ expandedDb === d.name ? '▾' : '▸' }}</span>
            <span class="db-name">{{ d.name }}</span>
            <span v-if="d.sizeOnDisk" class="db-size">{{ Math.round(d.sizeOnDisk / 1024 / 1024) }}MB</span>
          </button>
          <ul v-if="expandedDb === d.name" class="col-list">
            <li v-if="!collections[d.name]" class="loading-col">{{ t('sidebar.loading') }}</li>
            <li v-else-if="collections[d.name].length === 0" class="loading-col">{{ t('sidebar.emptyDb') }}</li>
            <li
              v-for="col in collections[d.name]"
              :key="col.name"
              :data-tree-key="`col:${d.name}/${col.name}`"
              :class="[
                'col-item',
                {
                  active: props.currentDatabase === d.name && props.currentCollection === col.name,
                  'kb-focus': tree.isFocused({
                    key: `col:${d.name}/${col.name}`,
                    name: col.name,
                    kind: 'col',
                  }),
                },
              ]"
              @click="focusCol(d.name, col.name); pickCollection(d.name, col.name)"
            >
              📄 {{ col.name }}
            </li>
          </ul>
        </li>
        <li
          v-if="tree.treeFocused.value && tree.searchBuffer.value"
          class="kb-search-hint"
          aria-live="polite"
        >
          🔎 {{ tree.searchBuffer.value }}
        </li>
      </ul>
    </div>

    <!-- 历史记录 -->
    <div v-else-if="activeTab === 'history'" class="content">
      <div class="section-head">
        <span class="section-title">{{ t('sidebar.sectionHistory') }}</span>
        <button class="mini-btn" :disabled="!historyShown.length" @click="history.clear()">
          {{ t('sidebar.clearAll') }}
        </button>
      </div>
      <div v-if="historyShown.length === 0" class="empty">
        <p>{{ t('sidebar.emptyHistory') }}</p>
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
            <span v-if="typeof h.count === 'number'" class="hist-count">{{ t('result.docs', { n: h.count }) }}</span>
            <button class="fav" @click.stop="history.toggleFavorite(h.id)">
              {{ h.favorite ? '★' : '☆' }}
            </button>
          </div>
        </li>
      </ul>
    </div>

    <!-- LLM API 配置 -->
    <LLMConfig v-else-if="activeTab === 'llm'" />

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
  outline: none;
  position: relative;
}
.db-list:focus,
.db-list.kb-focused {
  outline: none;
}
/* 键盘焦点行：左侧色条 + 强调色调，跟 .picked / .active 区分开 */
.db-toggle.kb-focus,
.col-item.kb-focus {
  box-shadow: inset 2px 0 0 var(--accent);
}
.db-list.kb-focused .db-toggle.kb-focus,
.db-list.kb-focused .col-item.kb-focus {
  background: var(--active);
}
.kb-search-hint {
  position: sticky;
  bottom: 4px;
  margin: 6px 6px 0;
  padding: 3px 8px;
  font-family: var(--mono);
  font-size: 11px;
  color: var(--text);
  background: var(--panel-2);
  border: 1px solid var(--accent);
  border-radius: 4px;
  text-align: center;
  pointer-events: none;
  opacity: 0.92;
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
