# MongoDB Runner

> 🌐 **中文** · [English](./README.en.md)

一个**纯本地**的 MongoDB 桌面客户端。前端用 Vue 3 + Element Plus，后端用 **Tauri (Rust)** 直接调用官方 mongodb 驱动 —— **不需要任何代理 server**，应用关掉就什么都不留。

## 布局

```
┌────────────────────────────────────────────────────────────┐
│  Header: 品牌名 / 当前连接 / 语言 / 主题切换                │
├──────────────┬─────────────────────────────┬───────────────┤
│              │ Query Editor (db.users.find)│               │
│              │     · Run / Stop            │               │
│              ├─────────────────────────────┤               │
│ Sidebar      │                             │ Chat (LLM)    │
│   - 连接管理 │   JSON 结果展示（树形/原文）│ 自然语言生成  │
│   - 数据库 / │                             │ Mongo 查询    │
│     集合树   │                             │ · 一键执行    │
│   - 历史记录 │                             │               │
└──────────────┴─────────────────────────────┴───────────────┘
```

- **左侧**：连接管理（增删改 + 测试）、数据库/集合树、执行历史（含收藏）
- **中间**：mongosh 风格命令编辑器 + JSON 结果（树形/原文/表格三视图）
- **右侧**：LLM 助手对话框，把「找出 7 天内活跃的 VIP 用户」翻译成命令并一键执行

## 技术栈

| 层 | 用到的东西 |
|---|---|
| UI | Vue 3 + Vite + TypeScript + Element Plus + vue-i18n |
| Shell | **Tauri 2** (Rust + 系统 WebView，无 Electron 体积) |
| MongoDB | 官方 [`mongodb`](https://crates.io/crates/mongodb) Rust 驱动 + [`bson`](https://crates.io/crates/bson)（EJSON） |
| LLM | OpenAI 兼容 `/v1/chat/completions`（reqwest），兜底走本地 `cursor-agent` CLI |

## 前置依赖

- **Node 18+** — 跑前端构建
- **Rust 1.77+** — Tauri 后端

```bash
# macOS / Linux 装 rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# 或者 macOS 用 Homebrew
brew install rust
```

## 开发启动

```bash
cd mongodb_runner
npm install
npm run tauri:dev        # 或 npm run dev
```

第一次启动会 `cargo build` 全部 Rust 依赖，需要几分钟。后续增量编译就很快。

`npm run dev` 等价于 `npm run tauri:dev`：Tauri 会自动起 Vite 前端（5174），再把 WebView 指过去。

## LLM 配置（可选）

不配置 LLM 时，**查询功能完全可用**，只是右侧 AI 面板会显示「未启用」。

要启用 LLM，把环境变量给到 Tauri 进程，或者写到下面任一文件中（按优先级）：

1. `<workspace>/.mongo-runner.env`（开发期最方便）
2. `~/.config/mongodb-runner/llm.env`（XDG 标准）
3. `~/.mongodb-runner.env`（兜底）

文件格式跟普通 dotenv 一样：

```ini
# OpenAI 兼容站（OpenAI / DeepSeek / OpenRouter / 通义 等等）
OPENAI_API_KEY=sk-xxxxxxxx
OPENAI_BASE_URL=https://api.openai.com
OPENAI_MODEL=gpt-4o-mini
LLM_TIMEOUT=60

# 也可以走本地 cursor-agent
# CURSOR_AGENT_BIN=/usr/local/bin/cursor-agent
# CURSOR_MODEL=
# CURSOR_TIMEOUT=120
```

## 使用步骤

### 1. 添加连接

左侧栏 → 「+ 新增」 → 填名称 + URI（如 `mongodb://localhost:27017` 或 `mongodb+srv://user:pass@cluster.example.net`）。
连接信息**只存在浏览器 localStorage**，不会写到磁盘。

### 2. 浏览数据库 / 集合

连接激活后左侧会出现树形结构，点击集合自动把光标定到编辑器并填入 `db.<collection>.find({})`。

### 3. 执行查询

编辑器支持 mongosh 风格命令（在 Rust 侧解析，**不调用任何 shell / mongosh**）：

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

参数支持 **MongoDB Extended JSON v2**（`$oid` / `$date` / `$numberLong` 等）以及 mongosh 风格 unquoted key。
`Cmd/Ctrl + Enter` 也能执行。

### 4. AI 助手

右侧聊天框里用中文 / 英文描述需求，例如：

> 查找过去 7 天注册且最近登录过的 20 个 VIP 用户，按登录时间倒序

- **Enter** → 自由对话（LLM 解释 + 给建议）
- **⌘/Ctrl + Enter** → 强约束生成（输出一行可执行命令）

LLM 自动带上当前数据库 / 集合 / 一份采样文档作为 schema 提示，生成的命令会贴合你的数据结构。
检测到命令时可以「填入编辑器」或「直接执行」。

### 5. 主题 & 语言

右上角 Header 提供：

- **🌐 语言切换器** — 内置 10 种语言（zh-CN / zh-TW / en-US / ja-JP / ko-KR / fr-FR / de-DE / es-ES / pt-BR / ru-RU），选择会持久化到 localStorage，Element Plus 内置组件文案同步切换。
- **🎨 主题切换器** — 浅色 / 深色 / 跟随系统三档；选「跟随系统」时还会在按钮上显示一个 `L` / `D` 小角标指示当前实际生效的颜色。

## 目录结构

```
mongodb_runner/
├── src/                          # Vue 前端
│   ├── App.vue                   # 三栏布局
│   ├── api/{mongo,llm}.ts        # 通过 @tauri-apps/api/core 的 invoke
│   ├── components/               # Sidebar / QueryEditor / ResultPanel / JsonTreeView / ChatPanel / LanguageSwitcher / ThemeSwitcher
│   ├── composables/              # useConnections / useHistory / useChat / useTheme
│   ├── i18n/                     # vue-i18n + 10 个语言包
│   └── styles/global.css
├── src-tauri/                    # Rust 后端
│   ├── Cargo.toml
│   ├── tauri.conf.json
│   ├── build.rs
│   ├── capabilities/default.json
│   ├── icons/
│   └── src/
│       ├── main.rs
│       ├── lib.rs                # 入口 + 注册 invoke 命令
│       ├── parser.rs             # mongosh 风格命令解析（regex + bson EJSON）
│       ├── mongo.rs              # mongodb 驱动调用
│       ├── llm.rs                # OpenAI / cursor-agent dispatch
│       └── env_loader.rs         # env / dotenv 加载
├── index.html
├── package.json
├── tsconfig.json
└── vite.config.ts
```

## 打包桌面应用

```bash
npm run tauri:build
```

产物：
- macOS: `src-tauri/target/release/bundle/dmg/*.dmg` & `*.app`
- Windows: `src-tauri/target/release/bundle/{msi,nsis}/`
- Linux: `src-tauri/target/release/bundle/{appimage,deb}/`

## 安全提示

- mongosh 命令在 Rust 侧用正则 + EJSON 解析，**不开 shell、不 eval JS**，无注入风险。
- 不在白名单内的操作（如 `drop` / `dropDatabase`）会被直接拒绝。
- 默认 LLM system prompt 只生成只读命令；写操作必须用户明确要求。
- 整个应用纯本地：MongoDB 连接、LLM 调用、配置文件，都不经过任何第三方代理。
