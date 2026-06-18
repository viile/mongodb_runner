# MongoDB Runner

> 🌐 [中文](./README.md) · **English**

A **fully-local** MongoDB desktop client. The frontend is Vue 3 + Element Plus, the backend is **Tauri (Rust)** calling the official MongoDB driver directly — **no proxy server required**, nothing left behind when you close the app.

## Layout

```
┌────────────────────────────────────────────────────────────┐
│  Header: brand / current connection / language / theme     │
├──────────────┬─────────────────────────────┬───────────────┤
│              │ Query Editor (db.users.find)│               │
│              │     · Run / Stop            │               │
│              ├─────────────────────────────┤               │
│ Sidebar      │                             │ Chat (LLM)    │
│   - conns    │   JSON result (tree / raw)  │ NL → Mongo    │
│   - db /     │                             │ query, one    │
│     coll tree│                             │ click to run  │
│   - history  │                             │               │
└──────────────┴─────────────────────────────┴───────────────┘
```

- **Left**: connection manager (CRUD + test), database/collection tree, execution history (with favorites)
- **Center**: mongosh-style command editor + JSON result (tree / raw / table views)
- **Right**: LLM chat. Turn _"find VIP users active in the last 7 days"_ into a Mongo command and run it with one click.

## Stack

| Layer | What we use |
|---|---|
| UI | Vue 3 + Vite + TypeScript + Element Plus + vue-i18n |
| Shell | **Tauri 2** (Rust + system WebView, no Electron bloat) |
| MongoDB | Official [`mongodb`](https://crates.io/crates/mongodb) Rust driver + [`bson`](https://crates.io/crates/bson) (EJSON) |
| LLM | OpenAI-compatible `/v1/chat/completions` (reqwest), with local `cursor-agent` CLI as fallback |

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

`npm run dev` is just an alias for `npm run tauri:dev`: Tauri starts the Vite dev server (port 5174) automatically and points the WebView at it.

## LLM configuration (optional)

Without an LLM, **query features still work fully**; the right-hand panel just shows _"Disabled"_.

To enable the LLM, pass environment variables to the Tauri process, or write them into one of these files (in priority order):

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

Once the connection is active, the sidebar shows a tree. Clicking a collection auto-populates the editor with `db.<collection>.find({})`.

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

Arguments accept **MongoDB Extended JSON v2** (`$oid` / `$date` / `$numberLong` etc.) and mongosh-style unquoted keys.
`Cmd/Ctrl + Enter` runs the command.

### 4. AI assistant

In the right-hand chat, describe what you want in any language, e.g.:

> Find the 20 VIP users who registered in the last 7 days and have logged in recently, sorted by last login desc.

- **Enter** → free-form chat (LLM explains and suggests)
- **⌘/Ctrl + Enter** → strict generation (single executable command on one line)

The LLM automatically gets the current database / collection / a sampled document as schema hints, so generated commands fit your data. When a command is detected, click _"Use in editor"_ or _"Run now"_.

### 5. Theme & language

Header (top-right) exposes:

- **🌐 Language switcher** — 10 locales (zh-CN / zh-TW / en-US / ja-JP / ko-KR / fr-FR / de-DE / es-ES / pt-BR / ru-RU). Preference is persisted in localStorage; Element Plus built-in components switch locale at the same time.
- **🎨 Theme switcher** — Light / Dark / Follow system. In _auto_ mode, an `L` / `D` badge tells you which one is currently active.

## Directory layout

```
mongodb_runner/
├── src/                          # Vue frontend
│   ├── App.vue                   # three-pane layout
│   ├── api/{mongo,llm}.ts        # invoke() into the Tauri Rust side
│   ├── components/               # Sidebar / QueryEditor / ResultPanel / JsonTreeView / ChatPanel / LanguageSwitcher / ThemeSwitcher
│   ├── composables/              # useConnections / useHistory / useChat / useTheme
│   ├── i18n/                     # vue-i18n + 10 locale packs
│   └── styles/global.css
├── src-tauri/                    # Rust backend
│   ├── Cargo.toml
│   ├── tauri.conf.json
│   ├── build.rs
│   ├── capabilities/default.json
│   ├── icons/
│   └── src/
│       ├── main.rs
│       ├── lib.rs                # entry + invoke command registry
│       ├── parser.rs             # mongosh-style parser (regex + bson EJSON)
│       ├── mongo.rs              # mongodb driver calls
│       ├── llm.rs                # OpenAI / cursor-agent dispatch
│       └── env_loader.rs         # env / dotenv loading
├── index.html
├── package.json
├── tsconfig.json
└── vite.config.ts
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

- mongosh commands are parsed on the Rust side with regex + EJSON — **no shell, no JS eval** — so there is no injection surface.
- Operations not on the allow-list (`drop` / `dropDatabase` etc.) are rejected outright.
- The default LLM system prompt only generates read-only commands; write operations must be explicitly requested by the user.
- Everything is local: the MongoDB connection, the LLM call, and the config files never go through a third-party proxy.
