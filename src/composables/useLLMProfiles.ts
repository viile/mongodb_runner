/**
 * LLM API 配置 profile 的本地存储与切换。
 *
 * 一个 profile = 一组「provider 类型 + 凭证」。用户可以维护多个：
 *   - 公司的 OpenAI
 *   - 自己的 DeepSeek key
 *   - 本地的 ollama
 *   - cursor-agent CLI 兜底
 *
 * 任何时刻最多一个 active；ChatPanel 调用时把 active profile 透传给 Rust，
 * 不 active 时 Rust 自己读 env / dotenv（跟 git_commit.py 行为对齐）。
 *
 * 存储格式（localStorage）：
 *   mongodb-runner:llm-profiles      → LLMProfile[]
 *   mongodb-runner:llm-profile-active → 当前 active 的 id 或 ''
 */

import { computed, ref } from 'vue';

const STORAGE_PROFILES = 'mongodb-runner:llm-profiles';
const STORAGE_ACTIVE = 'mongodb-runner:llm-profile-active';

export type LLMProviderKind = 'openai' | 'cursor';

export interface LLMProfile {
  id: string;
  name: string;
  providerKind: LLMProviderKind;

  /** OpenAI 兼容 */
  baseUrl?: string;
  model?: string;
  apiKey?: string;

  /** cursor-agent */
  binPath?: string;
  cursorModel?: string;

  /** 通用：HTTP 超时（秒），或 cursor 进程超时 */
  timeout?: number;

  createdAt: number;
  updatedAt: number;
}

/* ---------------- 内置模板（OpenAI 兼容主流站） ---------------- */

export interface ProviderTemplate {
  /** 模板 key，仅用于 UI */
  key: string;
  /** 显示名 */
  label: string;
  providerKind: LLMProviderKind;
  /** OpenAI 兼容字段 */
  baseUrl?: string;
  defaultModel?: string;
  /** 入口/文档链接，方便用户去拿 key */
  docsUrl?: string;
  /** 这家提供商的常见模型，用于下拉选择（可选） */
  suggestedModels?: string[];
}

export const PROVIDER_TEMPLATES: ProviderTemplate[] = [
  {
    key: 'openai',
    label: 'OpenAI',
    providerKind: 'openai',
    baseUrl: 'https://api.openai.com',
    defaultModel: 'gpt-4o-mini',
    docsUrl: 'https://platform.openai.com/api-keys',
    suggestedModels: ['gpt-4o-mini', 'gpt-4o', 'gpt-4.1-mini', 'gpt-4.1', 'o4-mini'],
  },
  {
    key: 'deepseek',
    label: 'DeepSeek',
    providerKind: 'openai',
    baseUrl: 'https://api.deepseek.com',
    defaultModel: 'deepseek-chat',
    docsUrl: 'https://platform.deepseek.com/api_keys',
    suggestedModels: ['deepseek-chat', 'deepseek-reasoner'],
  },
  {
    key: 'moonshot',
    label: 'Moonshot / Kimi',
    providerKind: 'openai',
    baseUrl: 'https://api.moonshot.cn',
    defaultModel: 'moonshot-v1-8k',
    docsUrl: 'https://platform.moonshot.cn/console/api-keys',
    suggestedModels: ['moonshot-v1-8k', 'moonshot-v1-32k', 'moonshot-v1-128k'],
  },
  {
    key: 'qwen',
    label: '通义千问 (DashScope)',
    providerKind: 'openai',
    baseUrl: 'https://dashscope.aliyuncs.com/compatible-mode',
    defaultModel: 'qwen-plus',
    docsUrl: 'https://dashscope.console.aliyun.com/apiKey',
    suggestedModels: ['qwen-plus', 'qwen-max', 'qwen-turbo', 'qwen2.5-72b-instruct'],
  },
  {
    key: 'glm',
    label: '智谱 GLM',
    providerKind: 'openai',
    baseUrl: 'https://open.bigmodel.cn/api/paas/v4',
    defaultModel: 'glm-4-flash',
    docsUrl: 'https://open.bigmodel.cn/usercenter/apikeys',
    suggestedModels: ['glm-4-flash', 'glm-4-plus', 'glm-4-air'],
  },
  {
    key: 'groq',
    label: 'Groq',
    providerKind: 'openai',
    baseUrl: 'https://api.groq.com/openai',
    defaultModel: 'llama-3.3-70b-versatile',
    docsUrl: 'https://console.groq.com/keys',
    suggestedModels: ['llama-3.3-70b-versatile', 'llama-3.1-70b-versatile', 'mixtral-8x7b-32768'],
  },
  {
    key: 'openrouter',
    label: 'OpenRouter',
    providerKind: 'openai',
    baseUrl: 'https://openrouter.ai/api',
    defaultModel: 'anthropic/claude-3.5-sonnet',
    docsUrl: 'https://openrouter.ai/keys',
    suggestedModels: [
      'anthropic/claude-3.5-sonnet',
      'openai/gpt-4o-mini',
      'google/gemini-2.0-flash-001',
      'meta-llama/llama-3.3-70b-instruct',
    ],
  },
  {
    key: 'ollama',
    label: 'Ollama (local)',
    providerKind: 'openai',
    baseUrl: 'http://localhost:11434',
    defaultModel: 'qwen2.5:7b',
    docsUrl: 'https://github.com/ollama/ollama',
    suggestedModels: ['qwen2.5:7b', 'llama3.2', 'mistral'],
  },
  {
    key: 'custom',
    label: 'Custom (OpenAI compatible)',
    providerKind: 'openai',
  },
  {
    key: 'cursor',
    label: 'cursor-agent CLI',
    providerKind: 'cursor',
    docsUrl: 'https://docs.cursor.com/cli/overview',
  },
];

/* ---------------- store ---------------- */

function loadProfiles(): LLMProfile[] {
  try {
    const raw = localStorage.getItem(STORAGE_PROFILES);
    if (!raw) return [];
    const arr = JSON.parse(raw);
    if (!Array.isArray(arr)) return [];
    return arr.filter(
      (p): p is LLMProfile =>
        p && typeof p === 'object' && typeof p.id === 'string' && typeof p.name === 'string'
    );
  } catch {
    return [];
  }
}

function loadActiveId(): string | null {
  try {
    const v = localStorage.getItem(STORAGE_ACTIVE);
    return v && v.length > 0 ? v : null;
  } catch {
    return null;
  }
}

const profiles = ref<LLMProfile[]>(loadProfiles());
const activeId = ref<string | null>(loadActiveId());

function persistProfiles() {
  try {
    localStorage.setItem(STORAGE_PROFILES, JSON.stringify(profiles.value));
  } catch {
    /* ignore quota */
  }
}

function persistActive() {
  try {
    if (activeId.value) localStorage.setItem(STORAGE_ACTIVE, activeId.value);
    else localStorage.removeItem(STORAGE_ACTIVE);
  } catch {
    /* ignore */
  }
}

function genId() {
  return `prof_${Date.now().toString(36)}_${Math.random().toString(36).slice(2, 7)}`;
}

function add(input: Omit<LLMProfile, 'id' | 'createdAt' | 'updatedAt'>): LLMProfile {
  const now = Date.now();
  const profile: LLMProfile = { ...input, id: genId(), createdAt: now, updatedAt: now };
  profiles.value.push(profile);
  persistProfiles();
  return profile;
}

function update(id: string, patch: Partial<Omit<LLMProfile, 'id' | 'createdAt'>>): LLMProfile | null {
  const idx = profiles.value.findIndex((p) => p.id === id);
  if (idx < 0) return null;
  const old = profiles.value[idx];
  const next: LLMProfile = { ...old, ...patch, id: old.id, createdAt: old.createdAt, updatedAt: Date.now() };
  profiles.value.splice(idx, 1, next);
  persistProfiles();
  return next;
}

function remove(id: string) {
  profiles.value = profiles.value.filter((p) => p.id !== id);
  if (activeId.value === id) {
    activeId.value = null;
    persistActive();
  }
  persistProfiles();
}

function setActive(id: string | null) {
  if (id && !profiles.value.some((p) => p.id === id)) return;
  activeId.value = id;
  persistActive();
}

const active = computed<LLMProfile | null>(() =>
  activeId.value ? profiles.value.find((p) => p.id === activeId.value) ?? null : null
);

const count = computed(() => profiles.value.length);

export function useLLMProfiles() {
  return {
    profiles,
    activeId,
    active,
    count,
    add,
    update,
    remove,
    setActive,
  };
}
