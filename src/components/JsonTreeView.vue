<script setup lang="ts">
/**
 * 递归 JSON 树。
 * 主要给中间页 ResultPanel 用。
 * - 顶层数组 / 对象会自动展开
 * - 节点超过阈值时默认折叠，避免一次性渲染
 * - 兼容 EJSON 形式：{ "$oid": "..." }, { "$date": "..." }, { "$numberLong": "..." } 等
 */
import { computed, ref, watchEffect } from 'vue';
import JsonTreeView from './JsonTreeView.vue';

const props = withDefaults(
  defineProps<{
    data: unknown;
    name?: string | number | null;
    depth?: number;
    asArrayItem?: boolean;
    expanded?: boolean | null;
  }>(),
  {
    name: null,
    depth: 0,
    asArrayItem: false,
    expanded: null,
  }
);

const AUTO_EXPAND_DEPTH = 2;
const LARGE_CONTAINER_THRESHOLD = 50;

type ValueKind =
  | 'object'
  | 'array'
  | 'string'
  | 'number'
  | 'boolean'
  | 'null'
  | 'undefined'
  | 'ejson';

/** 检测 EJSON 单字段对象，例如 { "$oid": "..." } */
function detectEjson(v: unknown): { tag: string; inner: string } | null {
  if (!v || typeof v !== 'object' || Array.isArray(v)) return null;
  const keys = Object.keys(v as object);
  if (keys.length !== 1) return null;
  const k = keys[0];
  if (!k.startsWith('$')) return null;
  const inner = (v as any)[k];
  if (typeof inner === 'string' || typeof inner === 'number') {
    return { tag: k, inner: String(inner) };
  }
  if (k === '$numberLong' || k === '$numberDecimal') {
    return { tag: k, inner: String(inner) };
  }
  if (k === '$date' && inner && typeof inner === 'object' && '$numberLong' in inner) {
    return { tag: '$date', inner: String((inner as any).$numberLong) };
  }
  return null;
}

const ejsonView = computed(() => detectEjson(props.data));

const kind = computed<ValueKind>(() => {
  if (ejsonView.value) return 'ejson';
  const d = props.data;
  if (d === null) return 'null';
  if (d === undefined) return 'undefined';
  if (Array.isArray(d)) return 'array';
  const t = typeof d;
  if (t === 'object') return 'object';
  if (t === 'string') return 'string';
  if (t === 'number') return 'number';
  if (t === 'boolean') return 'boolean';
  return 'string';
});

const isContainer = computed(() => kind.value === 'object' || kind.value === 'array');

const childCount = computed<number>(() => {
  if (kind.value === 'array') return (props.data as unknown[]).length;
  if (kind.value === 'object') return Object.keys(props.data as object).length;
  return 0;
});

const summary = computed(() => {
  if (kind.value === 'array') return `Array(${childCount.value})`;
  if (kind.value === 'object') return `Object(${childCount.value})`;
  return '';
});

const entries = computed<Array<[string | number, unknown]>>(() => {
  if (kind.value === 'array') {
    return (props.data as unknown[]).map((v, i) => [i, v] as [number, unknown]);
  }
  if (kind.value === 'object') {
    return Object.entries(props.data as Record<string, unknown>);
  }
  return [];
});

const expanded = ref(
  props.depth < AUTO_EXPAND_DEPTH ||
    (isContainer.value && childCount.value <= LARGE_CONTAINER_THRESHOLD && props.depth < 3)
);

watchEffect(() => {
  if (props.expanded === true) expanded.value = true;
  if (props.expanded === false) expanded.value = false;
});

function toggle() {
  expanded.value = !expanded.value;
}

function formatPrimitive(v: unknown): string {
  if (v === null) return 'null';
  if (v === undefined) return 'undefined';
  if (typeof v === 'string') return JSON.stringify(v);
  return String(v);
}

const keyDisplay = computed(() => {
  if (props.name === null) return '';
  if (props.asArrayItem) return `${props.name}`;
  return `"${props.name}"`;
});

const valueDisplay = computed(() => {
  if (kind.value === 'ejson') {
    const v = ejsonView.value!;
    if (v.tag === '$oid') return `ObjectId("${v.inner}")`;
    if (v.tag === '$date') {
      const ms = Number(v.inner);
      if (!Number.isNaN(ms) && /^\d+$/.test(v.inner)) {
        try {
          return `ISODate("${new Date(ms).toISOString()}")`;
        } catch {
          return `Date("${v.inner}")`;
        }
      }
      return `ISODate("${v.inner}")`;
    }
    if (v.tag === '$numberLong') return `Long("${v.inner}")`;
    if (v.tag === '$numberDecimal') return `Decimal128("${v.inner}")`;
    return `${v.tag}(${v.inner})`;
  }
  return isContainer.value ? '' : formatPrimitive(props.data);
});

const valueKindClass = computed(() => {
  if (kind.value === 'ejson') {
    const tag = ejsonView.value!.tag;
    if (tag === '$oid') return 'jt-oid';
    if (tag === '$date') return 'jt-date';
    return 'jt-number';
  }
  return `jt-${kind.value}`;
});
</script>

<template>
  <div class="jt-node" :class="{ 'jt-root': depth === 0, 'jt-container': isContainer }">
    <div class="jt-row" :class="{ clickable: isContainer }" @click="isContainer && toggle()">
      <span v-if="isContainer" class="jt-toggle" aria-hidden="true">
        {{ expanded ? '▾' : '▸' }}
      </span>
      <span v-else class="jt-toggle jt-toggle-spacer" aria-hidden="true"></span>

      <span
        v-if="keyDisplay"
        :class="['jt-key', asArrayItem ? 'jt-key-idx' : 'jt-key-str']"
      >
        {{ keyDisplay }}<span class="jt-colon">:</span>
      </span>

      <template v-if="isContainer">
        <span class="jt-bracket">{{ kind === 'array' ? '[' : '{' }}</span>
        <span v-if="!expanded" class="jt-summary">{{ summary }}</span>
        <span v-if="!expanded" class="jt-bracket">{{ kind === 'array' ? ']' : '}' }}</span>
      </template>
      <template v-else>
        <span :class="['jt-val', valueKindClass]">{{ valueDisplay }}</span>
      </template>
    </div>

    <template v-if="isContainer && expanded">
      <div class="jt-children">
        <JsonTreeView
          v-for="[k, v] in entries"
          :key="String(k)"
          :data="v"
          :name="k"
          :depth="depth + 1"
          :as-array-item="kind === 'array'"
        />
      </div>
      <div class="jt-row jt-row-close">
        <span class="jt-toggle jt-toggle-spacer" aria-hidden="true"></span>
        <span class="jt-bracket">{{ kind === 'array' ? ']' : '}' }}</span>
        <span class="jt-summary-tail">{{ summary }}</span>
      </div>
    </template>
  </div>
</template>

<style scoped>
.jt-node {
  font-family: var(--mono);
  font-size: 12.5px;
  line-height: 1.55;
  color: var(--text);
}
.jt-row {
  display: flex;
  align-items: baseline;
  gap: 4px;
  flex-wrap: wrap;
  border-radius: 3px;
  padding: 1px 4px;
  margin: 0 -4px;
}
.jt-row.clickable {
  cursor: pointer;
}
.jt-row.clickable:hover .jt-summary,
.jt-row.clickable:hover .jt-bracket {
  color: var(--accent);
}
.jt-toggle {
  display: inline-block;
  width: 12px;
  flex-shrink: 0;
  color: var(--text-dim);
  font-size: 10px;
  user-select: none;
  transition: color 0.12s;
}
.jt-toggle-spacer {
  visibility: hidden;
}
.jt-row.clickable:hover .jt-toggle {
  color: var(--accent);
}
.jt-key {
  font-weight: 500;
}
.jt-key-str {
  color: var(--tok-key);
}
.jt-key-idx {
  color: var(--text-dim);
}
.jt-colon {
  color: var(--text-dim);
  margin-right: 4px;
}
.jt-bracket {
  color: var(--text-dim);
}
.jt-summary {
  color: var(--text-dim);
  font-style: italic;
  margin: 0 4px;
}
.jt-summary-tail {
  color: var(--text-faint);
  font-size: 10.5px;
  margin-left: 8px;
  font-style: italic;
}
.jt-children {
  padding-left: 14px;
  border-left: 1px dashed var(--border);
  margin-left: 4px;
}
.jt-val {
  word-break: break-word;
  overflow-wrap: anywhere;
}
.jt-string {
  color: var(--tok-str);
}
.jt-number {
  color: var(--tok-num);
}
.jt-boolean {
  color: var(--tok-bool);
  font-weight: 500;
}
.jt-null,
.jt-undefined {
  color: var(--tok-null);
  font-style: italic;
}
.jt-oid {
  color: #a855f7;
  font-style: italic;
}
.jt-date {
  color: #0891b2;
  font-style: italic;
}
</style>
