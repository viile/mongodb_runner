import { ref } from 'vue';

export interface ChatMessageEntry {
  id: string;
  role: 'user' | 'assistant' | 'system';
  content: string;
  /** assistant 消息里抽出的可执行 mongosh 命令（多个时取第一个） */
  command?: string;
  /** 时间戳 */
  timestamp: number;
}

const STORAGE_KEY = 'mongodb-runner:chat-history';
const MAX_TURNS = 100;

function safeUUID(): string {
  try {
    if (typeof crypto !== 'undefined' && 'randomUUID' in crypto) {
      return (crypto as any).randomUUID();
    }
  } catch {
    /* ignore */
  }
  return `m-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`;
}

function load(): ChatMessageEntry[] {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return [];
    const arr = JSON.parse(raw);
    if (!Array.isArray(arr)) return [];
    return arr.filter(
      (x: any) =>
        x && typeof x.id === 'string' && typeof x.role === 'string' && typeof x.content === 'string'
    );
  } catch {
    return [];
  }
}

const messages = ref<ChatMessageEntry[]>(load());

function persist() {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(messages.value));
  } catch {
    /* ignore quota */
  }
}

/**
 * 从 assistant 的回复里抽出第一个 mongosh 命令。
 * 优先匹配 ```js / ```javascript / ```mongosh / ```mongo 代码块；
 * 否则匹配文本里出现的第一段 `db.<col>.<op>(...)`。
 */
export function extractCommand(text: string): string | undefined {
  if (!text) return undefined;
  const fence = text.match(/```(?:js|javascript|mongosh|mongo|json)?\n([\s\S]*?)```/i);
  if (fence && fence[1]) {
    const inner = fence[1].trim();
    if (/^db\s*\.\s*[\w$]+\s*\.\s*[\w$]+\s*\(/.test(inner)) return inner;
  }
  const inline = text.match(/db\s*\.\s*[\w$]+\s*\.\s*[\w$]+\s*\([\s\S]*\)(?:\s*\.\s*[\w$]+\s*\([\s\S]*?\))*/);
  if (inline) return inline[0].trim();
  return undefined;
}

export function useChat() {
  function addUser(content: string): ChatMessageEntry {
    const entry: ChatMessageEntry = {
      id: safeUUID(),
      role: 'user',
      content,
      timestamp: Date.now(),
    };
    messages.value.push(entry);
    trim();
    persist();
    return entry;
  }

  function addAssistant(content: string): ChatMessageEntry {
    const entry: ChatMessageEntry = {
      id: safeUUID(),
      role: 'assistant',
      content,
      command: extractCommand(content),
      timestamp: Date.now(),
    };
    messages.value.push(entry);
    trim();
    persist();
    return entry;
  }

  /**
   * 系统提示型消息（执行影响评估、成功摘要、取消提示等），
   * 在 UI 上用淡色卡片显示；发给 LLM 时会被过滤掉，不污染上下文。
   */
  function addSystem(content: string): ChatMessageEntry {
    const entry: ChatMessageEntry = {
      id: safeUUID(),
      role: 'system',
      content,
      timestamp: Date.now(),
    };
    messages.value.push(entry);
    trim();
    persist();
    return entry;
  }

  function trim() {
    if (messages.value.length > MAX_TURNS * 2) {
      messages.value = messages.value.slice(-MAX_TURNS * 2);
    }
  }

  function clear() {
    messages.value = [];
    persist();
  }

  function remove(id: string) {
    messages.value = messages.value.filter((m) => m.id !== id);
    persist();
  }

  return {
    messages,
    addUser,
    addAssistant,
    addSystem,
    clear,
    remove,
  };
}
