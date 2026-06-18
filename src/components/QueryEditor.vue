<script setup lang="ts">
import { computed, nextTick, onMounted, ref, watch } from 'vue';

const props = defineProps<{
  modelValue: string;
  loading: boolean;
  canRun: boolean;
}>();

const emit = defineEmits<{
  (e: 'update:modelValue', v: string): void;
  (e: 'run'): void;
  (e: 'stop'): void;
}>();

const text = computed({
  get: () => props.modelValue,
  set: (v) => emit('update:modelValue', v),
});

const taRef = ref<HTMLTextAreaElement | null>(null);
const lineCount = ref(1);

watch(text, (v) => {
  lineCount.value = Math.max(1, v.split('\n').length);
});

onMounted(() => {
  lineCount.value = Math.max(1, text.value.split('\n').length);
});

function onKeydown(ev: KeyboardEvent) {
  const isEnter = ev.key === 'Enter';
  const isCmd = ev.metaKey || ev.ctrlKey;
  if (isCmd && isEnter) {
    ev.preventDefault();
    if (props.loading) emit('stop');
    else emit('run');
    return;
  }
  // Tab 输入 2 个空格
  if (ev.key === 'Tab') {
    ev.preventDefault();
    const ta = ev.target as HTMLTextAreaElement;
    const start = ta.selectionStart;
    const end = ta.selectionEnd;
    const insert = '  ';
    text.value = text.value.slice(0, start) + insert + text.value.slice(end);
    nextTick(() => {
      ta.setSelectionRange(start + insert.length, start + insert.length);
    });
  }
}

const lineNumbers = computed(() => Array.from({ length: lineCount.value }, (_, i) => i + 1));

function format() {
  // 简单尝试用 JSON.parse 美化命令里的 JSON 段，失败就保持原样
  const m = text.value.match(/^(db\.[\w$]+\.[\w$]+\()([\s\S]*)(\)(?:\s*\.\s*[\w$]+\s*\([\s\S]*?\))*\s*;?\s*)$/);
  if (!m) return;
  const head = m[1];
  const argText = m[2];
  const tail = m[3];
  try {
    // 尝试切分顶层逗号
    const args = splitArgs(argText);
    const pretty = args
      .map((a) => {
        try {
          const obj = JSON.parse(a);
          return JSON.stringify(obj, null, 2);
        } catch {
          return a.trim();
        }
      })
      .join(',\n');
    text.value = `${head}\n${pretty.replace(/^/gm, '  ')}\n${tail}`;
  } catch {
    /* ignore */
  }
}

function splitArgs(s: string): string[] {
  const result: string[] = [];
  let depth = 0;
  let buf = '';
  let quote: string | null = null;
  for (let i = 0; i < s.length; i++) {
    const ch = s[i];
    if (quote) {
      buf += ch;
      if (ch === '\\' && i + 1 < s.length) {
        buf += s[i + 1];
        i++;
        continue;
      }
      if (ch === quote) quote = null;
      continue;
    }
    if (ch === '"' || ch === "'") {
      quote = ch;
      buf += ch;
      continue;
    }
    if (ch === '{' || ch === '[' || ch === '(') {
      depth++;
      buf += ch;
      continue;
    }
    if (ch === '}' || ch === ']' || ch === ')') {
      depth--;
      buf += ch;
      continue;
    }
    if (ch === ',' && depth === 0) {
      result.push(buf);
      buf = '';
      continue;
    }
    buf += ch;
  }
  if (buf.trim()) result.push(buf);
  return result;
}
</script>

<template>
  <div class="query-editor">
    <div class="toolbar">
      <span class="title">查询编辑器</span>
      <span class="kbd">⌘/Ctrl + Enter</span>
      <div class="spacer" />
      <button class="t-btn" :disabled="loading" @click="format">格式化</button>
      <button
        v-if="!loading"
        class="t-btn run"
        :disabled="!canRun"
        :title="canRun ? '执行' : '请先选择连接和数据库'"
        @click="emit('run')"
      >
        ▶ 执行
      </button>
      <button v-else class="t-btn stop" @click="emit('stop')">■ 停止</button>
    </div>
    <div class="editor-body">
      <div class="gutter">
        <span v-for="n in lineNumbers" :key="n" class="ln">{{ n }}</span>
      </div>
      <textarea
        ref="taRef"
        v-model="text"
        class="ta mono"
        spellcheck="false"
        autocapitalize="off"
        autocorrect="off"
        @keydown="onKeydown"
      />
    </div>
  </div>
</template>

<style scoped>
.query-editor {
  flex: 1;
  min-height: 0;
  min-width: 0;
  background: var(--panel);
  display: flex;
  flex-direction: column;
}
.toolbar {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 6px 12px;
  border-bottom: 1px solid var(--border);
  background: var(--panel-2);
  flex-shrink: 0;
}
.title {
  font-weight: 700;
  font-size: 12px;
  text-transform: uppercase;
  letter-spacing: 0.5px;
  color: var(--text-dim);
}
.kbd {
  font-family: var(--mono);
  font-size: 10.5px;
  color: var(--text-mute);
  background: var(--kbd-bg);
  padding: 1px 6px;
  border-radius: 3px;
}
.spacer {
  flex: 1;
}
.t-btn {
  background: var(--panel);
  border: 1px solid var(--border);
  border-radius: 4px;
  padding: 4px 12px;
  font-size: 12px;
  color: var(--text);
  cursor: pointer;
  transition: all 0.15s;
}
.t-btn:hover:not(:disabled) {
  background: var(--hover);
  border-color: var(--border-strong);
}
.t-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}
.t-btn.run {
  background: var(--accent);
  border-color: var(--accent);
  color: white;
  font-weight: 600;
}
.t-btn.run:hover:not(:disabled) {
  background: var(--accent-2);
  border-color: var(--accent-2);
}
.t-btn.stop {
  background: var(--danger);
  border-color: var(--danger);
  color: white;
  font-weight: 600;
}

.editor-body {
  flex: 1;
  display: flex;
  min-height: 0;
  overflow: hidden;
}
.gutter {
  background: var(--panel-2);
  border-right: 1px solid var(--border);
  padding: 8px 6px;
  user-select: none;
  text-align: right;
  font-family: var(--mono);
  font-size: 12px;
  color: var(--text-faint);
  line-height: 1.55;
  overflow: hidden;
  min-width: 32px;
}
.ln {
  display: block;
}
.ta {
  flex: 1;
  border: 0;
  outline: none;
  resize: none;
  background: var(--panel);
  color: var(--text);
  padding: 8px 12px;
  font-size: 13px;
  line-height: 1.55;
  font-family: var(--mono);
  white-space: pre;
  overflow: auto;
  tab-size: 2;
}
.ta::selection {
  background: var(--selection-bg);
}
</style>
