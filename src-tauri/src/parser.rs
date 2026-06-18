//! mongosh 风格命令解析器（与之前 Node 后端的 parseMongoCommand 行为等价）。
//!
//! 入参：
//!   db.<col>.<op>(<arg1>, <arg2>, ...).limit(n).sort({...}).skip(n).project({...}).allowDiskUse(true)
//!
//! 出参（结构化命令）：
//!   ParsedCommand { collection, op, args (Vec<Bson>), modifiers: Modifiers }
//!
//! 参数支持 MongoDB Extended JSON v2（{"$oid":..}, {"$date":..} 等）+ mongosh 风格的
//! unquoted key / 单引号字符串。我们用正则把它们规范化为合法 JSON，再用 bson::Bson::try_from
//! 走 EJSON relaxed 反序列化。
//!
//! 安全：纯文本解析，不开 shell、不 eval JS，无注入风险。

use bson::Bson;
use serde::Serialize;
use std::sync::OnceLock;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Modifiers {
    pub limit: Option<i64>,
    pub skip: Option<i64>,
    pub sort: Option<Bson>,
    pub projection: Option<Bson>,
    pub allow_disk_use: Option<bool>,
}

impl Default for Modifiers {
    fn default() -> Self {
        Self {
            limit: None,
            skip: None,
            sort: None,
            projection: None,
            allow_disk_use: None,
        }
    }
}

#[derive(Debug)]
pub struct ParsedCommand {
    pub collection: String,
    pub op: String,
    pub args: Vec<Bson>,
    pub modifiers: Modifiers,
}

const ALL_OPS: &[&str] = &[
    // read
    "find",
    "findOne",
    "aggregate",
    "countDocuments",
    "estimatedDocumentCount",
    "distinct",
    // write
    "insertOne",
    "insertMany",
    "updateOne",
    "updateMany",
    "replaceOne",
    "deleteOne",
    "deleteMany",
    "findOneAndUpdate",
    "findOneAndDelete",
    "findOneAndReplace",
];

/// 在顶层逗号处切分 (...args...) 中的参数串，正确处理嵌套 {} [] () 和字符串里的逗号。
fn split_top_level_args(text: &str) -> Vec<String> {
    let mut out: Vec<String> = vec![];
    let mut buf = String::new();
    let mut depth = 0i32;
    let mut quote: Option<char> = None;
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0usize;

    while i < chars.len() {
        let ch = chars[i];
        match quote {
            Some(q) => {
                buf.push(ch);
                if ch == '\\' && i + 1 < chars.len() {
                    buf.push(chars[i + 1]);
                    i += 2;
                    continue;
                }
                if ch == q {
                    quote = None;
                }
                i += 1;
            }
            None => {
                if ch == '"' || ch == '\'' {
                    quote = Some(ch);
                    buf.push(ch);
                } else if ch == '{' || ch == '[' || ch == '(' {
                    depth += 1;
                    buf.push(ch);
                } else if ch == '}' || ch == ']' || ch == ')' {
                    depth -= 1;
                    buf.push(ch);
                } else if ch == ',' && depth == 0 {
                    let t = buf.trim().to_string();
                    if !t.is_empty() {
                        out.push(t);
                    }
                    buf.clear();
                } else {
                    buf.push(ch);
                }
                i += 1;
            }
        }
    }
    let t = buf.trim().to_string();
    if !t.is_empty() {
        out.push(t);
    }
    out
}

/// 找出从 `start` 处 `(` 起，匹配的 `)` 的字节索引（包含）。
fn find_matching_paren(s: &str, start: usize) -> Option<usize> {
    let bytes: &[u8] = s.as_bytes();
    if start >= bytes.len() || bytes[start] != b'(' {
        return None;
    }
    let mut depth = 0i32;
    let mut quote: Option<u8> = None;
    let mut i = start;
    while i < bytes.len() {
        let ch = bytes[i];
        match quote {
            Some(q) => {
                if ch == b'\\' && i + 1 < bytes.len() {
                    i += 2;
                    continue;
                }
                if ch == q {
                    quote = None;
                }
            }
            None => {
                if ch == b'"' || ch == b'\'' {
                    quote = Some(ch);
                } else if ch == b'(' {
                    depth += 1;
                } else if ch == b')' {
                    depth -= 1;
                    if depth == 0 {
                        return Some(i);
                    }
                }
            }
        }
        i += 1;
    }
    None
}

/// 解析单个 EJSON 参数：先把 mongosh 风格规范化为合法 JSON，再走 bson EJSON。
fn parse_ejson_arg(text: &str) -> Result<Bson, String> {
    static SINGLE_Q: OnceLock<regex::Regex> = OnceLock::new();
    static UNQUOTED_KEY: OnceLock<regex::Regex> = OnceLock::new();

    let single_q = SINGLE_Q.get_or_init(|| regex::Regex::new(r"'([^'\\]*(?:\\.[^'\\]*)*)'").unwrap());
    let unquoted_key = UNQUOTED_KEY
        .get_or_init(|| regex::Regex::new(r"([\{,]\s*)([\$A-Za-z_][\w\$]*)\s*:").unwrap());

    // 'xxx' → "xxx"（仅顶层简单替换；对包含双引号的字符串会失败，但这里相比 unsafe-eval 安全得多）
    let after_quotes = single_q.replace_all(text, |caps: &regex::Captures| {
        let inner = &caps[1];
        // 把 \' 还原为 '（mongosh 里转义的单引号）
        let restored = inner.replace("\\'", "'");
        serde_json::to_string(&restored).unwrap_or_else(|_| format!("\"{}\"", inner))
    });

    // 给 unquoted key 加引号: `{foo:1}` 或 `, foo: 1` 或 `{ $oid: ..}`
    let normalized = unquoted_key.replace_all(&after_quotes, r#"$1"$2":"#);

    let value: serde_json::Value = serde_json::from_str(&normalized)
        .map_err(|e| format!("非合法 JSON: {} (输入: {})", e, normalized))?;

    Bson::try_from(value).map_err(|e| format!("EJSON 转换失败: {}", e))
}

/// 检查链式调用名是否合法。
fn handle_modifier(name: &str, value: Option<Bson>, mods: &mut Modifiers) -> Result<(), String> {
    match name {
        "limit" => {
            mods.limit = match value {
                Some(Bson::Int32(n)) => Some(n as i64),
                Some(Bson::Int64(n)) => Some(n),
                Some(Bson::Double(n)) => Some(n as i64),
                _ => return Err(".limit() 参数必须是整数".into()),
            };
        }
        "skip" => {
            mods.skip = match value {
                Some(Bson::Int32(n)) => Some(n as i64),
                Some(Bson::Int64(n)) => Some(n),
                Some(Bson::Double(n)) => Some(n as i64),
                _ => return Err(".skip() 参数必须是整数".into()),
            };
        }
        "sort" => {
            mods.sort = value;
        }
        "project" | "projection" => {
            mods.projection = value;
        }
        "allowDiskUse" => {
            mods.allow_disk_use = match value {
                Some(Bson::Boolean(b)) => Some(b),
                None => Some(true),
                _ => Some(true),
            };
        }
        other => return Err(format!("暂不支持的链式调用: .{}(...)", other)),
    }
    Ok(())
}

/// 主入口。
pub fn parse_mongo_command(input: &str) -> Result<ParsedCommand, String> {
    static HEAD: OnceLock<regex::Regex> = OnceLock::new();
    let head = HEAD.get_or_init(|| {
        regex::Regex::new(r"^db\s*\.\s*([^.\s]+)\s*\.\s*([A-Za-z_\$][\w\$]*)\s*\(").unwrap()
    });

    // 去掉行尾 ; 和外围空白
    let text: String = {
        let trimmed = input.trim();
        let stripped = trimmed.trim_end_matches(|c| c == ';' || c == ' ' || c == '\t');
        stripped.to_string()
    };

    let cap = head
        .captures(&text)
        .ok_or_else(|| "命令必须以 `db.<collection>.<op>(...)` 开头".to_string())?;
    let collection = cap.get(1).unwrap().as_str().to_string();
    let op = cap.get(2).unwrap().as_str().to_string();
    if !ALL_OPS.contains(&op.as_str()) {
        return Err(format!(
            "不支持的操作: {}。允许的操作: {}",
            op,
            ALL_OPS.join(", ")
        ));
    }

    // 第一对顶层 (...)
    let head_match = cap.get(0).unwrap();
    // 注意：head 里以 `(` 结尾，所以 `(` 位置就是 head_match.end() - 1
    let open_idx = head_match.end() - 1;
    let close_idx =
        find_matching_paren(&text, open_idx).ok_or_else(|| "括号不匹配".to_string())?;
    let arg_text = &text[open_idx + 1..close_idx];

    let arg_parts = split_top_level_args(arg_text);
    let mut args: Vec<Bson> = vec![];
    for p in arg_parts {
        args.push(parse_ejson_arg(&p).map_err(|e| format!("参数解析失败: {} ({})", p, e))?);
    }

    // 链式调用
    let mut modifiers = Modifiers::default();
    let mut rest = text[close_idx + 1..].trim().to_string();

    static CHAIN: OnceLock<regex::Regex> = OnceLock::new();
    let chain = CHAIN.get_or_init(|| regex::Regex::new(r"^\.\s*([A-Za-z_\$][\w\$]*)\s*\(").unwrap());

    while !rest.is_empty() {
        let cm = chain
            .captures(&rest)
            .ok_or_else(|| format!("无法识别的链式调用: {}", rest))?;
        let name = cm.get(1).unwrap().as_str().to_string();
        let cm0 = cm.get(0).unwrap();
        let c_open = cm0.end() - 1;
        let c_close = find_matching_paren(&rest, c_open)
            .ok_or_else(|| format!(".{}(...) 括号不匹配", name))?;
        let inner = rest[c_open + 1..c_close].trim().to_string();
        let value: Option<Bson> = if inner.is_empty() {
            None
        } else {
            Some(parse_ejson_arg(&inner).map_err(|e| format!(".{}(...) 参数解析失败: {}", name, e))?)
        };
        handle_modifier(&name, value, &mut modifiers)?;

        rest = rest[c_close + 1..].trim().to_string();
    }

    Ok(ParsedCommand {
        collection,
        op,
        args,
        modifiers,
    })
}
