/**
 * MongoDB 后端调用封装。后端响应均为 EJSON 字符串，前端用 JSON.parse 即可
 * （EJSON relaxed 形式与普通 JSON 兼容；ObjectId / Date 等会被序列化成 {"$oid":...} / {"$date":...}）。
 */

export interface MongoDatabase {
  name: string;
  sizeOnDisk?: number;
  empty?: boolean;
}

export interface MongoCollection {
  name: string;
  type?: string;
}

export type ExecuteResultKind = 'documents' | 'document' | 'scalar' | 'writeResult';

export interface ExecuteResult {
  ok: boolean;
  database?: string;
  collection?: string;
  operation?: string;
  modifiers?: Record<string, unknown>;
  kind?: ExecuteResultKind;
  data?: unknown;
  count?: number;
  truncated?: boolean;
  elapsedMs?: number;
  error?: string;
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

export async function listDatabases(uri: string, signal?: AbortSignal) {
  return postJSON<{ ok: boolean; databases?: MongoDatabase[]; error?: string }>(
    '/api/mongo/databases',
    { uri },
    signal
  );
}

export async function listCollections(uri: string, database: string, signal?: AbortSignal) {
  return postJSON<{ ok: boolean; collections?: MongoCollection[]; error?: string }>(
    '/api/mongo/collections',
    { uri, database },
    signal
  );
}

export async function sampleDocuments(
  uri: string,
  database: string,
  collection: string,
  size = 3,
  signal?: AbortSignal
) {
  return postJSON<{ ok: boolean; docs?: unknown[]; error?: string }>(
    '/api/mongo/sample',
    { uri, database, collection, size },
    signal
  );
}

export async function executeMongoCommand(
  uri: string,
  database: string,
  command: string,
  limit = 1000,
  signal?: AbortSignal
): Promise<ExecuteResult> {
  return postJSON<ExecuteResult>(
    '/api/mongo/execute',
    { uri, database, command, limit },
    signal
  );
}
