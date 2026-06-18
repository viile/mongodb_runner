//! MongoDB Runner —— Tauri 后端总入口。
//!
//! 注册的 invoke 命令：
//!
//!   mongo_list_databases(uri)                       -> { ok, databases }
//!   mongo_list_collections(uri, database)           -> { ok, collections }
//!   mongo_sample_documents(uri, database, col, size) -> { ok, docs }
//!   mongo_execute(uri, database, command, limit?)    -> ExecuteResult (EJSON relaxed)
//!
//!   llm_status()                                    -> { ok, available, providers }
//!   llm_generate(prompt, schema?)                    -> { ok, command, provider, model }
//!   llm_chat(messages, schema?)                      -> { ok, reply, provider, model }

mod env_loader;
mod llm;
mod mongo;
mod parser;

use std::sync::Arc;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let pool = Arc::new(mongo::MongoPool::new());

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(pool)
        .setup(|app| {
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            mongo::mongo_list_databases,
            mongo::mongo_list_collections,
            mongo::mongo_sample_documents,
            mongo::mongo_execute,
            llm::llm_status,
            llm::llm_generate,
            llm::llm_chat,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
