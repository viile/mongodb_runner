/**
 * LLM 调用：全部通过 Tauri invoke 走本地 Rust。
 *
 * - 用户在「LLM API」面板里维护的 `LLMProfile` 会作为 `profile` 参数发给 Rust
 * - 没传 profile 时，Rust 回退到 env / 本地配置文件 / cursor-agent，行为跟 git_commit.py 一致
 */

import { invoke } from '@tauri-apps/api/core';
import type { LLMProfile } from '../composables/useLLMProfiles';

export interface ChatMessage {
  role: 'user' | 'assistant';
  content: string;
}

export interface LLMResultPreview {
  ok: boolean;
  kind?: string | null;
  operation?: string | null;
  count?: number | null;
  elapsedMs?: number | null;
  truncated?: boolean | null;
  error?: string | null;
  /** 截断过的 data 预览（JSON 序列化由前端做、控制大小） */
  previewJson?: string | null;
}

export interface LLMSchema {
  /** 当前激活的连接「展示名」（不传 URI 以免把密码发给 LLM） */
  connectionName?: string | null;
  database?: string | null;
  collection?: string | null;
  /** 编辑器里当前的命令（已去除占位注释） */
  currentCommand?: string | null;
  /** 上一次查询结果摘要 */
  lastResult?: LLMResultPreview | null;
  /** 采样文档（从当前集合 sample 出来的 schema 提示） */
  sampleDocs?: unknown[];
}

export interface LLMStatus {
  ok: boolean;
  available: boolean;
  /** 调用时传入了 profile 时返回 */
  active?: {
    providerKind: 'openai' | 'cursor';
    model?: string;
    baseUrl?: string;
    binPath?: string;
  };
  /** 没传 profile，env 回退检测 */
  providers?: {
    openai?: { model: string; baseUrl: string } | null;
    cursor?: { bin: string; model: string | null } | null;
  };
  error?: string;
}

export interface EnvFileInfo {
  path: string;
  exists: boolean;
  keys: string[];
}

export interface EnvVarInfo {
  set: boolean;
  source?: string;
  maskedValue?: string;
}

export interface LocalDetect {
  ok: boolean;
  envFiles: EnvFileInfo[];
  envSnapshot: Record<string, EnvVarInfo>;
  openai: {
    available: boolean;
    baseUrl: string;
    model: string;
    apiKeyMasked: string;
    source: string;
  } | null;
  cursor: {
    binPath: string;
    /** 三态：true 已登录 / false 未登录 / null 检测失败 */
    loggedIn: boolean | null;
    source: string;
  } | null;
}

/**
 * 把 LLMProfile 转成 Rust 端期望的 camelCase 结构（其实已经是 camelCase 了，
 * 但 Rust 用了 `#[serde(rename_all = "camelCase")]`，直接传即可）。
 */
function profilePayload(profile?: LLMProfile | null) {
  if (!profile) return null;
  return {
    providerKind: profile.providerKind,
    baseUrl: profile.baseUrl ?? null,
    model: profile.model ?? null,
    apiKey: profile.apiKey ?? null,
    binPath: profile.binPath ?? null,
    cursorModel: profile.cursorModel ?? null,
    timeout: profile.timeout ?? null,
  };
}

async function ipc<T>(cmd: string, args: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(cmd, args);
  } catch (e: any) {
    if (typeof e === 'string') throw new Error(e);
    if (e && typeof e === 'object' && 'message' in e) throw new Error(String((e as any).message));
    throw new Error(String(e));
  }
}

export async function getLLMStatus(profile?: LLMProfile | null): Promise<LLMStatus> {
  try {
    return await ipc<LLMStatus>('llm_status', { profile: profilePayload(profile) });
  } catch {
    return { ok: false, available: false };
  }
}

export async function generateMongoCommand(
  prompt: string,
  schema?: LLMSchema,
  profile?: LLMProfile | null,
  locale?: string | null
) {
  return ipc<{
    ok: boolean;
    command?: string;
    provider?: string;
    model?: string;
    error?: string;
  }>('llm_generate', {
    prompt,
    schema: schema ?? null,
    profile: profilePayload(profile),
    locale: locale ?? null,
  });
}

export async function chatWithLLM(
  messages: ChatMessage[],
  schema?: LLMSchema,
  profile?: LLMProfile | null,
  locale?: string | null
) {
  return ipc<{
    ok: boolean;
    reply?: string;
    provider?: string;
    model?: string;
    error?: string;
  }>('llm_chat', {
    messages,
    schema: schema ?? null,
    profile: profilePayload(profile),
    locale: locale ?? null,
  });
}

export async function detectLocalLLM(): Promise<LocalDetect> {
  return ipc<LocalDetect>('llm_detect_local', {});
}
