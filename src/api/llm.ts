/**
 * LLM 后端调用封装。
 *
 *   - generateMongoCommand: 自然语言 → 一行 mongosh 命令（强约束）
 *   - chat: 自由对话（带历史）
 *
 * 调用方传入 schema (database/collection/sampleDocs)，让 LLM 生成贴合数据结构的命令。
 */

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

async function postJSON<T>(path: string, body: unknown, signal?: AbortSignal): Promise<T> {
  const res = await fetch(path, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify(body),
    signal,
  });
  const text = await res.text();
  try {
    return JSON.parse(text) as T;
  } catch {
    throw new Error(`后端返回了非 JSON 内容: ${text.slice(0, 200)}`);
  }
}

export async function getLLMStatus(signal?: AbortSignal): Promise<LLMStatus> {
  const res = await fetch('/api/llm/status', { signal });
  const text = await res.text();
  try {
    return JSON.parse(text) as LLMStatus;
  } catch {
    return { ok: false, available: false };
  }
}

export async function generateMongoCommand(
  prompt: string,
  schema?: LLMSchema,
  signal?: AbortSignal
) {
  return postJSON<{
    ok: boolean;
    command?: string;
    provider?: string;
    model?: string;
    error?: string;
  }>('/api/llm/generate', { prompt, schema }, signal);
}

export async function chatWithLLM(
  messages: ChatMessage[],
  schema?: LLMSchema,
  signal?: AbortSignal
) {
  return postJSON<{
    ok: boolean;
    reply?: string;
    provider?: string;
    model?: string;
    error?: string;
  }>('/api/llm/chat', { messages, schema }, signal);
}
