/**
 * LLM 调用：全部通过 Tauri invoke 走本地 Rust。
 * Rust 侧会按 git_commit.py 的优先级选 provider：openai > cursor-agent。
 */

import { invoke } from '@tauri-apps/api/core';

export interface ChatMessage {
  role: 'user' | 'assistant';
  content: string;
}

export interface LLMSchema {
  database?: string | null;
  collection?: string | null;
  sampleDocs?: unknown[];
}

export interface LLMStatus {
  ok: boolean;
  available: boolean;
  providers?: {
    openai?: { model: string; baseUrl: string } | null;
    cursor?: { bin: string; model: string | null } | null;
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

export async function getLLMStatus(): Promise<LLMStatus> {
  try {
    return await ipc<LLMStatus>('llm_status', {});
  } catch {
    return { ok: false, available: false };
  }
}

export async function generateMongoCommand(prompt: string, schema?: LLMSchema) {
  return ipc<{
    ok: boolean;
    command?: string;
    provider?: string;
    model?: string;
    error?: string;
  }>('llm_generate', { prompt, schema: schema ?? null });
}

export async function chatWithLLM(messages: ChatMessage[], schema?: LLMSchema) {
  return ipc<{
    ok: boolean;
    reply?: string;
    provider?: string;
    model?: string;
    error?: string;
  }>('llm_chat', { messages, schema: schema ?? null });
}
