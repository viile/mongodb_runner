//! 与 git_commit.py 的 provider 模式等价：
//!
//!   build_llm_config() → 走 OpenAI 兼容的 /v1/chat/completions
//!   build_cursor_config() → 走本地 cursor-agent CLI（可选）
//!
//! 配置来源（依次）：
//!   1) 进程环境变量
//!   2) `.mongo-runner.env` / `~/.config/mongodb-runner/llm.env`
//!
//! 输出统一是 JSON，前端直接吃。

use crate::env_loader::read_env;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::process::Stdio;
use std::time::Duration;
use tokio::io::AsyncReadExt;
use tokio::process::Command;

const GENERATE_SYSTEM: &str = "你是一个资深的 MongoDB 工程师。请根据用户的自然语言需求和当前的数据库 / 集合信息，生成 **一行** mongosh 风格命令。
严格输出格式（不要任何额外解释、不要 markdown 代码块）：
db.<collection>.<op>(<args>)  例如:
  db.users.find({ \"age\": { \"$gt\": 18 } }).limit(20).sort({ \"name\": 1 })
  db.events.aggregate([{ \"$match\": { \"type\": \"click\" } }, { \"$group\": { \"_id\": \"$userId\", \"n\": { \"$sum\": 1 } } }])
约束：
- 参数必须是合法 EJSON（数字/字符串/数组/对象，键加双引号）。
- 日期写 { \"$date\": \"2024-01-01T00:00:00Z\" }，ObjectId 写 { \"$oid\": \"...\" }。
- 如果用户没有明确给出 collection，使用 [DEFAULT_COLLECTION] 占位。
- 默认对 find / aggregate 加 .limit(50)，避免返回过多。
- 不要使用 mongosh 的 helper 比如 ObjectId(...) / ISODate(...)，要写成 EJSON 形式。
- 除非用户明确要求修改数据，否则只生成只读命令（find/findOne/aggregate/countDocuments/distinct）。";

const CHAT_SYSTEM: &str = "你是 MongoDB Runner 内置的助手。你既懂 MongoDB 也懂用户当前连接的数据。
回答简洁、直接；涉及命令时使用 mongosh 单行风格，参数用 EJSON 双引号 key。
当用户请求生成可执行命令时，把命令单独放在一对 ```js``` 代码块中。";

/* ---------------- config ---------------- */

#[derive(Debug, Clone)]
struct OpenAIConfig {
    api_key: String,
    base_url: String,
    model: String,
    timeout: Duration,
}

fn build_openai() -> Option<OpenAIConfig> {
    let api_key = read_env(&["OPENAI_API_KEY", "LLM_API_KEY"])?;
    let mut base_url = read_env(&["OPENAI_BASE_URL", "LLM_BASE_URL"])
        .unwrap_or_else(|| "https://api.openai.com".to_string());
    while base_url.ends_with('/') {
        base_url.pop();
    }
    if !base_url.contains("/v") {
        base_url.push_str("/v1");
    }
    let model = read_env(&["OPENAI_MODEL", "LLM_MODEL"]).unwrap_or_else(|| "gpt-4o-mini".to_string());
    let timeout = read_env(&["LLM_TIMEOUT"])
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(60);
    Some(OpenAIConfig {
        api_key,
        base_url,
        model,
        timeout: Duration::from_secs(timeout),
    })
}

#[derive(Debug, Clone)]
struct CursorConfig {
    bin: String,
    model: Option<String>,
    timeout: Duration,
}

fn build_cursor() -> Option<CursorConfig> {
    let bin = read_env(&["CURSOR_AGENT_BIN"])?;
    if !std::path::Path::new(&bin).exists() {
        return None;
    }
    let timeout = read_env(&["CURSOR_TIMEOUT"])
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(120);
    Some(CursorConfig {
        bin,
        model: read_env(&["CURSOR_MODEL"]),
        timeout: Duration::from_secs(timeout),
    })
}

/* ---------------- providers ---------------- */

#[derive(Serialize, Deserialize, Clone)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

async fn call_openai(
    cfg: &OpenAIConfig,
    messages: Vec<ChatMessage>,
    temperature: f64,
) -> Result<String, String> {
    let url = format!("{}/chat/completions", cfg.base_url);
    let payload = json!({
        "model": cfg.model,
        "messages": messages,
        "temperature": temperature,
    });

    let client = reqwest::Client::builder()
        .timeout(cfg.timeout)
        .build()
        .map_err(|e| format!("HTTP client 构建失败: {}", e))?;

    let resp = client
        .post(&url)
        .bearer_auth(&cfg.api_key)
        .json(&payload)
        .send()
        .await
        .map_err(|e| format!("LLM 请求失败: {}", e))?;

    let status = resp.status();
    let text = resp
        .text()
        .await
        .map_err(|e| format!("读取 LLM 响应失败: {}", e))?;

    if !status.is_success() {
        return Err(format!(
            "LLM HTTP {}: {}",
            status.as_u16(),
            text.chars().take(500).collect::<String>()
        ));
    }

    let data: Value = serde_json::from_str(&text)
        .map_err(|e| format!("LLM 返回了非 JSON 内容 ({}): {}", e, text.chars().take(500).collect::<String>()))?;
    let content = data
        .pointer("/choices/0/message/content")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            format!(
                "LLM 返回结构异常: {}",
                text.chars().take(500).collect::<String>()
            )
        })?;
    Ok(content.trim().to_string())
}

async fn call_cursor(cfg: &CursorConfig, prompt: String) -> Result<String, String> {
    let mut cmd = Command::new(&cfg.bin);
    cmd.arg("--print")
        .arg("--mode")
        .arg("ask")
        .arg("--output-format")
        .arg("text")
        .arg("--trust");
    if let Some(m) = &cfg.model {
        cmd.arg("--model").arg(m);
    }
    cmd.arg(prompt);
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("cursor-agent 启动失败: {}", e))?;

    let mut stdout = String::new();
    let mut stderr = String::new();
    if let Some(mut s) = child.stdout.take() {
        let _ = s.read_to_string(&mut stdout).await;
    }
    if let Some(mut s) = child.stderr.take() {
        let _ = s.read_to_string(&mut stderr).await;
    }

    let status = tokio::time::timeout(cfg.timeout, child.wait())
        .await
        .map_err(|_| format!("cursor-agent 超时 (>{}s)", cfg.timeout.as_secs()))?
        .map_err(|e| format!("cursor-agent wait 失败: {}", e))?;

    if !status.success() {
        return Err(format!(
            "cursor-agent 退出 {:?}: {}",
            status.code(),
            stderr.trim()
        ));
    }
    Ok(stdout.trim().to_string())
}

fn strip_code_fences(s: &str) -> String {
    let s = s.trim();
    let s = s.strip_prefix("```").unwrap_or(s);
    // 去掉首行的 language tag
    let s = if let Some(idx) = s.find('\n') {
        // 检查首行是不是只有 language tag
        let first = &s[..idx];
        if first.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-') {
            &s[idx + 1..]
        } else {
            s
        }
    } else {
        s
    };
    let s = s.trim_end_matches("```").trim();
    s.to_string()
}

fn describe_schema(schema: &Option<Value>) -> String {
    let Some(schema) = schema else {
        return "当前没有提供 schema 提示。".to_string();
    };
    let mut txt = String::from("当前上下文：\n");
    if let Some(db) = schema.get("database").and_then(|v| v.as_str()) {
        txt.push_str(&format!("- 数据库: {}\n", db));
    }
    if let Some(col) = schema.get("collection").and_then(|v| v.as_str()) {
        txt.push_str(&format!("- 默认集合: {}\n", col));
    }
    if let Some(docs) = schema.get("sampleDocs").and_then(|v| v.as_array()) {
        if !docs.is_empty() {
            txt.push_str(&format!("- 采样文档(共 {} 条):\n", docs.len()));
            txt.push_str("```json\n");
            txt.push_str(&serde_json::to_string_pretty(docs).unwrap_or_default());
            txt.push_str("\n```");
        }
    }
    txt
}

/* ---------------- dispatch ---------------- */

struct DispatchOutput {
    provider: &'static str,
    model: String,
    content: String,
}

async fn dispatch(
    system: &str,
    user_prompt: String,
    history: Vec<ChatMessage>,
) -> Result<DispatchOutput, String> {
    if let Some(cfg) = build_openai() {
        let mut msgs: Vec<ChatMessage> = vec![ChatMessage {
            role: "system".to_string(),
            content: system.to_string(),
        }];
        msgs.extend(history);
        msgs.push(ChatMessage {
            role: "user".to_string(),
            content: user_prompt,
        });
        let content = call_openai(&cfg, msgs, 0.2).await?;
        return Ok(DispatchOutput {
            provider: "openai",
            model: cfg.model.clone(),
            content,
        });
    }
    if let Some(cfg) = build_cursor() {
        let mut prompt = String::from(system);
        prompt.push_str("\n\n");
        for m in &history {
            prompt.push_str(&format!("[{}]\n{}\n\n", m.role, m.content));
        }
        prompt.push_str("[user]\n");
        prompt.push_str(&user_prompt);
        let content = call_cursor(&cfg, prompt).await?;
        return Ok(DispatchOutput {
            provider: "cursor",
            model: cfg.model.clone().unwrap_or_else(|| "default".into()),
            content,
        });
    }
    Err(
        "未配置 LLM。请设置 OPENAI_API_KEY 环境变量，或在 ~/.config/mongodb-runner/llm.env 中配置。"
            .into(),
    )
}

/* ---------------- tauri commands ---------------- */

#[tauri::command]
pub fn llm_status() -> Value {
    let openai = build_openai();
    let cursor = build_cursor();
    json!({
        "ok": true,
        "available": openai.is_some() || cursor.is_some(),
        "providers": {
            "openai": openai.as_ref().map(|c| json!({"model": c.model, "baseUrl": c.base_url})),
            "cursor": cursor.as_ref().map(|c| json!({"bin": c.bin, "model": c.model})),
        }
    })
}

#[tauri::command]
pub async fn llm_generate(prompt: String, schema: Option<Value>) -> Result<Value, String> {
    let prompt = prompt.trim().to_string();
    if prompt.is_empty() {
        return Ok(json!({"ok": false, "error": "prompt 不能为空"}));
    }
    let user_prompt = format!(
        "{}\n\n用户需求：\n{}",
        describe_schema(&schema),
        prompt
    );
    match dispatch(GENERATE_SYSTEM, user_prompt, vec![]).await {
        Ok(out) => {
            let command = strip_code_fences(&out.content);
            Ok(json!({
                "ok": true,
                "command": command,
                "provider": out.provider,
                "model": out.model,
            }))
        }
        Err(e) => Ok(json!({"ok": false, "error": e})),
    }
}

#[tauri::command]
pub async fn llm_chat(messages: Vec<ChatMessage>, schema: Option<Value>) -> Result<Value, String> {
    if messages.is_empty() {
        return Ok(json!({"ok": false, "error": "messages 不能为空"}));
    }
    let last = messages.last().unwrap();
    if last.role != "user" {
        return Ok(json!({"ok": false, "error": "最后一条 message 必须是 user"}));
    }
    let history: Vec<ChatMessage> = messages[..messages.len() - 1]
        .iter()
        .map(|m| ChatMessage {
            role: if m.role == "assistant" { "assistant".into() } else { "user".into() },
            content: m.content.clone(),
        })
        .collect();
    let user_prompt = format!(
        "{}\n\n用户：\n{}",
        describe_schema(&schema),
        last.content
    );
    match dispatch(CHAT_SYSTEM, user_prompt, history).await {
        Ok(out) => Ok(json!({
            "ok": true,
            "reply": out.content,
            "provider": out.provider,
            "model": out.model,
        })),
        Err(e) => Ok(json!({"ok": false, "error": e})),
    }
}
