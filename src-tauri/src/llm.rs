//! 与 git_commit.py 的 provider 模式等价：
//!
//!   OpenAI 兼容 (/v1/chat/completions) ←→ Anthropic-style? 暂不支持，先统一走 OpenAI 兼容
//!   cursor-agent CLI（本地子进程）
//!
//! 配置来源（优先级从高到低）：
//!   1) 前端传入的 LLMProfile（用户在「LLM API」面板里配的）
//!   2) 进程环境变量
//!   3) `.mongo-runner.env` / XDG / ~/.mongodb-runner.env
//!
//! 这样既可以「点开 UI 把多个 provider 攒着切换」，也能保留 git_commit.py 风格的零 UI 启动。

use crate::env_loader::{config_paths, parse_env_file, read_env};
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

/// 把前端传入的 BCP-47 locale 转成一个英文语言名（用于注入到 system prompt）。
/// 设计原则：英文名描述更稳定，且模型对 "English / Simplified Chinese" 这类称谓有更一致的认知。
fn locale_to_language_name(locale: &str) -> Option<&'static str> {
    let lower = locale.to_lowercase().replace('_', "-");
    let primary = lower.split('-').next().unwrap_or("");
    // 精确匹配优先
    let by_exact = match lower.as_str() {
        "zh-cn" | "zh-hans" | "zh" => Some("Simplified Chinese (简体中文)"),
        "zh-tw" | "zh-hk" | "zh-mo" | "zh-hant" => Some("Traditional Chinese (繁體中文)"),
        "en-us" | "en-gb" | "en" => Some("English"),
        "ja-jp" | "ja" => Some("Japanese (日本語)"),
        "ko-kr" | "ko" => Some("Korean (한국어)"),
        "fr-fr" | "fr" => Some("French (Français)"),
        "de-de" | "de" => Some("German (Deutsch)"),
        "es-es" | "es" => Some("Spanish (Español)"),
        "pt-br" | "pt-pt" | "pt" => Some("Brazilian Portuguese (Português)"),
        "ru-ru" | "ru" => Some("Russian (Русский)"),
        _ => None,
    };
    if by_exact.is_some() {
        return by_exact;
    }
    // 退而用 primary subtag
    match primary {
        "zh" => Some("Simplified Chinese (简体中文)"),
        "en" => Some("English"),
        "ja" => Some("Japanese (日本語)"),
        "ko" => Some("Korean (한국어)"),
        "fr" => Some("French (Français)"),
        "de" => Some("German (Deutsch)"),
        "es" => Some("Spanish (Español)"),
        "pt" => Some("Brazilian Portuguese (Português)"),
        "ru" => Some("Russian (Русский)"),
        _ => None,
    }
}

/// 给 chat 类系统提示追加「请用 X 语言回复」。
/// generate 命令本身只输出 mongosh 命令，不受影响，所以不附加。
fn with_locale(base: &str, locale: Option<&str>) -> String {
    let lang = locale
        .and_then(|l| if l.trim().is_empty() { None } else { Some(l) })
        .and_then(locale_to_language_name);
    match lang {
        Some(name) => format!(
            "{base}\n\n# Reply language\n\
             The user's UI language is **{name}**. \
             You MUST reply in {name} for any natural-language prose, \
             including explanations, summaries, warnings and clarifying questions. \
             Code, JSON, EJSON, command syntax, field names and identifiers must remain unchanged. \
             If the user explicitly switches to another language in their message, \
             follow the user instead.",
            base = base,
            name = name,
        ),
        None => base.to_string(),
    }
}

/* ---------------- profile（来自前端） ---------------- */

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LLMProfile {
    /// "openai" | "cursor"
    pub provider_kind: String,
    /// OpenAI 兼容
    pub base_url: Option<String>,
    pub model: Option<String>,
    pub api_key: Option<String>,
    /// cursor-agent
    pub bin_path: Option<String>,
    pub cursor_model: Option<String>,
    /// 通用
    pub timeout: Option<u64>,
}

/* ---------------- env-based config ---------------- */

#[derive(Debug, Clone)]
struct OpenAIConfig {
    api_key: String,
    base_url: String,
    model: String,
    timeout: Duration,
}

fn normalize_base_url(input: &str) -> String {
    let mut url = input.trim().trim_end_matches('/').to_string();
    if url.is_empty() {
        url.push_str("https://api.openai.com");
    }
    if !url.contains("/v") {
        url.push_str("/v1");
    }
    url
}

fn build_openai_from_env() -> Option<OpenAIConfig> {
    let api_key = read_env(&["OPENAI_API_KEY", "LLM_API_KEY"])?;
    let base_url = read_env(&["OPENAI_BASE_URL", "LLM_BASE_URL"])
        .unwrap_or_else(|| "https://api.openai.com".to_string());
    let model = read_env(&["OPENAI_MODEL", "LLM_MODEL"]).unwrap_or_else(|| "gpt-4o-mini".to_string());
    let timeout = read_env(&["LLM_TIMEOUT"])
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(60);
    Some(OpenAIConfig {
        api_key,
        base_url: normalize_base_url(&base_url),
        model,
        timeout: Duration::from_secs(timeout),
    })
}

fn build_openai_from_profile(p: &LLMProfile) -> Result<OpenAIConfig, String> {
    let api_key = p
        .api_key
        .as_ref()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "请填写 API Key".to_string())?;
    let base_url = p
        .base_url
        .as_ref()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "https://api.openai.com".to_string());
    let model = p
        .model
        .as_ref()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "gpt-4o-mini".to_string());
    let timeout = p.timeout.unwrap_or(60).max(1);
    Ok(OpenAIConfig {
        api_key,
        base_url: normalize_base_url(&base_url),
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

fn detect_cursor_binary() -> Option<String> {
    if let Some(p) = read_env(&["CURSOR_AGENT_BIN"]) {
        if std::path::Path::new(&p).exists() {
            return Some(p);
        }
    }
    which::which("cursor-agent")
        .ok()
        .map(|p| p.to_string_lossy().to_string())
}

fn build_cursor_from_env() -> Option<CursorConfig> {
    let bin = detect_cursor_binary()?;
    let timeout = read_env(&["CURSOR_TIMEOUT"])
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(120);
    Some(CursorConfig {
        bin,
        model: read_env(&["CURSOR_MODEL"]),
        timeout: Duration::from_secs(timeout),
    })
}

fn build_cursor_from_profile(p: &LLMProfile) -> Result<CursorConfig, String> {
    let bin = p
        .bin_path
        .as_ref()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .or_else(detect_cursor_binary)
        .ok_or_else(|| "未找到 cursor-agent 可执行文件".to_string())?;
    if !std::path::Path::new(&bin).exists() {
        return Err(format!("cursor-agent 不存在: {}", bin));
    }
    let timeout = p.timeout.unwrap_or(120).max(1);
    let model = p
        .cursor_model
        .as_ref()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    Ok(CursorConfig {
        bin,
        model,
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
        .map_err(|e| format!("LLM 返回了非 JSON ({}): {}", e, text.chars().take(500).collect::<String>()))?;
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
    let s = if let Some(idx) = s.find('\n') {
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
        return "当前没有提供任何上下文。".to_string();
    };
    let mut txt = String::from("当前上下文：\n");
    if let Some(name) = schema.get("connectionName").and_then(|v| v.as_str()) {
        if !name.is_empty() {
            txt.push_str(&format!("- 连接: {}\n", name));
        }
    }
    if let Some(db) = schema.get("database").and_then(|v| v.as_str()) {
        if !db.is_empty() {
            txt.push_str(&format!("- 数据库: {}\n", db));
        }
    }
    if let Some(col) = schema.get("collection").and_then(|v| v.as_str()) {
        if !col.is_empty() {
            txt.push_str(&format!("- 默认集合: {}\n", col));
        }
    }

    // 编辑器里当前的命令
    if let Some(cmd) = schema.get("currentCommand").and_then(|v| v.as_str()) {
        if !cmd.trim().is_empty() {
            txt.push_str("- 当前编辑器命令：\n```js\n");
            txt.push_str(cmd);
            txt.push_str("\n```\n");
        }
    }

    // 上一次执行结果摘要
    if let Some(last) = schema.get("lastResult").and_then(|v| v.as_object()) {
        let ok = last.get("ok").and_then(|v| v.as_bool()).unwrap_or(false);
        txt.push_str(&format!(
            "- 上一次执行结果（{}）",
            if ok { "成功" } else { "失败" }
        ));
        let mut meta: Vec<String> = vec![];
        if let Some(s) = last.get("operation").and_then(|v| v.as_str()) {
            if !s.is_empty() {
                meta.push(format!("operation={}", s));
            }
        }
        if let Some(s) = last.get("kind").and_then(|v| v.as_str()) {
            if !s.is_empty() {
                meta.push(format!("kind={}", s));
            }
        }
        if let Some(n) = last.get("count").and_then(|v| v.as_i64()) {
            meta.push(format!("count={}", n));
        }
        if let Some(n) = last.get("elapsedMs").and_then(|v| v.as_i64()) {
            meta.push(format!("elapsedMs={}", n));
        }
        if last.get("truncated").and_then(|v| v.as_bool()) == Some(true) {
            meta.push("truncated=true".into());
        }
        if !meta.is_empty() {
            txt.push_str(&format!("：{}", meta.join(", ")));
        }
        txt.push('\n');

        if let Some(err) = last.get("error").and_then(|v| v.as_str()) {
            if !err.is_empty() {
                txt.push_str("  错误：\n```\n");
                txt.push_str(err);
                txt.push_str("\n```\n");
            }
        }
        if let Some(preview) = last.get("previewJson").and_then(|v| v.as_str()) {
            if !preview.is_empty() {
                txt.push_str("  数据预览：\n```json\n");
                txt.push_str(preview);
                txt.push_str("\n```\n");
            }
        }
    }

    // 采样文档
    if let Some(docs) = schema.get("sampleDocs").and_then(|v| v.as_array()) {
        if !docs.is_empty() {
            txt.push_str(&format!("- 采样文档(共 {} 条):\n", docs.len()));
            txt.push_str("```json\n");
            txt.push_str(&serde_json::to_string_pretty(docs).unwrap_or_default());
            txt.push_str("\n```\n");
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
    profile: Option<LLMProfile>,
) -> Result<DispatchOutput, String> {
    // 1) 前端 profile 优先
    if let Some(p) = profile.clone() {
        match p.provider_kind.as_str() {
            "openai" => {
                let cfg = build_openai_from_profile(&p)?;
                let mut msgs = vec![ChatMessage {
                    role: "system".into(),
                    content: system.to_string(),
                }];
                msgs.extend(history.clone());
                msgs.push(ChatMessage {
                    role: "user".into(),
                    content: user_prompt.clone(),
                });
                let content = call_openai(&cfg, msgs, 0.2).await?;
                return Ok(DispatchOutput {
                    provider: "openai",
                    model: cfg.model,
                    content,
                });
            }
            "cursor" => {
                let cfg = build_cursor_from_profile(&p)?;
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
                    model: cfg.model.unwrap_or_else(|| "default".into()),
                    content,
                });
            }
            other => return Err(format!("未知 provider_kind: {}", other)),
        }
    }

    // 2) env / 文件回退
    if let Some(cfg) = build_openai_from_env() {
        let mut msgs = vec![ChatMessage {
            role: "system".into(),
            content: system.to_string(),
        }];
        msgs.extend(history);
        msgs.push(ChatMessage {
            role: "user".into(),
            content: user_prompt,
        });
        let content = call_openai(&cfg, msgs, 0.2).await?;
        return Ok(DispatchOutput {
            provider: "openai",
            model: cfg.model,
            content,
        });
    }
    if let Some(cfg) = build_cursor_from_env() {
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
            model: cfg.model.unwrap_or_else(|| "default".into()),
            content,
        });
    }

    Err(
        "未配置 LLM。请在左侧「LLM API」面板添加，或设置 OPENAI_API_KEY 环境变量。".into(),
    )
}

/* ---------------- helpers for detect_local ---------------- */

fn mask_key(s: &str) -> String {
    let s = s.trim();
    if s.is_empty() {
        return String::new();
    }
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= 8 {
        return "*".repeat(chars.len());
    }
    let head: String = chars.iter().take(4).collect();
    let tail: String = chars.iter().rev().take(4).collect::<String>().chars().rev().collect();
    format!("{}…{}", head, tail)
}

async fn cursor_agent_logged_in(bin: &str) -> Option<bool> {
    if read_env(&["CURSOR_API_KEY"]).is_some() {
        return Some(true);
    }
    let out = tokio::time::timeout(
        Duration::from_secs(5),
        Command::new(bin).arg("status").output(),
    )
    .await
    .ok()?
    .ok()?;
    if !out.status.success() {
        return Some(false);
    }
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    Some(!combined.contains("Not logged in"))
}

/* ---------------- tauri commands ---------------- */

#[tauri::command]
pub fn llm_status(profile: Option<LLMProfile>) -> Value {
    // 如果传入了 profile，直接报告 profile 状态
    if let Some(p) = profile {
        let kind = p.provider_kind.clone();
        return match kind.as_str() {
            "openai" => match build_openai_from_profile(&p) {
                Ok(cfg) => json!({
                    "ok": true,
                    "available": true,
                    "active": {
                        "providerKind": "openai",
                        "model": cfg.model,
                        "baseUrl": cfg.base_url,
                    }
                }),
                Err(e) => json!({"ok": true, "available": false, "error": e}),
            },
            "cursor" => match build_cursor_from_profile(&p) {
                Ok(cfg) => json!({
                    "ok": true,
                    "available": true,
                    "active": {
                        "providerKind": "cursor",
                        "binPath": cfg.bin,
                        "model": cfg.model,
                    }
                }),
                Err(e) => json!({"ok": true, "available": false, "error": e}),
            },
            other => json!({"ok": false, "error": format!("未知 providerKind: {}", other)}),
        };
    }

    // 没传 profile：报告 env / 文件回退状态
    let openai = build_openai_from_env();
    let cursor = build_cursor_from_env();
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
pub async fn llm_generate(
    prompt: String,
    schema: Option<Value>,
    profile: Option<LLMProfile>,
    locale: Option<String>,
) -> Result<Value, String> {
    let prompt = prompt.trim().to_string();
    if prompt.is_empty() {
        return Ok(json!({"ok": false, "error": "prompt 不能为空"}));
    }
    let user_prompt = format!(
        "{}\n\n用户需求：\n{}",
        describe_schema(&schema),
        prompt
    );
    // generate 主要是单行命令，本身不需要语言指令；但保留 locale 以便日后输出注释
    let system = with_locale(GENERATE_SYSTEM, locale.as_deref());
    match dispatch(&system, user_prompt, vec![], profile).await {
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
pub async fn llm_chat(
    messages: Vec<ChatMessage>,
    schema: Option<Value>,
    profile: Option<LLMProfile>,
    locale: Option<String>,
) -> Result<Value, String> {
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
    let system = with_locale(CHAT_SYSTEM, locale.as_deref());
    match dispatch(&system, user_prompt, history, profile).await {
        Ok(out) => Ok(json!({
            "ok": true,
            "reply": out.content,
            "provider": out.provider,
            "model": out.model,
        })),
        Err(e) => Ok(json!({"ok": false, "error": e})),
    }
}

/// 扫描本地环境，告诉前端「你电脑上能用哪些 LLM 配置」。
///
/// 返回结构（camelCase 给前端用）：
/// ```json
/// {
///   "ok": true,
///   "envFiles": [{ "path": "...", "exists": true, "keys": ["OPENAI_API_KEY", ...] }],
///   "envSnapshot": { "OPENAI_API_KEY": { "set": true, "maskedValue": "sk-…abcd", "source": "env|file:<path>" }, ... },
///   "openai": { "available": true, "baseUrl": "...", "model": "...", "apiKeyMasked": "sk-…", "source": "..." },
///   "cursor": { "binPath": "/usr/local/bin/cursor-agent", "loggedIn": true, "source": "PATH" }
/// }
/// ```
#[tauri::command]
pub async fn llm_detect_local() -> Value {
    // env 文件清单
    let mut env_files = vec![];
    for p in config_paths() {
        let exists = p.exists();
        let keys: Vec<String> = if exists {
            parse_env_file(&p).keys().cloned().collect()
        } else {
            vec![]
        };
        env_files.push(json!({
            "path": p.to_string_lossy(),
            "exists": exists,
            "keys": keys,
        }));
    }

    // 关心的 key + 来源
    let interesting = [
        "OPENAI_API_KEY",
        "OPENAI_BASE_URL",
        "OPENAI_MODEL",
        "LLM_API_KEY",
        "LLM_BASE_URL",
        "LLM_MODEL",
        "LLM_TIMEOUT",
        "CURSOR_AGENT_BIN",
        "CURSOR_API_KEY",
        "CURSOR_MODEL",
        "CURSOR_TIMEOUT",
    ];

    // 预先把所有文件解析一遍，建立 key -> (value, file_path)
    let mut file_origin: std::collections::HashMap<String, (String, String)> =
        std::collections::HashMap::new();
    for p in config_paths() {
        if !p.exists() {
            continue;
        }
        let kv = parse_env_file(&p);
        for (k, v) in kv {
            file_origin
                .entry(k)
                .or_insert((v, p.to_string_lossy().to_string()));
        }
    }

    let mut env_snapshot = serde_json::Map::new();
    for k in interesting {
        // 进程 env 优先
        if let Ok(v) = std::env::var(k) {
            if !v.is_empty() {
                let secret = k.contains("KEY");
                env_snapshot.insert(
                    k.to_string(),
                    json!({
                        "set": true,
                        "source": "env",
                        "maskedValue": if secret { mask_key(&v) } else { v.clone() },
                    }),
                );
                continue;
            }
        }
        if let Some((v, path)) = file_origin.get(k) {
            let secret = k.contains("KEY");
            env_snapshot.insert(
                k.to_string(),
                json!({
                    "set": true,
                    "source": format!("file:{}", path),
                    "maskedValue": if secret { mask_key(v) } else { v.clone() },
                }),
            );
            continue;
        }
        env_snapshot.insert(k.to_string(), json!({ "set": false }));
    }

    // OpenAI 兼容是否可用
    let openai = build_openai_from_env().map(|c| {
        let masked = mask_key(&c.api_key);
        // 找出 OPENAI_API_KEY 的来源（用于 UI 提示）
        let src = env_snapshot
            .get("OPENAI_API_KEY")
            .or_else(|| env_snapshot.get("LLM_API_KEY"))
            .and_then(|v| v.get("source"))
            .cloned()
            .unwrap_or_else(|| json!("unknown"));
        json!({
            "available": true,
            "baseUrl": c.base_url,
            "model": c.model,
            "apiKeyMasked": masked,
            "source": src,
        })
    });

    // cursor-agent
    let cursor_bin = detect_cursor_binary();
    let cursor_block = if let Some(bin) = cursor_bin.clone() {
        let logged_in = cursor_agent_logged_in(&bin).await;
        let src = if env_snapshot
            .get("CURSOR_AGENT_BIN")
            .and_then(|v| v.get("set"))
            .and_then(|v| v.as_bool())
            == Some(true)
        {
            "env-or-file"
        } else {
            "PATH"
        };
        json!({
            "binPath": bin,
            "loggedIn": logged_in,
            "source": src,
        })
    } else {
        json!(null)
    };

    json!({
        "ok": true,
        "envFiles": env_files,
        "envSnapshot": env_snapshot,
        "openai": openai,
        "cursor": cursor_block,
    })
}
