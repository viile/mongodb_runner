/**
 * 把 LLM 输出的 mongosh 风格单行命令美化成多行可读形式，再写入编辑器。
 *
 * 设计原则：
 *  - 永远不丢失用户的命令：任何环节失败都原样返回 `input`。
 *  - 输出仍然能被 Rust 端 `parser.rs` 直接吃下（顶层平衡括号 + 链式 `.trim()`）。
 *  - 小参数保持内联（`.limit(50)`），大参数才展开。
 *
 * 示例：
 *   db.users.find({"age":{"$gt":18}}).sort({"name":1}).limit(50)
 * 格式化为：
 *   db.users.find({
 *     "age": { "$gt": 18 }
 *   }).sort({ "name": 1 }).limit(50)
 */

type Part =
  | { kind: 'text'; value: string }
  | { kind: 'args'; raw: string };

/** 单参数 / 多参数压缩成单行后，长度 ≤ 该阈值就保持内联，超过才展开多行 */
const INLINE_MAX = 80;

export function formatMongoCommand(input: string): string {
  const src = (input ?? '').trim();
  if (!src) return src;

  const parts = splitParts(src);
  if (!parts) return src;

  // 必须以 `db.<col>.<op>` 开头才尝试格式化，否则原样返回
  const head = parts[0];
  if (!head || head.kind !== 'text' || !/^\s*db\s*\./.test(head.value)) {
    return src;
  }

  return parts
    .map((p) => (p.kind === 'text' ? collapseWhitespace(p.value) : formatArgsBlock(p.raw)))
    .join('')
    .trim();
}

/** 把 `db.users.find` / `.sort` 这类纯结构片段里的多余空白塌缩掉 */
function collapseWhitespace(text: string): string {
  return text.replace(/\s+/g, '');
}

/** `(raw)` 形式的参数块：先尝试单行紧凑形式，超长才展开多行 */
function formatArgsBlock(raw: string): string {
  const trimmed = raw.trim();
  if (!trimmed) return '()';

  const args = parseArgs(trimmed);
  if (args == null) {
    // 解析失败 → 用原样内容，但保留最外层一对括号
    return `(${trimmed})`;
  }

  // 优先紧凑
  const compact =
    args.length === 1
      ? stringifyCompact(args[0])
      : args.map(stringifyCompact).join(', ');
  if (compact.length <= INLINE_MAX) return `(${compact})`;

  // 超长才展开
  const expanded =
    args.length === 1
      ? JSON.stringify(args[0], null, 2)
      : args.map((v) => JSON.stringify(v, null, 2)).join(',\n');
  const indented = expanded
    .split('\n')
    .map((line) => '  ' + line)
    .join('\n');
  return `(\n${indented}\n)`;
}

/** 把 args 原文当作 `[<args>]` 解析成数组；失败返回 null。 */
function parseArgs(raw: string): unknown[] | null {
  const arr = parseRelaxedJson(`[${raw}]`);
  return Array.isArray(arr) ? arr : null;
}

/**
 * 紧凑单行序列化（比 JSON.stringify(v) 多一些可读性空格）：
 *   `{ "a": 1, "b": 2 }`   `[1, 2, 3]`   `{}`   `[]`
 * 用于判断「能不能塞回一行」。
 */
function stringifyCompact(v: unknown): string {
  if (v === null) return 'null';
  switch (typeof v) {
    case 'number':
    case 'boolean':
      return String(v);
    case 'string':
      return JSON.stringify(v);
  }
  if (Array.isArray(v)) {
    if (v.length === 0) return '[]';
    return '[' + v.map(stringifyCompact).join(', ') + ']';
  }
  if (typeof v === 'object') {
    const entries = Object.entries(v as Record<string, unknown>);
    if (entries.length === 0) return '{}';
    return (
      '{ ' +
      entries.map(([k, val]) => `${JSON.stringify(k)}: ${stringifyCompact(val)}`).join(', ') +
      ' }'
    );
  }
  return JSON.stringify(v);
}

/**
 * 容错 JSON 解析：依次尝试严格 JSON、再做 mongosh 风格规范化（与 Rust `parser.rs` 同思路）：
 *   - 单引号字符串 → 双引号；
 *   - unquoted key (`{foo:1}`) → quoted；
 *   - 尾随逗号 `,}` / `,]` 去掉。
 * 任何 ObjectId(...) / ISODate(...) / NumberLong(...) 之类的 JS helper 都会让解析失败，返回 null。
 */
function parseRelaxedJson(s: string): unknown | null {
  try {
    return JSON.parse(s);
  } catch {
    /* try relaxed */
  }
  let t = s;
  // 单引号 → 双引号
  t = t.replace(/'([^'\\]*(?:\\.[^'\\]*)*)'/g, (_m, inner: string) => {
    const safe = inner
      .replace(/\\'/g, "'")
      .replace(/\\/g, '\\\\')
      .replace(/"/g, '\\"')
      .replace(/\n/g, '\\n')
      .replace(/\r/g, '\\r');
    return `"${safe}"`;
  });
  // unquoted key
  t = t.replace(/([\{,]\s*)([\$A-Za-z_][\w$]*)\s*:/g, '$1"$2":');
  // 尾随逗号
  t = t.replace(/,(\s*[}\]])/g, '$1');
  try {
    return JSON.parse(t);
  } catch {
    return null;
  }
}

/** 顶层按 `(...)` 切分成 text / args 段；同时正确处理字符串里的括号 */
function splitParts(src: string): Part[] | null {
  const out: Part[] = [];
  let i = 0;
  let textStart = 0;
  let inStr: '"' | "'" | null = null;
  let escape = false;

  while (i < src.length) {
    const c = src[i];
    if (escape) {
      escape = false;
      i++;
      continue;
    }
    if (inStr) {
      if (c === '\\') escape = true;
      else if (c === inStr) inStr = null;
      i++;
      continue;
    }
    if (c === '"' || c === "'") {
      inStr = c;
      i++;
      continue;
    }
    if (c === '(') {
      if (i > textStart) out.push({ kind: 'text', value: src.slice(textStart, i) });
      const close = findMatchingParen(src, i);
      if (close < 0) return null;
      out.push({ kind: 'args', raw: src.slice(i + 1, close) });
      i = close + 1;
      textStart = i;
      continue;
    }
    i++;
  }
  if (textStart < src.length) {
    out.push({ kind: 'text', value: src.slice(textStart) });
  }
  return out;
}

/** 从 `(` 位置开始找到匹配的 `)`；考虑字符串中括号不计入深度 */
function findMatchingParen(s: string, start: number): number {
  if (s[start] !== '(') return -1;
  let depth = 0;
  let inStr: '"' | "'" | null = null;
  let escape = false;
  for (let i = start; i < s.length; i++) {
    const c = s[i];
    if (escape) {
      escape = false;
      continue;
    }
    if (inStr) {
      if (c === '\\') escape = true;
      else if (c === inStr) inStr = null;
      continue;
    }
    if (c === '"' || c === "'") {
      inStr = c;
      continue;
    }
    if (c === '(') depth++;
    else if (c === ')') {
      depth--;
      if (depth === 0) return i;
    }
  }
  return -1;
}
