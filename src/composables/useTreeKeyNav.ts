/**
 * 给「db / collection 平铺树」加键盘导航。
 *
 *   ↑ / ↓        在可见行之间上下移动焦点
 *   → / Enter    DB：未展开则展开；已展开则进入第一个集合 / Collection：选中
 *   ←            DB：折叠；Collection：回父 DB
 *   Space        选中（DB → 展开/折叠；Collection → pick）
 *   Esc          清空 type-to-search 缓冲
 *   字母/数字     累加进 700ms 缓冲，跳到第一个名字以缓冲为前缀的项
 *
 * `flatItems` 是一个 computed，必须按渲染顺序输出所有可见项。
 *
 * 调用者负责：
 *   - 在容器元素上设置 `tabindex=0`、`@keydown` → `onKeydown`、`@focus` → `onFocus`、`@blur` → `onBlur`
 *   - 用 `isFocused(item)` 在模板里加 focus 高亮
 *   - 鼠标点击行后调用 `setFocusedByKey(key)` 同步焦点
 *   - 通过 `onActivate` 回调实际执行「选中」动作（展开 DB / 选集合）
 *   - 通过 `onExpand` / `onCollapse` 回调实现 ←/→ 的展开折叠语义
 */

import { computed, nextTick, ref, type ComputedRef, type Ref } from 'vue';

export interface TreeFlatItem {
  /** 唯一 key（用于焦点定位和高亮） */
  key: string;
  /** 列表中可被字母搜索匹配的名称（按 startsWith 比较，会做 lowercase） */
  name: string;
  /** 给上层判断这是哪一类节点 */
  kind: 'db' | 'col' | (string & {});
  /** 给上层用的原始数据 */
  data?: unknown;
}

export interface TreeKeyNavOptions {
  flatItems: ComputedRef<TreeFlatItem[]>;
  container: Ref<HTMLElement | null>;
  onActivate: (item: TreeFlatItem) => void | Promise<void>;
  onExpand?: (item: TreeFlatItem) => void | Promise<void>;
  onCollapse?: (item: TreeFlatItem) => void | Promise<void>;
  /** 默认选中策略（无 focusedKey 时第一次 focus 调用） */
  pickInitial?: () => string | null;
  /** type-to-search 重置时长（毫秒） */
  searchResetMs?: number;
}

export function useTreeKeyNav(opts: TreeKeyNavOptions) {
  const focusedKey = ref<string>('');
  const treeFocused = ref(false);
  const searchBuffer = ref<string>('');
  let searchTimer: ReturnType<typeof setTimeout> | null = null;
  const SEARCH_RESET_MS = opts.searchResetMs ?? 700;

  const focusedIndex = computed(() => {
    const idx = opts.flatItems.value.findIndex((it) => it.key === focusedKey.value);
    return idx >= 0 ? idx : -1;
  });

  const focusedItem = computed<TreeFlatItem | null>(() => {
    const idx = focusedIndex.value;
    if (idx < 0) return null;
    return opts.flatItems.value[idx] ?? null;
  });

  function isFocused(item: TreeFlatItem): boolean {
    return treeFocused.value && focusedKey.value === item.key;
  }

  function setFocusedByKey(k: string) {
    focusedKey.value = k;
    scrollFocusedIntoView();
  }

  function scrollFocusedIntoView() {
    nextTick(() => {
      const root = opts.container.value;
      if (!root) return;
      const el = root.querySelector('[data-tree-key="' + cssEscape(focusedKey.value) + '"]') as
        | HTMLElement
        | null;
      el?.scrollIntoView({ block: 'nearest' });
    });
  }

  function cssEscape(s: string): string {
    // 简化的属性选择器转义：双引号 / 反斜杠
    return s.replace(/\\/g, '\\\\').replace(/"/g, '\\"');
  }

  function moveFocus(delta: number) {
    const list = opts.flatItems.value;
    if (list.length === 0) return;
    let i = focusedIndex.value;
    if (i < 0) i = 0;
    i = Math.max(0, Math.min(list.length - 1, i + delta));
    setFocusedByKey(list[i].key);
  }

  function resetSearch() {
    searchBuffer.value = '';
    if (searchTimer) {
      clearTimeout(searchTimer);
      searchTimer = null;
    }
  }

  function applyTypeAhead(ch: string) {
    if (searchTimer) clearTimeout(searchTimer);
    searchBuffer.value += ch.toLowerCase();
    searchTimer = setTimeout(resetSearch, SEARCH_RESET_MS);

    const buf = searchBuffer.value;
    const list = opts.flatItems.value;
    if (list.length === 0) return;

    // 当前位置作为起点，找下一项匹配；找不到再从头找一次 —— 单字符输入时支持「重复按同一字母循环」
    const start = focusedIndex.value < 0 ? 0 : focusedIndex.value;
    const isSingleCharRepeat = buf.length === 1;
    const startOffset = isSingleCharRepeat ? 1 : 0; // 单字符：跳过当前；多字符：从当前开始（含）
    for (let i = 0; i < list.length; i++) {
      const idx = (start + startOffset + i) % list.length;
      if (list[idx].name.toLowerCase().startsWith(buf)) {
        setFocusedByKey(list[idx].key);
        return;
      }
    }
    // 完全没匹配 → 不动
  }

  async function activate() {
    const it = focusedItem.value;
    if (!it) return;
    await Promise.resolve(opts.onActivate(it));
  }

  async function expandRight() {
    const it = focusedItem.value;
    if (!it) return;
    if (opts.onExpand) {
      await Promise.resolve(opts.onExpand(it));
    } else {
      await activate();
    }
  }

  async function collapseLeft() {
    const it = focusedItem.value;
    if (!it) return;
    if (opts.onCollapse) {
      await Promise.resolve(opts.onCollapse(it));
    }
  }

  function onKeydown(ev: KeyboardEvent) {
    // 让带 meta/ctrl 的快捷键交给浏览器（比如 Cmd+R 刷新、Cmd+C 复制）
    if (ev.metaKey || ev.ctrlKey) return;

    const k = ev.key;
    if (k === 'ArrowDown') {
      ev.preventDefault();
      moveFocus(1);
      return;
    }
    if (k === 'ArrowUp') {
      ev.preventDefault();
      moveFocus(-1);
      return;
    }
    if (k === 'Home') {
      ev.preventDefault();
      const list = opts.flatItems.value;
      if (list[0]) setFocusedByKey(list[0].key);
      return;
    }
    if (k === 'End') {
      ev.preventDefault();
      const list = opts.flatItems.value;
      const last = list[list.length - 1];
      if (last) setFocusedByKey(last.key);
      return;
    }
    if (k === 'ArrowRight') {
      ev.preventDefault();
      void expandRight();
      return;
    }
    if (k === 'ArrowLeft') {
      ev.preventDefault();
      void collapseLeft();
      return;
    }
    if (k === 'Enter' || k === ' ') {
      ev.preventDefault();
      void activate();
      return;
    }
    if (k === 'Escape') {
      ev.preventDefault();
      resetSearch();
      return;
    }

    // type-to-search：长度为 1 的可打印字符
    if (k.length === 1 && /\S/.test(k) && !ev.altKey) {
      ev.preventDefault();
      applyTypeAhead(k);
    }
  }

  function onFocus() {
    treeFocused.value = true;
    if (!focusedKey.value) {
      const initial = opts.pickInitial?.() ?? null;
      if (initial && opts.flatItems.value.some((it) => it.key === initial)) {
        focusedKey.value = initial;
      } else if (opts.flatItems.value.length > 0) {
        focusedKey.value = opts.flatItems.value[0].key;
      }
      scrollFocusedIntoView();
    } else {
      scrollFocusedIntoView();
    }
  }

  function onBlur() {
    treeFocused.value = false;
    resetSearch();
  }

  return {
    focusedKey,
    treeFocused,
    searchBuffer,
    isFocused,
    setFocusedByKey,
    onKeydown,
    onFocus,
    onBlur,
    moveFocus,
  };
}
