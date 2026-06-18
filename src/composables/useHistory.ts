import { computed, ref } from 'vue';

/**
 * 执行历史：保存执行过的 mongosh 命令 + 一些上下文。
 * 结果体积可能很大，**不**保存到 localStorage，只保存命令/连接/数据库/状态/时间。
 */

const STORAGE_KEY = 'mongodb-runner:history';
const MAX_ITEMS = 200;

export interface HistoryItem {
  id: string;
  connectionId: string | null;
  connectionName: string | null;
  database: string | null;
  command: string;
  ok: boolean;
  error?: string;
  count?: number;
  elapsedMs?: number;
  timestamp: number;
  favorite?: boolean;
}

function safeUUID(): string {
  try {
    if (typeof crypto !== 'undefined' && 'randomUUID' in crypto) {
      return (crypto as any).randomUUID();
    }
  } catch {
    /* ignore */
  }
  return `h-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`;
}

function load(): HistoryItem[] {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return [];
    const arr = JSON.parse(raw);
    if (!Array.isArray(arr)) return [];
    return arr.filter(
      (x: any) => x && typeof x.id === 'string' && typeof x.command === 'string'
    );
  } catch {
    return [];
  }
}

const items = ref<HistoryItem[]>(load());

function persist() {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(items.value));
  } catch {
    while (items.value.length > 20) {
      const nonFavIdx = items.value.findIndex((i) => !i.favorite);
      if (nonFavIdx < 0) break;
      items.value.splice(nonFavIdx, 1);
      try {
        localStorage.setItem(STORAGE_KEY, JSON.stringify(items.value));
        return;
      } catch {
        /* keep trimming */
      }
    }
  }
}

export function useHistory() {
  function record(entry: Omit<HistoryItem, 'id' | 'timestamp'>) {
    if (!entry.command.trim()) return;
    items.value.unshift({
      id: safeUUID(),
      timestamp: Date.now(),
      ...entry,
    });
    if (items.value.length > MAX_ITEMS) {
      const favs = items.value.filter((i) => i.favorite);
      const nonFavs = items.value.filter((i) => !i.favorite);
      const keep = Math.max(0, MAX_ITEMS - favs.length);
      items.value = [...favs, ...nonFavs.slice(0, keep)].sort(
        (a, b) => b.timestamp - a.timestamp
      );
    }
    persist();
  }

  function remove(id: string) {
    items.value = items.value.filter((i) => i.id !== id);
    persist();
  }

  function toggleFavorite(id: string) {
    const it = items.value.find((i) => i.id === id);
    if (!it) return;
    it.favorite = !it.favorite;
    persist();
  }

  function clear() {
    items.value = items.value.filter((i) => i.favorite);
    persist();
  }

  return {
    items,
    count: computed(() => items.value.length),
    record,
    remove,
    toggleFavorite,
    clear,
  };
}
