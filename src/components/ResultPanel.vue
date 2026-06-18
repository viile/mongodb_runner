<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import { ElMessage } from 'element-plus';
import JsonTreeView from './JsonTreeView.vue';
import type { ExecuteResult } from '../api/mongo';

const props = defineProps<{
  result: ExecuteResult | null;
  loading: boolean;
}>();

type ViewMode = 'tree' | 'raw' | 'table';
const view = ref<ViewMode>('tree');

const hasResult = computed(() => !!props.result);
const isOk = computed(() => !!props.result?.ok);

const rawText = computed(() => {
  if (!props.result) return '';
  try {
    return JSON.stringify(props.result.data, null, 2);
  } catch {
    return String(props.result.data);
  }
});

const isDocList = computed(() => {
  const k = props.result?.kind;
  return k === 'documents' && Array.isArray(props.result?.data);
});

const docList = computed<any[]>(() => (isDocList.value ? (props.result!.data as any[]) : []));

/** 表格模式：抽出所有出现过的 key */
const tableColumns = computed<string[]>(() => {
  if (!isDocList.value) return [];
  const set = new Set<string>();
  for (const doc of docList.value.slice(0, 100)) {
    if (doc && typeof doc === 'object') {
      for (const k of Object.keys(doc)) set.add(k);
    }
  }
  return Array.from(set).slice(0, 12);
});

function cellPreview(val: unknown): string {
  if (val === null || val === undefined) return '';
  if (typeof val === 'string') return val.length > 80 ? val.slice(0, 80) + '…' : val;
  if (typeof val === 'number' || typeof val === 'boolean') return String(val);
  if (val && typeof val === 'object') {
    if ('$oid' in (val as any)) return `oid:${String((val as any).$oid).slice(0, 8)}…`;
    if ('$date' in (val as any)) {
      const v = (val as any).$date;
      try {
        const d = typeof v === 'number' ? new Date(v) : new Date(v);
        return d.toISOString();
      } catch {
        return String(v);
      }
    }
    try {
      const s = JSON.stringify(val);
      return s.length > 80 ? s.slice(0, 80) + '…' : s;
    } catch {
      return '[obj]';
    }
  }
  return String(val);
}

async function copy(text: string) {
  try {
    await navigator.clipboard.writeText(text);
    ElMessage.success('已复制');
  } catch {
    ElMessage.error('复制失败');
  }
}

watch(
  () => props.result,
  (r) => {
    if (!r) return;
    if (r.kind === 'documents' && Array.isArray(r.data) && r.data.length > 0) {
      view.value = 'tree';
    } else if (r.kind === 'scalar' || r.kind === 'writeResult') {
      view.value = 'raw';
    }
  }
);
</script>

<template>
  <div class="result-panel">
    <div class="toolbar">
      <template v-if="hasResult">
        <span :class="['status', isOk ? 'ok' : 'err']">
          {{ isOk ? '✓' : '✗' }} {{ isOk ? 'OK' : 'ERROR' }}
        </span>
        <span v-if="props.result?.collection" class="meta mono">
          {{ props.result.database }}.{{ props.result.collection }}.{{ props.result.operation }}()
        </span>
        <span v-if="typeof props.result?.count === 'number'" class="meta">
          {{ props.result.count }} docs
        </span>
        <span v-if="props.result?.truncated" class="meta warn">已截断到 limit</span>
        <span v-if="typeof props.result?.elapsedMs === 'number'" class="meta">
          {{ props.result.elapsedMs }}ms
        </span>
      </template>
      <template v-else>
        <span class="meta">{{ loading ? '执行中...' : '尚无结果' }}</span>
      </template>
      <div class="spacer" />
      <template v-if="hasResult && isOk">
        <div class="view-tabs">
          <button :class="['vt', { active: view === 'tree' }]" @click="view = 'tree'">树形</button>
          <button :class="['vt', { active: view === 'raw' }]" @click="view = 'raw'">原文</button>
          <button v-if="isDocList" :class="['vt', { active: view === 'table' }]" @click="view = 'table'">
            表格
          </button>
        </div>
        <button class="ic-btn" title="复制 JSON" @click="copy(rawText)">⧉ 复制</button>
      </template>
    </div>

    <div class="body">
      <div v-if="loading" class="placeholder">
        <div class="loading-dot" />
        <span>正在查询 MongoDB...</span>
      </div>

      <div v-else-if="!hasResult" class="placeholder">
        <span class="hint-icon">🍃</span>
        <p>左侧选择集合后写命令，按 ⌘/Ctrl + Enter 执行</p>
      </div>

      <div v-else-if="!isOk" class="error-box">
        <strong>查询失败</strong>
        <pre>{{ props.result?.error }}</pre>
      </div>

      <template v-else>
        <div v-if="view === 'tree'" class="json-wrap">
          <JsonTreeView :data="props.result?.data" :depth="0" />
        </div>

        <pre v-else-if="view === 'raw'" class="raw mono">{{ rawText }}</pre>

        <div v-else-if="view === 'table' && isDocList" class="table-wrap">
          <table class="docs-table mono">
            <thead>
              <tr>
                <th class="idx-col">#</th>
                <th v-for="col in tableColumns" :key="col">{{ col }}</th>
              </tr>
            </thead>
            <tbody>
              <tr v-for="(doc, i) in docList" :key="i">
                <td class="idx-col">{{ i + 1 }}</td>
                <td v-for="col in tableColumns" :key="col" :title="cellPreview(doc?.[col])">
                  {{ cellPreview(doc?.[col]) }}
                </td>
              </tr>
            </tbody>
          </table>
        </div>
      </template>
    </div>
  </div>
</template>

<style scoped>
.result-panel {
  flex: 1;
  display: flex;
  flex-direction: column;
  min-height: 0;
  overflow: hidden;
  background: var(--panel);
}
.toolbar {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 6px 12px;
  border-bottom: 1px solid var(--border);
  background: var(--panel-2);
  flex-shrink: 0;
  font-size: 12px;
}
.status.ok {
  color: var(--accent);
  font-weight: 700;
}
.status.err {
  color: var(--danger);
  font-weight: 700;
}
.meta {
  color: var(--text-dim);
}
.meta.warn {
  color: var(--warn);
}
.mono {
  font-family: var(--mono);
}
.spacer {
  flex: 1;
}
.view-tabs {
  display: flex;
  background: var(--panel);
  border: 1px solid var(--border);
  border-radius: 4px;
  overflow: hidden;
}
.vt {
  background: transparent;
  border: 0;
  padding: 4px 12px;
  font-size: 11px;
  color: var(--text-dim);
  cursor: pointer;
  border-right: 1px solid var(--border);
}
.vt:last-child {
  border-right: 0;
}
.vt:hover {
  background: var(--hover);
  color: var(--text);
}
.vt.active {
  background: var(--accent);
  color: white;
  font-weight: 600;
}
.ic-btn {
  background: var(--panel);
  border: 1px solid var(--border);
  border-radius: 4px;
  padding: 3px 10px;
  font-size: 11px;
  color: var(--text-dim);
  cursor: pointer;
}
.ic-btn:hover {
  background: var(--hover);
  color: var(--text);
}

.body {
  flex: 1;
  min-height: 0;
  overflow: auto;
  padding: 10px 14px;
}

.placeholder {
  height: 100%;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 12px;
  color: var(--text-mute);
}
.placeholder .hint-icon {
  font-size: 48px;
  opacity: 0.4;
}
.placeholder p {
  font-size: 13px;
  margin: 0;
}
.loading-dot {
  width: 24px;
  height: 24px;
  border: 3px solid var(--border);
  border-top-color: var(--accent);
  border-radius: 50%;
  animation: spin 0.8s linear infinite;
}
@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}

.error-box {
  padding: 12px 14px;
  border: 1px solid var(--danger);
  border-radius: 6px;
  background: rgba(214, 57, 72, 0.08);
  color: var(--danger);
}
.error-box strong {
  display: block;
  margin-bottom: 6px;
  font-size: 13px;
}
.error-box pre {
  margin: 0;
  font-family: var(--mono);
  font-size: 12px;
  color: var(--text);
  white-space: pre-wrap;
  word-break: break-word;
}

.raw {
  margin: 0;
  font-size: 12.5px;
  white-space: pre-wrap;
  word-break: break-word;
  line-height: 1.5;
  color: var(--text);
}

.json-wrap {
  font-family: var(--mono);
}

.table-wrap {
  overflow: auto;
}
.docs-table {
  border-collapse: collapse;
  font-size: 11.5px;
  min-width: 100%;
}
.docs-table th,
.docs-table td {
  border-bottom: 1px solid var(--border);
  border-right: 1px solid var(--border);
  padding: 5px 8px;
  text-align: left;
  vertical-align: top;
  max-width: 240px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.docs-table th {
  background: var(--panel-2);
  font-weight: 700;
  color: var(--text-dim);
  position: sticky;
  top: 0;
  z-index: 1;
}
.docs-table tr:hover td {
  background: var(--hover);
}
.idx-col {
  text-align: center;
  color: var(--text-faint);
  width: 36px;
}
</style>
