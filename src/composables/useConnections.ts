import { computed, ref } from 'vue';

/**
 * 连接管理：把用户配置的 MongoDB 连接持久化到 localStorage。
 *
 * 不加密，纯文本。这是个本地开发工具，URI 里写了密码请确认浏览器是可信的。
 */

const STORAGE_KEY = 'mongodb-runner:connections';
const ACTIVE_KEY = 'mongodb-runner:active-connection';

export interface MongoConnection {
  id: string;
  name: string;
  uri: string;
  defaultDatabase?: string;
  createdAt: number;
  lastUsedAt?: number;
}

function safeUUID(): string {
  try {
    if (typeof crypto !== 'undefined' && 'randomUUID' in crypto) {
      return (crypto as any).randomUUID();
    }
  } catch {
    /* ignore */
  }
  return `c-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`;
}

function loadAll(): MongoConnection[] {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return [];
    const arr = JSON.parse(raw);
    if (!Array.isArray(arr)) return [];
    return arr.filter(
      (x: any) =>
        x && typeof x.id === 'string' && typeof x.uri === 'string' && typeof x.name === 'string'
    );
  } catch {
    return [];
  }
}

function loadActiveId(): string | null {
  try {
    return localStorage.getItem(ACTIVE_KEY);
  } catch {
    return null;
  }
}

const items = ref<MongoConnection[]>(loadAll());
const activeId = ref<string | null>(loadActiveId());

function persist() {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(items.value));
  } catch {
    /* quota; nothing safe to do */
  }
}

function persistActive() {
  try {
    if (activeId.value) localStorage.setItem(ACTIVE_KEY, activeId.value);
    else localStorage.removeItem(ACTIVE_KEY);
  } catch {
    /* ignore */
  }
}

export function useConnections() {
  const active = computed<MongoConnection | null>(
    () => items.value.find((c) => c.id === activeId.value) || null
  );

  function add(partial: Omit<MongoConnection, 'id' | 'createdAt'>) {
    const conn: MongoConnection = {
      id: safeUUID(),
      createdAt: Date.now(),
      ...partial,
    };
    items.value.unshift(conn);
    persist();
    return conn;
  }

  function update(id: string, patch: Partial<MongoConnection>) {
    const idx = items.value.findIndex((c) => c.id === id);
    if (idx < 0) return;
    items.value[idx] = { ...items.value[idx], ...patch };
    persist();
  }

  function remove(id: string) {
    items.value = items.value.filter((c) => c.id !== id);
    if (activeId.value === id) {
      activeId.value = null;
      persistActive();
    }
    persist();
  }

  function setActive(id: string | null) {
    activeId.value = id;
    if (id) {
      const idx = items.value.findIndex((c) => c.id === id);
      if (idx >= 0) {
        items.value[idx].lastUsedAt = Date.now();
        persist();
      }
    }
    persistActive();
  }

  return {
    items,
    active,
    activeId,
    add,
    update,
    remove,
    setActive,
  };
}
