#!/usr/bin/env python3
"""Auto-summarize current diff and run `git commit -m "..."` with confirmation."""

from __future__ import annotations

import argparse
import json
import os
import re
import shlex
import shutil
import subprocess
import sys
import tempfile
import urllib.error
import urllib.request
from collections import Counter
from dataclasses import dataclass
from typing import Optional


DEFAULT_CONFIG_FILE = ".git-commit.env"
DEFAULT_LLM_TIMEOUT = 60
DEFAULT_LLM_MODEL = "gpt-4o-mini"
DEFAULT_CURSOR_TIMEOUT = 120
DEFAULT_DIFF_BUDGET = 12000

PROVIDER_CURSOR = "cursor"
PROVIDER_OPENAI = "openai"
PROVIDER_HEURISTIC = "heuristic"
PROVIDER_AUTO = "auto"

TYPE_PATTERNS: list[tuple[str, re.Pattern[str]]] = [
    ("docs", re.compile(r"(^|/)(readme|README|docs?/|.*\.md$)")),
    ("test", re.compile(r"(^|/)(tests?/|.*Test\.php$|.*\.test\.[jt]sx?$|.*_test\.go$)")),
    ("ci", re.compile(r"(^|/)(\.github/|\.gitlab-ci\.yml$|Jenkinsfile|\.drone\.yml$)")),
    ("build", re.compile(r"(^|/)(Dockerfile|docker-compose|composer\.json$|composer\.lock$|package\.json$|package-lock\.json$|yarn\.lock$|go\.mod$|go\.sum$|pyproject\.toml$|requirements.*\.txt$)")),
    ("style", re.compile(r"\.(css|scss|less|sass)$")),
    ("chore", re.compile(r"(^|/)(\.env|\.gitignore$|\.editorconfig$|\.prettierrc|\.eslintrc)")),
]


@dataclass
class LLMConfig:
    api_key: str
    base_url: str
    model: str
    timeout: int


@dataclass
class CursorConfig:
    binary: str
    model: Optional[str]
    timeout: int


class CommitError(RuntimeError):
    pass


def run_git(args: list[str], check: bool = True) -> str:
    result = subprocess.run(
        ["git", *args],
        check=check,
        capture_output=True,
        text=True,
    )
    return result.stdout.rstrip("\n")


def run_git_stream(args: list[str]) -> int:
    result = subprocess.run(["git", *args])
    return result.returncode


def load_local_env_file(path: str) -> dict[str, str]:
    if not os.path.exists(path):
        return {}

    values: dict[str, str] = {}
    with open(path, "r", encoding="utf-8") as handle:
        for raw_line in handle:
            line = raw_line.strip()
            if not line or line.startswith("#"):
                continue
            if line.startswith("export "):
                line = line[len("export "):].strip()
            if "=" not in line:
                continue
            key, value = line.split("=", 1)
            key = key.strip()
            value = value.strip()
            if not key:
                continue
            try:
                parsed = shlex.split(value, comments=False, posix=True)
                values[key] = parsed[0] if parsed else ""
            except ValueError:
                values[key] = value.strip("\"'")
    return values


def get_staged_files() -> list[tuple[str, str]]:
    """Return [(status, path), ...] for staged files."""
    raw = run_git(["diff", "--cached", "--name-status"])
    rows: list[tuple[str, str]] = []
    for line in raw.splitlines():
        if not line.strip():
            continue
        parts = line.split("\t")
        status = parts[0]
        path = parts[-1]
        rows.append((status, path))
    return rows


def get_working_tree_dirty() -> bool:
    raw = run_git(["status", "--porcelain"])
    return bool(raw.strip())


def get_unstaged_files() -> list[tuple[str, str]]:
    raw = run_git(["status", "--porcelain"])
    rows: list[tuple[str, str]] = []
    for line in raw.splitlines():
        if len(line) < 3:
            continue
        index_status = line[0]
        worktree_status = line[1]
        path = line[3:]
        if "->" in path:
            path = path.split("->", 1)[1].strip()
        if worktree_status != " " or index_status == "?":
            rows.append((worktree_status if worktree_status != " " else index_status, path))
    return rows


def stage_all() -> None:
    subprocess.run(["git", "add", "-A"], check=True)


def get_diff_stat() -> str:
    return run_git(["diff", "--cached", "--stat"])


def get_diff_numstat() -> list[tuple[int, int, str]]:
    raw = run_git(["diff", "--cached", "--numstat"])
    items: list[tuple[int, int, str]] = []
    for line in raw.splitlines():
        parts = line.split("\t")
        if len(parts) != 3:
            continue
        added_s, removed_s, path = parts
        try:
            added = int(added_s)
            removed = int(removed_s)
        except ValueError:
            added = 0
            removed = 0
        items.append((added, removed, path))
    return items


def get_diff_text(max_chars: int) -> str:
    raw = run_git(["diff", "--cached", "--no-color"])
    if len(raw) <= max_chars:
        return raw
    return raw[:max_chars] + f"\n\n... [diff 截断，原始长度 {len(raw)} 字符]"


def detect_type(files: list[str]) -> str:
    if not files:
        return "chore"

    counts: Counter[str] = Counter()
    for path in files:
        matched = False
        for type_name, pattern in TYPE_PATTERNS:
            if pattern.search(path):
                counts[type_name] += 1
                matched = True
                break
        if not matched:
            counts["feat"] += 1

    most_common, _ = counts.most_common(1)[0]
    return most_common


def detect_scope(files: list[str]) -> Optional[str]:
    if not files:
        return None

    scopes: Counter[str] = Counter()
    for path in files:
        parts = path.split("/")
        if len(parts) >= 3 and parts[0] == "app":
            scopes[parts[1].lower()] += 1
        elif len(parts) >= 2:
            scopes[parts[0].lower()] += 1
        else:
            scopes[parts[0].lower()] += 1

    if not scopes:
        return None

    top, count = scopes.most_common(1)[0]
    if count < max(1, len(files) // 3):
        return None
    sanitized = re.sub(r"[^a-z0-9_-]+", "", top)
    return sanitized or None


def summarize_status(files: list[tuple[str, str]]) -> str:
    groups: Counter[str] = Counter()
    for status, _ in files:
        s = status[0]
        if s == "A":
            groups["新增"] += 1
        elif s == "D":
            groups["删除"] += 1
        elif s == "R":
            groups["重命名"] += 1
        elif s == "M":
            groups["修改"] += 1
        else:
            groups["变更"] += 1

    parts = [f"{action} {count} 个文件" for action, count in groups.items()]
    return "，".join(parts)


def generate_heuristic_message(
    staged: list[tuple[str, str]],
    numstat: list[tuple[int, int, str]],
) -> str:
    files = [path for _, path in staged]
    commit_type = detect_type(files)
    scope = detect_scope(files)

    total_added = sum(a for a, _, _ in numstat)
    total_removed = sum(r for _, r, _ in numstat)

    subject_core = summarize_status(staged)
    subject = f"{subject_core} (+{total_added} -{total_removed})"

    prefix = f"{commit_type}({scope})" if scope else commit_type
    header = f"{prefix}: {subject}"

    body_lines = []
    sorted_numstat = sorted(
        numstat,
        key=lambda x: (x[0] + x[1]),
        reverse=True,
    )
    for added, removed, path in sorted_numstat[:10]:
        body_lines.append(f"- {path} (+{added} -{removed})")

    if len(sorted_numstat) > 10:
        body_lines.append(f"- ... 以及其他 {len(sorted_numstat) - 10} 个文件")

    body = "\n".join(body_lines)
    return f"{header}\n\n{body}" if body else header


def build_cursor_config(local_env: dict[str, str]) -> Optional[CursorConfig]:
    binary = (
        os.getenv("CURSOR_AGENT_BIN")
        or local_env.get("CURSOR_AGENT_BIN")
        or shutil.which("cursor-agent")
    )
    if not binary:
        return None

    model = (
        os.getenv("CURSOR_MODEL")
        or local_env.get("CURSOR_MODEL")
        or None
    )
    timeout_raw = (
        os.getenv("CURSOR_TIMEOUT")
        or local_env.get("CURSOR_TIMEOUT")
        or str(DEFAULT_CURSOR_TIMEOUT)
    )
    try:
        timeout = int(timeout_raw)
    except ValueError:
        timeout = DEFAULT_CURSOR_TIMEOUT

    return CursorConfig(binary=binary, model=model, timeout=timeout)


def cursor_agent_logged_in(binary: str) -> bool:
    if os.getenv("CURSOR_API_KEY"):
        return True
    try:
        result = subprocess.run(
            [binary, "status"],
            capture_output=True,
            text=True,
            timeout=10,
        )
    except (subprocess.TimeoutExpired, FileNotFoundError):
        return False
    if result.returncode != 0:
        return False
    return "Not logged in" not in (result.stdout + result.stderr)


def generate_cursor_message(
    config: CursorConfig,
    diff_text: str,
    stat: str,
) -> str:
    prompt = (
        "你是一个资深工程师。请根据下面的 git diff 生成简洁、准确的英文 Conventional Commits 提交信息。\n"
        "输出格式严格遵循：\n"
        "第一行: <type>(<scope>): <subject>，subject 用一句话概括，不超过 72 字符。\n"
        "type 取值: feat / fix / refactor / docs / style / test / chore / build / ci / perf。\n"
        "空一行后是可选的 body，使用 `-` 列出关键改动，每条不超过 100 字符。\n"
        "只输出提交信息本身，不要任何额外解释、不要 markdown 代码块。\n\n"
        f"以下是 `git diff --cached --stat`:\n{stat}\n\n"
        f"以下是 `git diff --cached`:\n{diff_text}"
    )

    args = [
        config.binary,
        "--print",
        "--mode", "ask",
        "--output-format", "text",
        "--trust",
    ]
    if config.model:
        args.extend(["--model", config.model])
    args.append(prompt)

    try:
        result = subprocess.run(
            args,
            capture_output=True,
            text=True,
            timeout=config.timeout,
        )
    except subprocess.TimeoutExpired as exc:
        raise CommitError(f"cursor-agent 超时 (>{config.timeout}s)") from exc
    except FileNotFoundError as exc:
        raise CommitError(f"未找到 cursor-agent 可执行文件: {config.binary}") from exc

    if result.returncode != 0:
        stderr = (result.stderr or "").strip()
        raise CommitError(f"cursor-agent 调用失败 (exit {result.returncode}): {stderr}")

    output = (result.stdout or "").strip()
    if not output:
        raise CommitError("cursor-agent 没有返回任何内容")

    cleaned = re.sub(r"^```[a-zA-Z]*\n?", "", output)
    cleaned = re.sub(r"\n?```$", "", cleaned)
    return cleaned.strip()


def build_llm_config(local_env: dict[str, str]) -> Optional[LLMConfig]:
    api_key = (
        os.getenv("OPENAI_API_KEY")
        or local_env.get("OPENAI_API_KEY")
        or os.getenv("LLM_API_KEY")
        or local_env.get("LLM_API_KEY")
    )
    if not api_key:
        return None

    base_url = (
        os.getenv("OPENAI_BASE_URL")
        or local_env.get("OPENAI_BASE_URL")
        or os.getenv("LLM_BASE_URL")
        or local_env.get("LLM_BASE_URL")
        or "https://api.openai.com"
    ).rstrip("/")
    if not base_url.endswith("/v1"):
        if "/v1" not in base_url:
            base_url = f"{base_url}/v1"

    model = (
        os.getenv("OPENAI_MODEL")
        or local_env.get("OPENAI_MODEL")
        or os.getenv("LLM_MODEL")
        or local_env.get("LLM_MODEL")
        or DEFAULT_LLM_MODEL
    )

    timeout_raw = (
        os.getenv("LLM_TIMEOUT")
        or local_env.get("LLM_TIMEOUT")
        or str(DEFAULT_LLM_TIMEOUT)
    )
    try:
        timeout = int(timeout_raw)
    except ValueError:
        timeout = DEFAULT_LLM_TIMEOUT

    return LLMConfig(api_key=api_key, base_url=base_url, model=model, timeout=timeout)


def generate_llm_message(config: LLMConfig, diff_text: str, stat: str) -> str:
    system_prompt = (
        "你是一个资深工程师，根据 git diff 生成简洁、准确的中文 Conventional Commits 提交信息。"
        "输出格式：\n"
        "第一行: <type>(<scope>): <subject>，subject 用一句话概括，不超过 72 字符。\n"
        "type 取值: feat / fix / refactor / docs / style / test / chore / build / ci / perf。\n"
        "空一行后是可选的 body，使用 `-` 列出关键改动，每条不超过 100 字符。\n"
        "只输出提交信息本身，不要任何额外解释或代码块标记。"
    )
    user_prompt = (
        f"以下是 `git diff --cached --stat`:\n```\n{stat}\n```\n\n"
        f"以下是 `git diff --cached` 内容:\n```diff\n{diff_text}\n```\n\n"
        "请生成提交信息。"
    )

    payload = {
        "model": config.model,
        "messages": [
            {"role": "system", "content": system_prompt},
            {"role": "user", "content": user_prompt},
        ],
        "temperature": 0.2,
    }

    request = urllib.request.Request(
        url=f"{config.base_url}/chat/completions",
        data=json.dumps(payload).encode("utf-8"),
        headers={
            "Authorization": f"Bearer {config.api_key}",
            "Content-Type": "application/json",
        },
        method="POST",
    )

    try:
        with urllib.request.urlopen(request, timeout=config.timeout) as response:
            raw = response.read().decode("utf-8")
    except urllib.error.HTTPError as exc:
        body = exc.read().decode("utf-8", errors="replace")
        raise CommitError(f"LLM 接口失败 HTTP {exc.code}: {body}") from exc
    except urllib.error.URLError as exc:
        raise CommitError(f"无法连接 LLM 接口: {exc.reason}") from exc

    try:
        data = json.loads(raw)
        message = data["choices"][0]["message"]["content"].strip()
    except (KeyError, IndexError, json.JSONDecodeError) as exc:
        raise CommitError(f"LLM 返回了无法解析的响应: {raw}") from exc

    cleaned = re.sub(r"^```[a-zA-Z]*\n?", "", message)
    cleaned = re.sub(r"\n?```$", "", cleaned)
    return cleaned.strip()


def print_section(title: str) -> None:
    print()
    print(f"---- {title} ----")


def confirm_or_edit(message: str) -> tuple[bool, str]:
    """Return (confirmed, possibly-edited message)."""
    print()
    try:
        answer = input(
            "回车=确认提交，e=编辑后提交，其他=取消: "
        )
    except (EOFError, KeyboardInterrupt):
        print()
        return False, message

    answer = answer.strip().lower()
    if answer == "":
        return True, message
    if answer == "e":
        edited = edit_message(message)
        if edited is None or not edited.strip():
            return False, message
        return True, edited
    return False, message


def edit_message(initial: str) -> Optional[str]:
    editor = os.getenv("GIT_EDITOR") or os.getenv("EDITOR") or "vi"
    with tempfile.NamedTemporaryFile(
        mode="w+",
        suffix=".COMMIT_EDITMSG",
        delete=False,
        encoding="utf-8",
    ) as handle:
        handle.write(initial)
        handle.write(
            "\n\n# 以 `#` 开头的行会被忽略。保存退出即提交，留空则取消。\n"
        )
        tmp_path = handle.name

    try:
        rc = subprocess.run(shlex.split(editor) + [tmp_path]).returncode
        if rc != 0:
            return None
        with open(tmp_path, "r", encoding="utf-8") as handle:
            content = handle.read()
    finally:
        os.unlink(tmp_path)

    lines = [
        line for line in content.splitlines() if not line.lstrip().startswith("#")
    ]
    return "\n".join(lines).strip()


def resolve_and_generate(
    provider: str,
    local_env: dict[str, str],
    staged: list[tuple[str, str]],
    numstat: list[tuple[int, int, str]],
    stat_text: str,
    diff_budget: int,
) -> tuple[str, str]:
    """Return (message, source-label) honoring provider preference with fallback."""

    def try_cursor() -> Optional[tuple[str, str]]:
        cfg = build_cursor_config(local_env)
        if not cfg:
            return None
        if not cursor_agent_logged_in(cfg.binary):
            print(
                f"提示: 已找到 cursor-agent ({cfg.binary}) 但未登录，"
                "执行 `cursor-agent login` 或设置 CURSOR_API_KEY 后即可使用。",
                file=sys.stderr,
            )
            return None
        diff_text = get_diff_text(diff_budget)
        label = f"cursor-agent ({cfg.model})" if cfg.model else "cursor-agent"
        print_section(f"调用 {label} 生成提交信息")
        msg = generate_cursor_message(cfg, diff_text, stat_text)
        return msg, label

    def try_openai() -> Optional[tuple[str, str]]:
        cfg = build_llm_config(local_env)
        if not cfg:
            return None
        diff_text = get_diff_text(diff_budget)
        print_section(f"调用 LLM 生成提交信息 (model={cfg.model})")
        msg = generate_llm_message(cfg, diff_text, stat_text)
        return msg, f"LLM ({cfg.model})"

    def heuristic() -> tuple[str, str]:
        return generate_heuristic_message(staged, numstat), "启发式"

    if provider == PROVIDER_HEURISTIC:
        return heuristic()

    if provider == PROVIDER_CURSOR:
        try:
            result = try_cursor()
        except CommitError as exc:
            print(f"cursor-agent 生成失败，回退到启发式: {exc}", file=sys.stderr)
            return generate_heuristic_message(staged, numstat), "启发式 (cursor 失败回退)"
        if result is None:
            print("cursor-agent 不可用，回退到启发式。", file=sys.stderr)
            return generate_heuristic_message(staged, numstat), "启发式 (cursor 不可用)"
        return result

    if provider == PROVIDER_OPENAI:
        try:
            result = try_openai()
        except CommitError as exc:
            print(f"LLM 生成失败，回退到启发式: {exc}", file=sys.stderr)
            return generate_heuristic_message(staged, numstat), "启发式 (LLM 失败回退)"
        if result is None:
            print("未配置 OpenAI API key，回退到启发式。", file=sys.stderr)
            return generate_heuristic_message(staged, numstat), "启发式 (LLM 未配置)"
        return result

    for attempt in (try_cursor, try_openai):
        try:
            result = attempt()
        except CommitError as exc:
            print(f"{attempt.__name__} 生成失败，尝试下一个: {exc}", file=sys.stderr)
            continue
        if result is not None:
            return result

    return heuristic()


def do_commit(message: str, no_verify: bool) -> int:
    args = ["commit", "-m", message]
    if no_verify:
        args.append("--no-verify")
    print()
    print(f"==> git commit -m {shlex.quote(message.splitlines()[0] + '...')}")
    return run_git_stream(args)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="自动总结当前 staged/working 变更，生成 commit message 并提交。",
    )
    parser.add_argument(
        "-a",
        "--all",
        action="store_true",
        help="提交前先 git add -A，把工作区所有变更（含新文件）一起暂存",
    )
    parser.add_argument(
        "-y",
        "--yes",
        action="store_true",
        help="跳过确认，直接使用生成的信息提交",
    )
    parser.add_argument(
        "--provider",
        choices=[PROVIDER_AUTO, PROVIDER_CURSOR, PROVIDER_OPENAI, PROVIDER_HEURISTIC],
        default=PROVIDER_AUTO,
        help=(
            "选择生成 message 的提供方："
            "auto=按可用性自动选择（cursor-agent > openai > heuristic），"
            "cursor=本地 cursor-agent CLI，"
            "openai=OpenAI 兼容 API，"
            "heuristic=纯启发式规则"
        ),
    )
    parser.add_argument(
        "--no-llm",
        action="store_true",
        help="等价于 --provider heuristic",
    )
    parser.add_argument(
        "--no-verify",
        action="store_true",
        help="提交时跳过 pre-commit / commit-msg 钩子",
    )
    parser.add_argument(
        "-m",
        "--message",
        default=None,
        help="直接给定 message，跳过自动总结（仍会走确认流程，除非加 -y）",
    )
    parser.add_argument(
        "--diff-budget",
        type=int,
        default=DEFAULT_DIFF_BUDGET,
        help=f"发给 LLM 的 diff 最大字符数，默认 {DEFAULT_DIFF_BUDGET}",
    )
    return parser.parse_args()


def main() -> int:
    try:
        args = parse_args()
        local_env = load_local_env_file(DEFAULT_CONFIG_FILE)

        if args.all and get_working_tree_dirty():
            print("==> git add -A")
            stage_all()

        staged = get_staged_files()
        if not staged:
            unstaged = get_unstaged_files()
            if unstaged:
                print("没有任何暂存的改动。检测到工作区有未暂存的变更:")
                for status, path in unstaged[:20]:
                    print(f"  {status}  {path}")
                if len(unstaged) > 20:
                    print(f"  ... 以及其他 {len(unstaged) - 20} 个文件")
                print()
                print("可以使用 `./commit -a` 自动暂存所有改动，或先手动 `git add` 后再运行。")
            else:
                print("工作区干净，没有可提交的内容。")
            return 1

        numstat = get_diff_numstat()
        stat_text = get_diff_stat()

        print_section("即将提交的文件")
        print(stat_text or "(无统计输出)")

        if args.message:
            message = args.message.strip()
            source = "手动指定"
        else:
            provider = PROVIDER_HEURISTIC if args.no_llm else args.provider
            message, source = resolve_and_generate(
                provider=provider,
                local_env=local_env,
                staged=staged,
                numstat=numstat,
                stat_text=stat_text,
                diff_budget=args.diff_budget,
            )

        print_section(f"建议的提交信息 [{source}]")
        print(message)

        if args.yes:
            confirmed = True
            final_message = message
        else:
            confirmed, final_message = confirm_or_edit(message)

        if not confirmed:
            print("已取消，未执行 commit。")
            return 1

        return do_commit(final_message, no_verify=args.no_verify)
    except CommitError as exc:
        print(f"错误: {exc}", file=sys.stderr)
        return 2
    except subprocess.CalledProcessError as exc:
        stderr = (exc.stderr or "").strip() if isinstance(exc.stderr, str) else ""
        print(stderr or str(exc), file=sys.stderr)
        return 3


if __name__ == "__main__":
    sys.exit(main())
