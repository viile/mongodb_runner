//! 与 git_commit.py 风格一致的轻量 env 解析：
//!   - 优先从进程环境变量取
//!   - 否则按顺序查找：
//!       1) 当前工作目录的 `.mongo-runner.env`（dev 阶段方便）
//!       2) ~/.config/mongodb-runner/llm.env（XDG）
//!       3) (macOS) ~/Library/Application Support/dev.mongodb-runner.app/llm.env
//!
//! 文件格式：
//!     KEY=value
//!     KEY="value with spaces"
//!     export KEY=value
//!     # comments

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::OnceLock;

pub fn parse_env_file(path: &PathBuf) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let Ok(content) = std::fs::read_to_string(path) else {
        return map;
    };
    for raw in content.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let cleaned = line.strip_prefix("export ").unwrap_or(line).trim();
        let Some(eq) = cleaned.find('=') else { continue };
        let key = cleaned[..eq].trim().to_string();
        let mut value = cleaned[eq + 1..].trim().to_string();
        if (value.starts_with('"') && value.ends_with('"'))
            || (value.starts_with('\'') && value.ends_with('\''))
        {
            value = value[1..value.len() - 1].to_string();
        }
        if !key.is_empty() {
            map.insert(key, value);
        }
    }
    map
}

pub fn config_paths() -> Vec<PathBuf> {
    collect_config_paths()
}

fn collect_config_paths() -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = vec![];

    // 1) 当前工作目录（开发时 tauri dev 的 cwd 是 src-tauri/，所以也看一下父目录）
    if let Ok(cwd) = std::env::current_dir() {
        out.push(cwd.join(".mongo-runner.env"));
        if let Some(parent) = cwd.parent() {
            out.push(parent.join(".mongo-runner.env"));
        }
    }

    // 2) XDG / 用户 config dir
    if let Some(cfg) = dirs::config_dir() {
        out.push(cfg.join("mongodb-runner").join("llm.env"));
    }

    // 3) home dir 作为兜底
    if let Some(home) = dirs::home_dir() {
        out.push(home.join(".mongodb-runner.env"));
    }

    out
}

fn local_env() -> &'static HashMap<String, String> {
    static CELL: OnceLock<HashMap<String, String>> = OnceLock::new();
    CELL.get_or_init(|| {
        let mut acc: HashMap<String, String> = HashMap::new();
        for p in collect_config_paths() {
            for (k, v) in parse_env_file(&p) {
                acc.entry(k).or_insert(v);
            }
        }
        acc
    })
}

pub fn read_env(keys: &[&str]) -> Option<String> {
    for k in keys {
        if let Ok(v) = std::env::var(k) {
            if !v.is_empty() {
                return Some(v);
            }
        }
    }
    let map = local_env();
    for k in keys {
        if let Some(v) = map.get(*k) {
            if !v.is_empty() {
                return Some(v.clone());
            }
        }
    }
    None
}
