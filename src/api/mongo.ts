/**
 * MongoDB 调用：全部通过 Tauri invoke 走本地 Rust。
 * 不再依赖任何 HTTP 后端。
 *
 * Rust 侧返回的对象里，BSON 特殊类型已经被转成 relaxed EJSON：
 *   - ObjectId → {"$oid": "..."}
 *   - Date     → {"$date": "..."}
 *   - Long     → {"$numberLong": "..."}（仅在超出 i32 时）
 */

import { invoke } from '@tauri-apps/api/core';

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

async function ipc<T>(cmd: string, args: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(cmd, args);
  } catch (e: any) {
    if (typeof e === 'string') throw new Error(e);
    if (e && typeof e === 'object' && 'message' in e) throw new Error(String((e as any).message));
    throw new Error(String(e));
  }
}

export async function listDatabases(uri: string) {
  return ipc<{ ok: boolean; databases?: MongoDatabase[]; error?: string }>('mongo_list_databases', {
    uri,
  });
}

export async function listCollections(uri: string, database: string) {
  return ipc<{ ok: boolean; collections?: MongoCollection[]; error?: string }>(
    'mongo_list_collections',
    { uri, database }
  );
}

export type ImpactAffectKind =
  | 'read'
  | 'insert'
  | 'updateSingle'
  | 'updateMulti'
  | 'deleteSingle'
  | 'deleteMulti'
  | 'replaceSingle'
  | 'unknown';

export type ImpactDangerLevel = 'safe' | 'caution' | 'danger';

export interface ImpactInfo {
  ok: boolean;
  operation: string;
  collection: string;
  database: string;
  isWrite: boolean;
  affectKind: ImpactAffectKind;
  /** 写操作 filter 命中的文档数（empty filter 用 estimatedDocumentCount 提速） */
  matchedEstimate: number | null;
  /** insertOne / insertMany 的插入条数 */
  insertCount: number | null;
  /** 最大可能受影响文档数 */
  affectedMax: number | null;
  dangerLevel: ImpactDangerLevel;
  /** 顶层 filter 的 EJSON 预览（最多 200 字符） */
  filterPreview: string | null;
  emptyFilter: boolean;
}

export async function estimateImpact(uri: string, database: string, command: string) {
  return ipc<ImpactInfo>('mongo_impact_estimate', { uri, database, command });
}

export async function sampleDocuments(
  uri: string,
  database: string,
  collection: string,
  size = 3
) {
  return ipc<{ ok: boolean; docs?: unknown[]; error?: string }>('mongo_sample_documents', {
    uri,
    database,
    collection,
    size,
  });
}

export async function executeMongoCommand(
  uri: string,
  database: string,
  command: string,
  limit = 1000
): Promise<ExecuteResult> {
  return ipc<ExecuteResult>('mongo_execute', {
    uri,
    database,
    command,
    limit,
  });
}
