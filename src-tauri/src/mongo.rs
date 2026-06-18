//! Tauri 命令：直接在本进程内通过官方 mongodb 驱动访问 MongoDB。
//!
//! 所有返回值都是 EJSON relaxed 形式的 `serde_json::Value`，前端可以直接当 JSON 用：
//!   - ObjectId  → {"$oid": "..."}
//!   - Date      → {"$date": "..."}
//!   - Long      → {"$numberLong": "..."}（取决于值大小）
//!   - ...

use crate::parser::{parse_mongo_command, ParsedCommand};
use bson::{Bson, Document};
use futures::stream::TryStreamExt;
use mongodb::options::{ClientOptions, FindOneAndUpdateOptions, FindOneAndReplaceOptions, FindOneAndDeleteOptions};
use mongodb::{Client, Collection, Database};
use serde::Serialize;
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

/// 简单的全局连接缓存：进程内同一个 URI 复用 Client。
#[derive(Default)]
pub struct MongoPool {
    inner: Mutex<std::collections::HashMap<String, Arc<Client>>>,
}

impl MongoPool {
    pub fn new() -> Self {
        Self::default()
    }

    async fn get(&self, uri: &str) -> Result<Arc<Client>, String> {
        {
            let guard = self.inner.lock().await;
            if let Some(c) = guard.get(uri) {
                return Ok(c.clone());
            }
        }
        let mut opts = ClientOptions::parse(uri)
            .await
            .map_err(|e| format!("URI 解析失败: {}", e))?;
        opts.server_selection_timeout = Some(Duration::from_secs(5));
        opts.app_name = Some("mongodb-runner".to_string());
        let client = Client::with_options(opts).map_err(|e| format!("创建客户端失败: {}", e))?;
        let client = Arc::new(client);
        let mut guard = self.inner.lock().await;
        guard.insert(uri.to_string(), client.clone());
        Ok(client)
    }
}

/* ---------------- helpers ---------------- */

fn bson_to_value(b: Bson) -> Value {
    b.into_relaxed_extjson()
}

fn doc_to_value(d: Document) -> Value {
    Bson::Document(d).into_relaxed_extjson()
}

fn bson_to_document(b: Bson) -> Result<Document, String> {
    match b {
        Bson::Document(d) => Ok(d),
        Bson::Null => Ok(Document::new()),
        other => Err(format!("期望对象，得到 {:?}", other)),
    }
}

fn bson_to_array(b: Bson) -> Result<Vec<Document>, String> {
    match b {
        Bson::Array(arr) => {
            let mut out = vec![];
            for v in arr {
                match v {
                    Bson::Document(d) => out.push(d),
                    other => return Err(format!("数组元素必须是对象，得到 {:?}", other)),
                }
            }
            Ok(out)
        }
        other => Err(format!("期望数组，得到 {:?}", other)),
    }
}

fn nth_arg(parsed: &mut ParsedCommand, i: usize) -> Option<Bson> {
    if i < parsed.args.len() {
        Some(std::mem::replace(&mut parsed.args[i], Bson::Null))
    } else {
        None
    }
}

fn nth_doc(parsed: &mut ParsedCommand, i: usize) -> Result<Document, String> {
    bson_to_document(nth_arg(parsed, i).unwrap_or(Bson::Null))
}

fn nth_doc_or_empty(parsed: &mut ParsedCommand, i: usize) -> Document {
    bson_to_document(nth_arg(parsed, i).unwrap_or(Bson::Null)).unwrap_or_default()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecuteSuccess {
    pub ok: bool,
    pub database: String,
    pub collection: String,
    pub operation: String,
    pub modifiers: crate::parser::Modifiers,
    pub kind: &'static str,
    pub data: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub truncated: Option<bool>,
    pub elapsed_ms: u128,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecuteFailure {
    pub ok: bool,
    pub database: String,
    pub collection: Option<String>,
    pub operation: Option<String>,
    pub error: String,
    pub elapsed_ms: u128,
}

fn fail(database: String, parsed: &Option<ParsedCommand>, error: String, started: Instant) -> Value {
    serde_json::to_value(ExecuteFailure {
        ok: false,
        database,
        collection: parsed.as_ref().map(|p| p.collection.clone()),
        operation: parsed.as_ref().map(|p| p.op.clone()),
        error,
        elapsed_ms: started.elapsed().as_millis(),
    })
    .unwrap_or_else(|_| json!({"ok": false, "error": "serialize failure"}))
}

/* ---------------- core execute ---------------- */

async fn run_command(
    db: Database,
    parsed: &mut ParsedCommand,
    result_limit: i64,
) -> Result<ExecuteSuccess, String> {
    let coll: Collection<Document> = db.collection(&parsed.collection);
    let op = parsed.op.clone();

    match op.as_str() {
        "find" => {
            let filter = nth_doc_or_empty(parsed, 0);
            let projection_arg = nth_arg(parsed, 1);
            let mut find = coll.find(filter);
            if let Some(p) = projection_arg.clone() {
                if let Ok(doc) = bson_to_document(p) {
                    find = find.projection(doc);
                }
            }
            if let Some(p) = parsed.modifiers.projection.clone() {
                if let Ok(doc) = bson_to_document(p) {
                    find = find.projection(doc);
                }
            }
            if let Some(s) = parsed.modifiers.sort.clone() {
                if let Ok(doc) = bson_to_document(s) {
                    find = find.sort(doc);
                }
            }
            if let Some(sk) = parsed.modifiers.skip {
                find = find.skip(sk as u64);
            }
            let user_limit = parsed.modifiers.limit.unwrap_or(0);
            let hard = if user_limit > 0 {
                user_limit.min(result_limit)
            } else {
                result_limit
            };
            find = find.limit(hard);
            let cursor = find.await.map_err(|e| e.to_string())?;
            let docs: Vec<Document> = cursor.try_collect().await.map_err(|e| e.to_string())?;
            let count = docs.len();
            let truncated = if user_limit > 0 {
                count as i64 == user_limit
            } else {
                count as i64 == result_limit
            };
            Ok(ExecuteSuccess {
                ok: true,
                database: db.name().to_string(),
                collection: parsed.collection.clone(),
                operation: op,
                modifiers: std::mem::take(&mut parsed.modifiers),
                kind: "documents",
                data: Value::Array(docs.into_iter().map(doc_to_value).collect()),
                count: Some(count),
                truncated: Some(truncated),
                elapsed_ms: 0,
            })
        }
        "findOne" => {
            let filter = nth_doc_or_empty(parsed, 0);
            let mut q = coll.find_one(filter);
            if let Some(p) = nth_arg(parsed, 1) {
                if let Ok(doc) = bson_to_document(p) {
                    q = q.projection(doc);
                }
            }
            let doc = q.await.map_err(|e| e.to_string())?;
            Ok(ExecuteSuccess {
                ok: true,
                database: db.name().to_string(),
                collection: parsed.collection.clone(),
                operation: op,
                modifiers: std::mem::take(&mut parsed.modifiers),
                kind: "document",
                data: match doc {
                    Some(d) => doc_to_value(d),
                    None => Value::Null,
                },
                count: None,
                truncated: None,
                elapsed_ms: 0,
            })
        }
        "aggregate" => {
            let pipeline = bson_to_array(nth_arg(parsed, 0).unwrap_or(Bson::Array(vec![])))?;
            let mut agg = coll.aggregate(pipeline);
            if parsed.modifiers.allow_disk_use == Some(true) {
                agg = agg.allow_disk_use(true);
            }
            let cursor = agg.await.map_err(|e| e.to_string())?;
            let mut docs: Vec<Document> = cursor.try_collect().await.map_err(|e| e.to_string())?;
            let user_limit = parsed.modifiers.limit.unwrap_or(0);
            let hard = if user_limit > 0 {
                user_limit.min(result_limit) as usize
            } else {
                result_limit as usize
            };
            let truncated = docs.len() >= hard;
            if docs.len() > hard {
                docs.truncate(hard);
            }
            let count = docs.len();
            Ok(ExecuteSuccess {
                ok: true,
                database: db.name().to_string(),
                collection: parsed.collection.clone(),
                operation: op,
                modifiers: std::mem::take(&mut parsed.modifiers),
                kind: "documents",
                data: Value::Array(docs.into_iter().map(doc_to_value).collect()),
                count: Some(count),
                truncated: Some(truncated),
                elapsed_ms: 0,
            })
        }
        "countDocuments" => {
            let filter = nth_doc_or_empty(parsed, 0);
            let n = coll.count_documents(filter).await.map_err(|e| e.to_string())?;
            Ok(ExecuteSuccess {
                ok: true,
                database: db.name().to_string(),
                collection: parsed.collection.clone(),
                operation: op,
                modifiers: std::mem::take(&mut parsed.modifiers),
                kind: "scalar",
                data: Value::from(n),
                count: None,
                truncated: None,
                elapsed_ms: 0,
            })
        }
        "estimatedDocumentCount" => {
            let n = coll
                .estimated_document_count()
                .await
                .map_err(|e| e.to_string())?;
            Ok(ExecuteSuccess {
                ok: true,
                database: db.name().to_string(),
                collection: parsed.collection.clone(),
                operation: op,
                modifiers: std::mem::take(&mut parsed.modifiers),
                kind: "scalar",
                data: Value::from(n),
                count: None,
                truncated: None,
                elapsed_ms: 0,
            })
        }
        "distinct" => {
            let field = match nth_arg(parsed, 0) {
                Some(Bson::String(s)) => s,
                _ => return Err("distinct 第一个参数必须是字段名(字符串)".into()),
            };
            let filter = nth_doc_or_empty(parsed, 1);
            let values: Vec<Bson> = coll
                .distinct(field, filter)
                .await
                .map_err(|e| e.to_string())?;
            let count = values.len();
            Ok(ExecuteSuccess {
                ok: true,
                database: db.name().to_string(),
                collection: parsed.collection.clone(),
                operation: op,
                modifiers: std::mem::take(&mut parsed.modifiers),
                kind: "documents",
                data: Value::Array(values.into_iter().map(bson_to_value).collect()),
                count: Some(count),
                truncated: None,
                elapsed_ms: 0,
            })
        }
        "insertOne" => {
            let doc = nth_doc(parsed, 0)?;
            let r = coll.insert_one(doc).await.map_err(|e| e.to_string())?;
            let data = json!({
                "insertedId": bson_to_value(r.inserted_id),
                "acknowledged": true,
            });
            Ok(ExecuteSuccess {
                ok: true,
                database: db.name().to_string(),
                collection: parsed.collection.clone(),
                operation: op,
                modifiers: std::mem::take(&mut parsed.modifiers),
                kind: "writeResult",
                data,
                count: None,
                truncated: None,
                elapsed_ms: 0,
            })
        }
        "insertMany" => {
            let docs = bson_to_array(nth_arg(parsed, 0).unwrap_or(Bson::Array(vec![])))?;
            let r = coll.insert_many(docs).await.map_err(|e| e.to_string())?;
            let mut inserted = serde_json::Map::new();
            for (k, v) in r.inserted_ids {
                inserted.insert(k.to_string(), bson_to_value(v));
            }
            let data = json!({
                "insertedIds": Value::Object(inserted),
                "acknowledged": true,
            });
            Ok(ExecuteSuccess {
                ok: true,
                database: db.name().to_string(),
                collection: parsed.collection.clone(),
                operation: op,
                modifiers: std::mem::take(&mut parsed.modifiers),
                kind: "writeResult",
                data,
                count: None,
                truncated: None,
                elapsed_ms: 0,
            })
        }
        "updateOne" | "updateMany" => {
            let filter = nth_doc(parsed, 0)?;
            let update_arg = nth_arg(parsed, 1).unwrap_or(Bson::Null);
            let update_doc = match update_arg {
                Bson::Document(d) => bson::UpdateModifications::Document(d),
                Bson::Array(arr) => {
                    let mut docs: Vec<Document> = vec![];
                    for v in arr {
                        if let Bson::Document(d) = v {
                            docs.push(d);
                        } else {
                            return Err("pipeline update 数组元素必须是对象".into());
                        }
                    }
                    bson::UpdateModifications::Pipeline(docs)
                }
                _ => return Err("update 第二个参数必须是对象或管道数组".into()),
            };
            let r = if op == "updateOne" {
                coll.update_one(filter, update_doc).await
            } else {
                coll.update_many(filter, update_doc).await
            }
            .map_err(|e| e.to_string())?;
            let data = json!({
                "matchedCount": r.matched_count,
                "modifiedCount": r.modified_count,
                "upsertedId": match r.upserted_id { Some(v) => bson_to_value(v), None => Value::Null },
                "acknowledged": true,
            });
            Ok(ExecuteSuccess {
                ok: true,
                database: db.name().to_string(),
                collection: parsed.collection.clone(),
                operation: op,
                modifiers: std::mem::take(&mut parsed.modifiers),
                kind: "writeResult",
                data,
                count: None,
                truncated: None,
                elapsed_ms: 0,
            })
        }
        "replaceOne" => {
            let filter = nth_doc(parsed, 0)?;
            let replacement = nth_doc(parsed, 1)?;
            let r = coll
                .replace_one(filter, replacement)
                .await
                .map_err(|e| e.to_string())?;
            let data = json!({
                "matchedCount": r.matched_count,
                "modifiedCount": r.modified_count,
                "upsertedId": match r.upserted_id { Some(v) => bson_to_value(v), None => Value::Null },
                "acknowledged": true,
            });
            Ok(ExecuteSuccess {
                ok: true,
                database: db.name().to_string(),
                collection: parsed.collection.clone(),
                operation: op,
                modifiers: std::mem::take(&mut parsed.modifiers),
                kind: "writeResult",
                data,
                count: None,
                truncated: None,
                elapsed_ms: 0,
            })
        }
        "deleteOne" => {
            let filter = nth_doc_or_empty(parsed, 0);
            let r = coll.delete_one(filter).await.map_err(|e| e.to_string())?;
            Ok(ExecuteSuccess {
                ok: true,
                database: db.name().to_string(),
                collection: parsed.collection.clone(),
                operation: op,
                modifiers: std::mem::take(&mut parsed.modifiers),
                kind: "writeResult",
                data: json!({"deletedCount": r.deleted_count, "acknowledged": true}),
                count: None,
                truncated: None,
                elapsed_ms: 0,
            })
        }
        "deleteMany" => {
            let filter = nth_doc_or_empty(parsed, 0);
            let r = coll.delete_many(filter).await.map_err(|e| e.to_string())?;
            Ok(ExecuteSuccess {
                ok: true,
                database: db.name().to_string(),
                collection: parsed.collection.clone(),
                operation: op,
                modifiers: std::mem::take(&mut parsed.modifiers),
                kind: "writeResult",
                data: json!({"deletedCount": r.deleted_count, "acknowledged": true}),
                count: None,
                truncated: None,
                elapsed_ms: 0,
            })
        }
        "findOneAndUpdate" => {
            let filter = nth_doc(parsed, 0)?;
            let update_arg = nth_arg(parsed, 1).unwrap_or(Bson::Null);
            let update_doc = match update_arg {
                Bson::Document(d) => bson::UpdateModifications::Document(d),
                Bson::Array(arr) => {
                    let mut docs: Vec<Document> = vec![];
                    for v in arr {
                        if let Bson::Document(d) = v {
                            docs.push(d);
                        }
                    }
                    bson::UpdateModifications::Pipeline(docs)
                }
                _ => return Err("findOneAndUpdate update 必须是对象或管道数组".into()),
            };
            let _options: Option<FindOneAndUpdateOptions> = None; // 默认即可
            let r = coll
                .find_one_and_update(filter, update_doc)
                .await
                .map_err(|e| e.to_string())?;
            Ok(ExecuteSuccess {
                ok: true,
                database: db.name().to_string(),
                collection: parsed.collection.clone(),
                operation: op,
                modifiers: std::mem::take(&mut parsed.modifiers),
                kind: "document",
                data: match r {
                    Some(d) => doc_to_value(d),
                    None => Value::Null,
                },
                count: None,
                truncated: None,
                elapsed_ms: 0,
            })
        }
        "findOneAndDelete" => {
            let filter = nth_doc(parsed, 0)?;
            let _options: Option<FindOneAndDeleteOptions> = None;
            let r = coll.find_one_and_delete(filter).await.map_err(|e| e.to_string())?;
            Ok(ExecuteSuccess {
                ok: true,
                database: db.name().to_string(),
                collection: parsed.collection.clone(),
                operation: op,
                modifiers: std::mem::take(&mut parsed.modifiers),
                kind: "document",
                data: match r {
                    Some(d) => doc_to_value(d),
                    None => Value::Null,
                },
                count: None,
                truncated: None,
                elapsed_ms: 0,
            })
        }
        "findOneAndReplace" => {
            let filter = nth_doc(parsed, 0)?;
            let replacement = nth_doc(parsed, 1)?;
            let _options: Option<FindOneAndReplaceOptions> = None;
            let r = coll
                .find_one_and_replace(filter, replacement)
                .await
                .map_err(|e| e.to_string())?;
            Ok(ExecuteSuccess {
                ok: true,
                database: db.name().to_string(),
                collection: parsed.collection.clone(),
                operation: op,
                modifiers: std::mem::take(&mut parsed.modifiers),
                kind: "document",
                data: match r {
                    Some(d) => doc_to_value(d),
                    None => Value::Null,
                },
                count: None,
                truncated: None,
                elapsed_ms: 0,
            })
        }
        other => Err(format!("不支持的操作: {}", other)),
    }
}

/* ---------------- tauri commands ---------------- */

#[tauri::command]
pub async fn mongo_list_databases(
    state: tauri::State<'_, Arc<MongoPool>>,
    uri: String,
) -> Result<Value, String> {
    let client = state.get(&uri).await?;
    let specs = client
        .list_databases()
        .await
        .map_err(|e| e.to_string())?;
    let mut arr = vec![];
    for s in specs {
        arr.push(json!({
            "name": s.name,
            "sizeOnDisk": s.size_on_disk,
            "empty": s.empty,
        }));
    }
    Ok(json!({ "ok": true, "databases": arr }))
}

#[tauri::command]
pub async fn mongo_list_collections(
    state: tauri::State<'_, Arc<MongoPool>>,
    uri: String,
    database: String,
) -> Result<Value, String> {
    let client = state.get(&uri).await?;
    let cursor = client
        .database(&database)
        .list_collections()
        .await
        .map_err(|e| e.to_string())?;
    let specs: Vec<bson::Document> = cursor
        .map_ok(|spec| {
            let mut d = Document::new();
            d.insert("name", Bson::String(spec.name.clone()));
            d.insert("type", Bson::String(format!("{:?}", spec.collection_type)));
            d
        })
        .try_collect()
        .await
        .map_err(|e| e.to_string())?;
    let mut arr = vec![];
    for d in specs {
        arr.push(doc_to_value(d));
    }
    Ok(json!({ "ok": true, "collections": arr }))
}

#[tauri::command]
pub async fn mongo_sample_documents(
    state: tauri::State<'_, Arc<MongoPool>>,
    uri: String,
    database: String,
    collection: String,
    size: u32,
) -> Result<Value, String> {
    let client = state.get(&uri).await?;
    let coll: Collection<Document> = client.database(&database).collection(&collection);
    let size = size.clamp(1, 20);
    let pipeline = vec![bson::doc! { "$sample": { "size": size as i32 } }];
    let cursor = coll.aggregate(pipeline).await.map_err(|e| e.to_string())?;
    let docs: Vec<Document> = cursor.try_collect().await.map_err(|e| e.to_string())?;
    let arr: Vec<Value> = docs.into_iter().map(doc_to_value).collect();
    Ok(json!({ "ok": true, "docs": arr }))
}

#[tauri::command]
pub async fn mongo_execute(
    state: tauri::State<'_, Arc<MongoPool>>,
    uri: String,
    database: String,
    command: String,
    limit: Option<i64>,
) -> Result<Value, String> {
    let started = Instant::now();

    let mut parsed = match parse_mongo_command(&command) {
        Ok(p) => p,
        Err(e) => return Ok(fail(database, &None, format!("命令解析失败: {}", e), started)),
    };

    let client = match state.get(&uri).await {
        Ok(c) => c,
        Err(e) => return Ok(fail(database, &Some(parsed), e, started)),
    };
    let db = client.database(&database);

    let limit = limit.unwrap_or(1000);
    let res = run_command(db, &mut parsed, limit).await;
    match res {
        Ok(mut s) => {
            s.elapsed_ms = started.elapsed().as_millis();
            Ok(serde_json::to_value(s).unwrap_or_else(|_| json!({"ok": false, "error": "serialize"})))
        }
        Err(e) => Ok(fail(database, &Some(parsed), e, started)),
    }
}
