# MongoDB Runner

> 🌐 **中文** · [English](./README.en.md)

一个**纯本地**的 MongoDB 桌面客户端。前端 Vue 3 + Element Plus，后端 **Tauri (Rust)** 直接调用官方 mongodb 驱动 —— **不需要任何代理 server**，应用关掉就什么都不留。

## 特性

### 数据浏览与查询
- 左侧管理多个连接（增删改 + 测试），数据库 / 集合按字母排序
- mongosh 风格命令编辑器，支持 EJSON、unquoted key、链式调用（`.sort().limit().skip()`）
- 结果区按结果类型自适应展示：文档列表 / 单文档 / 标量 / 写入结果，附带 elapsedMs、count、truncated 等元信息
- 中间栏「上：编辑器 / 下：结果」可拖拽分栏，高度持久化
- 执行历史按连接 + 数据库聚类，可收藏 / 一键回填到编辑器

### LLM 集成
- **多 Profile 管理**：左侧「🤖 LLM API」Tab 可同时配 OpenAI / DeepSeek / Anthropic 兼容 / 通义千问 / OpenRouter / Groq / Together / 本地 `cursor-agent` 等等，一键切换 active profile，并自带「Test config」即时验证
- **本地配置检测**：自动扫描 `.mongo-runner.env` / `~/.config/mongodb-runner/llm.env` / `~/.mongodb-runner.env` / 环境变量 / `cursor-agent` 可执行文件 / 登录态，并展示可一键导入为 profile 的检测项
- **上下文感知**：每次对话自动附带「连接名 / 当前 db / 当前 collection / 编辑器命令文本 / 上一次执行结果（自动裁剪） / 采样文档」；三个开关分别控制 sample 文档、编辑器命令、上一次结果是否随消息发出
- **回复语言跟随**：LLM 自然语言回复跟随软件语言（10 种全部支持），代码 / JSON / 命令语法保持不变；切换语言后下一次发送立即生效
- **失败自动诊断**：在 chat 面板点「运行」执行 LLM 生成的命令后，如果失败会自动把失败命令 + 错误 + 上下文喂回 LLM，得到原因解释 + 修复方案；修复命令同样附带「使用 / 运行」按钮，由你决定是否继续执行
- **新对话**：右上角 `✨ 新对话` 按钮带二次确认，清空当前对话上下文并显示消息数 badge

### 体验
- **🌐 10 种 UI 语言**：zh-CN / zh-TW / en-US / ja-JP / ko-KR / fr-FR / de-DE / es-ES / pt-BR / ru-RU，Element Plus 内置组件同步切换
- **🎨 三主题模式**：浅色 / 深色 / 跟随系统；系统模式下按钮上有 `L` / `D` 小角标显示当前实际生效色
- **闪烁提示**：LLM 把命令写入编辑器时边框轻微闪烁，方便用户感知
- 所有配置（连接、历史、对话、locale、theme、editor 高度、LLM profiles）**只存浏览器 localStorage**，不落盘

## 布局

```
┌────────────────────────────────────────────────────────────┐
│  Header: 品牌名 / 当前连接 / 语言 / 主题切换                │
├──────────────┬─────────────────────────────┬───────────────┤
│              │ Query Editor (db.users.find)│  Chat (LLM)   │
│ Sidebar      │     · Run / Stop            │  · 新对话 ✨  │
│ - 连接管理   ├─────────────────────────────┤  自然语言生成 │
│ - DB/Coll 树 │                             │  Mongo 查询   │
│ - 历史记录   │   JSON 结果（tree / 原文）  │  · 一键执行   │
│ - 🤖 LLM API │                             │  · 失败自诊断 │
└──────────────┴─────────────────────────────┴───────────────┘
```

- **左侧 4 Tab**：连接管理、数据库/集合树、执行历史、LLM API 配置
- **中间上下分栏**：上为 mongosh 编辑器，下为 JSON 结果；可拖拽调整高度
- **右侧**：LLM 助手对话框 —— 把「找出 7 天内活跃的 VIP 用户」翻译成命令并一键执行

## 技术栈

| 层 | 用到的东西 |
|---|---|
| UI | Vue 3 + Vite + TypeScript + Element Plus + vue-i18n（自定义 messageCompiler，避开 JSON 字面量花括号冲突） |
| Shell | **Tauri 2**（Rust + 系统 WebView，无 Electron 体积） |
| MongoDB | 官方 [`mongodb`](https://crates.io/crates/mongodb) Rust 驱动 + [`bson`](https://crates.io/crates/bson)（Relaxed EJSON） |
| LLM HTTP | [`reqwest`](https://crates.io/crates/reqwest) → OpenAI 兼容 `/v1/chat/completions` |
| LLM CLI | 本地 [`cursor-agent`](https://www.cursor.com/) 子进程兜底，通过 [`which`](https://crates.io/crates/which) 探测可执行文件 |
| 异步 | `tokio` + `tokio::process` |
| 配置 | 自实现 dotenv 解析 + XDG 配置目录 + 浏览器 localStorage |

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
没装 Rust 也可以单独跑 `npm run dev:web` 在浏览器里看 UI，但所有 Tauri IPC 都不可用。

## LLM 配置（可选）

不配置 LLM 时**查询功能完全可用**，只是右侧 AI 面板会显示「未启用」。

有 **两种方式**，可以并存。UI Profile 优先级 > 环境变量 / dotenv。

### 方式一：「LLM API」面板（推荐）

左侧切到 `🤖 LLM` Tab：

- 「+ 新建 Profile」可以添加任意多个 provider：OpenAI 官方 / DeepSeek / Anthropic 兼容 / 通义千问 / OpenRouter / Groq / Together / 本地 cursor-agent 等
- 每个 profile 都有「Test config」按钮，配错了立刻知道
- 顶部「Active」下拉切换当前用哪个 profile
- 下方「系统检测」展示扫描出来的环境变量 / dotenv 文件 / cursor-agent 二进制 / 登录态，可一键「导入为 profile」省去抄写

### 方式二：环境变量 / dotenv 文件

零 UI 启动，跟 `tools/git_commit.py` 一致。把环境变量传给 Tauri 进程，或写入下列任一文件（按优先级）：

1. `<workspace>/.mongo-runner.env`（开发期最方便）
2. `~/.config/mongodb-runner/llm.env`（XDG 标准）
3. `~/.mongodb-runner.env`（兜底）

文件格式跟普通 dotenv 一样：

```ini
# OpenAI 兼容站（OpenAI / DeepSeek / OpenRouter / 通义 ...）
OPENAI_API_KEY=sk-xxxxxxxx
OPENAI_BASE_URL=https://api.openai.com
OPENAI_MODEL=gpt-4o-mini
LLM_TIMEOUT=60

# 或者本地 cursor-agent
# CURSOR_AGENT_BIN=/usr/local/bin/cursor-agent
# CURSOR_MODEL=
# CURSOR_TIMEOUT=120
```

## 使用步骤

### 1. 添加连接

左侧栏 → 「+ 新增」 → 填名称 + URI（如 `mongodb://localhost:27017` 或 `mongodb+srv://user:pass@cluster.example.net`）。
连接信息**只存在浏览器 localStorage**，不写磁盘。

### 2. 浏览数据库 / 集合

连接激活后左侧会出现树形结构（按字母排序），点击集合自动把光标定到编辑器并填入 `db.<collection>.find({})`。

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
`Cmd/Ctrl + Enter` 执行。

### 4. AI 助手

右侧聊天框里用任意语言描述需求，例如：

> 查找过去 7 天注册且最近登录过的 20 个 VIP 用户，按登录时间倒序

- **Enter** → 自由对话（LLM 解释 + 给建议）
- **⌘/Ctrl + Enter** → 强约束生成（输出一行可执行命令）

发送时默认附带的上下文（可单独关闭）：
- **采样文档** — 当前集合的若干样例文档，给 LLM 推断 schema
- **编辑器命令** — 上方编辑器里此刻的文本，用于「把它改成…」这类问法
- **最近一次结果** — 下方刚跑的结果（自动裁剪 ≤3KB），方便 LLM 看着真实数据回答
- 另外连接名、当前 db、collection 始终随同发送

LLM 回复**始终用当前 UI 语言**（代码块除外）。当回复包含 ```js``` 代码块时，会自动出现「使用」和「运行」按钮。
点「运行」执行 LLM 命令后，如果失败且「失败时自动诊断」开启，会自动把失败命令 + 错误 + 上下文再次喂给 LLM，由它给出原因和修复方案 —— 修复命令同样附带「使用 / 运行」按钮，由你决定是否继续执行。

右上角 **`✨ 新对话`** 按钮会清空当前聊天上下文（带二次确认）。

### 5. 主题 & 语言

右上角 Header 提供：

- **🌐 语言切换器** — 10 种语言全部支持，Element Plus 内置组件文案同步切换；当前 locale 持久化到 localStorage
- **🎨 主题切换器** — 浅色 / 深色 / 跟随系统三档；选「跟随系统」时按钮上显示 `L` / `D` 小角标表示当前实际生效色

## 目录结构

```
mongodb_runner/
├── src/                                # Vue 前端
│   ├── App.vue                         # 三栏布局 + 中间栏拖拽
│   ├── main.ts
│   ├── shims-vue.d.ts
│   ├── api/
│   │   ├── mongo.ts                    # invoke 调 Rust mongodb 命令
│   │   └── llm.ts                      # invoke 调 Rust LLM 命令（含 locale）
│   ├── components/
│   │   ├── SidebarMenu.vue             # 连接 / DB 树 / 历史 / LLM 四 Tab
│   │   ├── ConnectionDialog.vue        # 连接增删改
│   │   ├── QueryEditor.vue             # mongosh 编辑器
│   │   ├── ResultPanel.vue
│   │   ├── JsonTreeView.vue
│   │   ├── ChatPanel.vue               # 对话框 + 新对话 + 自动诊断 + 上下文开关
│   │   ├── LLMConfig.vue               # LLM API 配置面板（active / profiles / detect）
│   │   ├── LLMProfileDialog.vue        # Profile 增删改 + Test config
│   │   ├── LanguageSwitcher.vue
│   │   └── ThemeSwitcher.vue
│   ├── composables/
│   │   ├── useConnections.ts
│   │   ├── useHistory.ts
│   │   ├── useChat.ts                  # messages + extractCommand + clear
│   │   ├── useLLMProfiles.ts           # profile CRUD + active + provider 模板
│   │   └── useTheme.ts
│   ├── i18n/
│   │   ├── index.ts                    # vue-i18n + 自定义 messageCompiler + 10 locale 注册
│   │   └── locales/                    # zh-CN / zh-TW / en-US / ja-JP / ko-KR / fr-FR / de-DE / es-ES / pt-BR / ru-RU + types.ts
│   └── styles/global.css               # 主题 token + 公共按钮样式
├── src-tauri/                          # Rust 后端
│   ├── Cargo.toml
│   ├── tauri.conf.json
│   ├── build.rs
│   ├── capabilities/default.json
│   ├── icons/
│   └── src/
│       ├── main.rs
│       ├── lib.rs                      # 注册全部 #[tauri::command]
│       ├── parser.rs                   # mongosh 风格命令解析（regex + EJSON）
│       ├── mongo.rs                    # mongodb 驱动调用、结果裁剪、采样
│       ├── llm.rs                      # OpenAI / cursor-agent dispatch、locale 注入、llm_detect_local
│       └── env_loader.rs               # env / dotenv / XDG 路径加载
├── tools/
│   └── git_commit.py                   # LLM 集成参考实现
├── index.html
├── package.json
├── tsconfig.json
├── vite.config.ts
├── README.md / README.en.md
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

- mongosh 命令在 Rust 侧用正则 + EJSON 解析，**不开 shell、不 eval JS**，无注入风险
- 不在白名单内的操作（如 `drop` / `dropDatabase`）会被直接拒绝
- LLM 默认 system prompt 只生成只读命令，写操作必须用户在自然语言里明确要求
- 喂给 LLM 的上下文里只送连接「展示名」而**不送 URI**，避免把数据库密码 / token 发到云端 LLM；「最近一次结果」也会自动裁剪到 ≤3KB 并提供单独开关
- 整个应用纯本地：MongoDB 连接、LLM 调用、配置文件，都不经过任何第三方代理服务
