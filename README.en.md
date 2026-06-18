# MongoDB Runner

> 🌐 [中文](./README.md) · **English**

A **fully-local** MongoDB desktop client. The frontend is Vue 3 + Element Plus, the backend is **Tauri (Rust)** calling the official MongoDB driver directly — **no proxy server required**, nothing left behind when you close the app.

## Features

### Data browsing & querying
- Manage multiple connections in the left sidebar (CRUD + test), databases and collections sorted alphabetically
- mongosh-style command editor with EJSON, unquoted keys, chained calls (`.sort().limit().skip()`)
- Result panel adapts to result kind: document list / single doc / scalar / write result, with `elapsedMs`, `count`, `truncated` metadata
- Center pane "editor on top / result on bottom" split is draggable, height persisted
- Execution history grouped by connection + database, favoritable and one-click refillable into the editor

### LLM integration
- **Multi-profile management**: the left `🤖 LLM API` tab lets you keep OpenAI / DeepSeek / Anthropic-compatible / Qwen / OpenRouter / Groq / Together / local `cursor-agent` profiles side by side, switch the active one in a click, with a "Test config" button to validate immediately
- **Local config detection**: scans `.mongo-runner.env` / `~/.config/mongodb-runner/llm.env` / `~/.mongodb-runner.env` / environment variables / `cursor-agent` binary / login state, and offers one-click import as a profile
- **Context-aware**: every chat automatically attaches "connection name / current db / current collection / editor command text / last query result (auto-truncated) / sample docs"; three checkboxes control sample docs, editor command and last result individually
- **Reply language follows the UI**: natural-language replies use the current UI language (all 10 locales supported); code, JSON, and command syntax are kept verbatim. Switching language takes effect on the next send.
- **Auto-diagnose on failure**: after you hit "Run" on an LLM-suggested command in the chat panel, if it fails, the failed command + error + context are sent back to the LLM, which returns an explanation and a proposed fix. The fix shows up with the same "Use / Run" buttons so you decide whether to proceed.
- **New chat**: the `✨ New chat` button (with a confirmation dialog and a message-count badge) clears the current conversation context.

### Polish
- **🌐 10 UI locales**: zh-CN / zh-TW / en-US / ja-JP / ko-KR / fr-FR / de-DE / es-ES / pt-BR / ru-RU; Element Plus built-in components switch locale at the same time
- **🎨 Three theme modes**: light / dark / follow system; in _auto_ mode a small `L` / `D` badge shows which one is actually active
- **Flash hint**: the editor border briefly flashes when the LLM injects a command, making it obvious where it landed
- All state (connections, history, chats, locale, theme, editor height, LLM profiles) lives **only in browser localStorage** — nothing is written to disk

## Layout

```
┌────────────────────────────────────────────────────────────┐
│  Header: brand / current connection / language / theme     │
├──────────────┬─────────────────────────────┬───────────────┤
│              │ Query Editor (db.users.find)│  Chat (LLM)   │
│ Sidebar      │     · Run / Stop            │  · New chat ✨│
│ - conns      ├─────────────────────────────┤  NL → Mongo   │
│ - db/coll    │                             │  query, one   │
│ - history    │   JSON result (tree / raw)  │  click to run │
│ - 🤖 LLM API │                             │  · auto-fix   │
└──────────────┴─────────────────────────────┴───────────────┘
```

- **Left (4 tabs)**: connections, database / collection tree, history, LLM API config
- **Center (top / bottom split)**: mongosh editor on top, JSON result on bottom; height is draggable
- **Right**: LLM assistant — turn _"find VIP users active in the last 7 days"_ into a Mongo command and run it in one click

## Stack

| Layer | What we use |
|---|---|
| UI | Vue 3 + Vite + TypeScript + Element Plus + vue-i18n (custom `messageCompiler` to avoid JSON brace clashes) |
| Shell | **Tauri 2** (Rust + system WebView, no Electron bloat) |
| MongoDB | Official [`mongodb`](https://crates.io/crates/mongodb) Rust driver + [`bson`](https://crates.io/crates/bson) (Relaxed EJSON) |
| LLM HTTP | [`reqwest`](https://crates.io/crates/reqwest) → OpenAI-compatible `/v1/chat/completions` |
| LLM CLI | Local [`cursor-agent`](https://www.cursor.com/) subprocess fallback, located via [`which`](https://crates.io/crates/which) |
| Async | `tokio` + `tokio::process` |
| Config | Hand-rolled dotenv parser + XDG config dirs + browser localStorage |

## Prerequisites

- **Node 18+** — for the frontend build
- **Rust 1.77+** — for the Tauri backend

```bash
# macOS / Linux — install rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Or with Homebrew on macOS
brew install rust
```

## Run in dev

```bash
cd mongodb_runner
npm install
npm run tauri:dev        # or: npm run dev
```

First launch will `cargo build` every Rust dependency — takes a few minutes. Incremental builds after that are fast.

`npm run dev` is an alias for `npm run tauri:dev`: Tauri starts the Vite dev server (port 5174) and points the WebView at it.
If you don't have Rust installed yet, `npm run dev:web` still serves the UI in a regular browser, but Tauri IPC won't work there.

## LLM configuration (optional)

Without an LLM, **query features still work fully**; the right-hand panel just shows _"Disabled"_.

There are **two ways** to configure, and they can coexist. UI profiles take priority over env / dotenv.

### Option 1: the "LLM API" panel (recommended)

Sidebar → `🤖 LLM` tab:

- "+ New profile" can add as many providers as you like: OpenAI / DeepSeek / Anthropic-compatible / Qwen / OpenRouter / Groq / Together / local cursor-agent / ...
- Every profile has a "Test config" button to validate immediately
- The top "Active" dropdown switches which profile is in use
- The "System detected" section below lists scanned env vars / dotenv files / `cursor-agent` binary / login state with a one-click "Import as profile"

### Option 2: env vars / dotenv files

Zero-UI startup, same as `tools/git_commit.py`. Pass env vars to the Tauri process, or write into one of these files (in priority order):

1. `<workspace>/.mongo-runner.env` (most convenient during development)
2. `~/.config/mongodb-runner/llm.env` (XDG standard)
3. `~/.mongodb-runner.env` (fallback)

Plain dotenv format:

```ini
# OpenAI-compatible endpoints (OpenAI / DeepSeek / OpenRouter / Qwen / ...)
OPENAI_API_KEY=sk-xxxxxxxx
OPENAI_BASE_URL=https://api.openai.com
OPENAI_MODEL=gpt-4o-mini
LLM_TIMEOUT=60

# Or use the local cursor-agent CLI
# CURSOR_AGENT_BIN=/usr/local/bin/cursor-agent
# CURSOR_MODEL=
# CURSOR_TIMEOUT=120
```

## Usage

### 1. Add a connection

Sidebar → _"+ Add"_ → fill in a name and URI (e.g. `mongodb://localhost:27017` or `mongodb+srv://user:pass@cluster.example.net`).
Connection info is stored **only in browser localStorage** and never written to disk.

### 2. Browse databases / collections

Once the connection is active, the sidebar shows a tree (alphabetically sorted). Clicking a collection auto-populates the editor with `db.<collection>.find({})`.

### 3. Run a query

The editor accepts mongosh-style commands (parsed on the Rust side — **no shell or mongosh is invoked**):

```js
db.users.find({ "age": { "$gt": 18 } }).sort({ "name": 1 }).limit(20)
db.users.findOne({ "_id": { "$oid": "65f3..." } })
db.events.find({ "ts": { "$gte": { "$date": "2024-01-01T00:00:00Z" } } })
db.users.aggregate([{ "$group": { "_id": "$role", "n": { "$sum": 1 } } }])
db.users.countDocuments({ "active": true })
db.users.distinct("role")
db.users.insertOne({ "name": "Tom", "age": 18 })
db.users.updateOne({ "_id": "abc" }, { "$set": { "active": true } })
db.users.deleteMany({ "expired": true })
```

Arguments accept **MongoDB Extended JSON v2** (`$oid` / `$date` / `$numberLong`, etc.) and mongosh-style unquoted keys.
`Cmd/Ctrl + Enter` runs the command.

### 4. AI assistant

In the right-hand chat, describe what you want in any language, e.g.:

> Find the 20 VIP users who registered in the last 7 days and have logged in recently, sorted by last login desc.

- **Enter** → free-form chat (LLM explains and suggests)
- **⌘/Ctrl + Enter** → strict generation (single executable command on one line)

Context attached by default (each toggle is individually controllable):
- **Sample docs** — a few documents sampled from the current collection so the LLM can infer your schema
- **Editor command** — whatever you currently have in the editor (great for "rewrite this to ...")
- **Last result** — the most recent execution result, auto-truncated to ≤3KB, including error if it failed
- Plus connection name, current db, and collection are always sent

LLM replies **always use the current UI language** (code blocks excluded). When a reply contains a ```js``` code block, "Use" / "Run" buttons appear next to it.
After hitting "Run" on an LLM-suggested command, if it fails and "Auto-diagnose failures" is on, the failed command + error + context are sent back to the LLM, which returns an explanation and a fix proposal — the fix carries the same "Use / Run" buttons so you decide whether to proceed.

The **`✨ New chat`** button at the top right clears the current conversation context (with a confirmation dialog).

### 5. Theme & language

Header (top-right) exposes:

- **🌐 Language switcher** — 10 locales, Element Plus built-in components switch at the same time; selection is persisted in localStorage
- **🎨 Theme switcher** — Light / Dark / Follow system. In _auto_ mode, an `L` / `D` badge tells you which one is currently active.

## Directory layout

```
mongodb_runner/
├── src/                                # Vue frontend
│   ├── App.vue                         # three-pane layout + draggable split
│   ├── main.ts
│   ├── shims-vue.d.ts
│   ├── api/
│   │   ├── mongo.ts                    # invoke() into Rust MongoDB commands
│   │   └── llm.ts                      # invoke() into Rust LLM commands (incl. locale)
│   ├── components/
│   │   ├── SidebarMenu.vue             # 4 tabs: connections / db tree / history / LLM
│   │   ├── ConnectionDialog.vue        # connection CRUD
│   │   ├── QueryEditor.vue             # mongosh editor
│   │   ├── ResultPanel.vue
│   │   ├── JsonTreeView.vue
│   │   ├── ChatPanel.vue               # chat + new-chat + auto-diagnose + context toggles
│   │   ├── LLMConfig.vue               # LLM API panel (active / profiles / detect)
│   │   ├── LLMProfileDialog.vue        # profile CRUD + Test config
│   │   ├── LanguageSwitcher.vue
│   │   └── ThemeSwitcher.vue
│   ├── composables/
│   │   ├── useConnections.ts
│   │   ├── useHistory.ts
│   │   ├── useChat.ts                  # messages + extractCommand + clear
│   │   ├── useLLMProfiles.ts           # profile CRUD + active + provider templates
│   │   └── useTheme.ts
│   ├── i18n/
│   │   ├── index.ts                    # vue-i18n + custom messageCompiler + 10-locale registry
│   │   └── locales/                    # zh-CN / zh-TW / en-US / ja-JP / ko-KR / fr-FR / de-DE / es-ES / pt-BR / ru-RU + types.ts
│   └── styles/global.css               # theme tokens + shared button styles
├── src-tauri/                          # Rust backend
│   ├── Cargo.toml
│   ├── tauri.conf.json
│   ├── build.rs
│   ├── capabilities/default.json
│   ├── icons/
│   └── src/
│       ├── main.rs
│       ├── lib.rs                      # registers every #[tauri::command]
│       ├── parser.rs                   # mongosh-style command parser (regex + EJSON)
│       ├── mongo.rs                    # mongodb driver calls, result trimming, sampling
│       ├── llm.rs                      # OpenAI / cursor-agent dispatch, locale injection, llm_detect_local
│       └── env_loader.rs               # env / dotenv / XDG path loader
├── tools/
│   └── git_commit.py                   # reference LLM integration
├── index.html
├── package.json
├── tsconfig.json
├── vite.config.ts
├── README.md / README.en.md
```

## Build the desktop app

```bash
npm run tauri:build
```

Output:
- macOS: `src-tauri/target/release/bundle/dmg/*.dmg` & `*.app`
- Windows: `src-tauri/target/release/bundle/{msi,nsis}/`
- Linux: `src-tauri/target/release/bundle/{appimage,deb}/`

## Security notes

- mongosh commands are parsed on the Rust side with regex + EJSON — **no shell, no JS eval** — so there is no injection surface
- Operations outside the allow-list (`drop` / `dropDatabase`, etc.) are rejected outright
- The default LLM system prompt only generates read-only commands; write operations must be explicitly requested by the user
- Only the connection's _display name_ is sent to the LLM as context — **not the URI** — so database passwords and tokens never leak to the cloud LLM; the "last result" attachment is auto-truncated to ≤3KB and has its own toggle
- Everything stays local: the MongoDB connection, the LLM call, and the config files never go through a third-party proxy
