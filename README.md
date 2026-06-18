# MongoDB Runner

一个本地运行的 **MongoDB 可视化客户端**，参考了 [`curl_display`](../curl_display) 的代码组织方式，
并按 [`git_commit.py`](../../php/v-game-api/tools/git_commit.py) 的思路集成了一个 LLM 助手用来把自然语言转换成 MongoDB 查询。

## 布局

```
┌────────────────────────────────────────────────────────────┐
│  Header: 品牌名 / 当前连接 / 主题切换                       │
├──────────────┬─────────────────────────────┬───────────────┤
│              │ Query Editor (db.users.find)│               │
│              │     · Run / Stop / Clear    │               │
│              ├─────────────────────────────┤               │
│ Sidebar      │                             │ Chat (LLM)    │
│   - 连接管理 │   JSON 结果展示（树形/原文）│ 自然语言生成  │
│   - 数据库/  │                             │ Mongo 查询    │
│     集合     │                             │  · Run 一键执行│
│   - 历史记录 │                             │               │
└──────────────┴─────────────────────────────┴───────────────┘
```

- **左侧栏**：连接管理、数据库/集合浏览、执行历史
- **中间页**：查询编辑器 + JSON 样式结果（可切换树形 / 原文 / 表格）
- **右侧栏**：LLM 交互对话框，输入「找出 7 天内活跃的 VIP 用户」之类的自然语言即可生成查询并一键执行

## 技术栈

- **前端**：Vue 3 + Vite + TypeScript + Element Plus
- **后端**：Node.js + Express + 官方 `mongodb` 驱动
- **LLM 接入**：OpenAI 兼容的 Chat Completions（推荐），或本地 `cursor-agent` CLI 兜底
- **本地存储**：连接信息 / 历史记录 / 主题 都存在浏览器 localStorage

## 安装 & 启动

```bash
cd mongodb_runner
npm install

# 复制 LLM 配置（可选，不配也能用查询功能，只是没有 AI 助手）
cp .mongo-runner.env.example .mongo-runner.env
# 编辑 .mongo-runner.env 填入 OPENAI_API_KEY 等

# 同时启动 前端 (5174) + 后端 (8788)
npm run dev
```

打开 <http://localhost:5174> 即可。

## 使用

### 1. 添加连接

点击左侧栏「连接管理」→「新增」，填入：

- 名称（任意备注名）
- URI（如 `mongodb://localhost:27017` 或 `mongodb+srv://user:pass@cluster.example.net`）
- 默认数据库（可选，进入后能切）

连接信息保存在 **本地 localStorage**，**不会上传到任何远端**。

### 2. 浏览数据库 / 集合

连接成功后，左侧栏会出现数据库/集合树。点击集合会自动把光标定位到编辑器并填好 `db.<collection>.find({})` 模板。

### 3. 执行查询

中间编辑器支持 mongosh 风格命令（在后端被解析后用驱动执行，**不经过 shell**，无注入风险）：

```js
db.users.find({ "age": { "$gt": 18 } }).sort({ "name": 1 }).limit(20)
db.users.findOne({ "_id": "abc" })
db.users.aggregate([{ "$group": { "_id": "$role", "n": { "$sum": 1 } } }])
db.users.countDocuments({ "active": true })
db.users.distinct("role")
db.users.insertOne({ "name": "Tom", "age": 18 })
db.users.updateOne({ "_id": "abc" }, { "$set": { "active": true } })
db.users.deleteMany({ "expired": true })
```

参数支持 **MongoDB Extended JSON v2**，可以写：

```js
db.events.find({ "_id": { "$oid": "65f3..." } })
db.events.find({ "ts":  { "$gte": { "$date": "2024-01-01T00:00:00Z" } } })
```

`Cmd/Ctrl + Enter` 也能执行。

### 4. AI 助手

在右侧聊天框里用中文/英文描述需求，例如：

> 查找过去 7 天内最近一次登录的 20 个 VIP 用户，按登录时间倒序

LLM 会输出一个 mongosh 命令；点 **Run** 即可直接执行，结果会显示在中间面板。

LLM 同时知道当前连接的数据库 / 选中的集合 / （可选）一份采样文档作为 schema 提示，所以生成的查询能贴合你的数据结构。

## 安全提示

- 后端通过 **mongodb driver** 调用 MongoDB，**不经过 shell**，没有命令注入风险。
- 但是：后端会按用户输入对**任意 URI** 发起连接、并按编辑器命令做**写操作**（insert/update/delete）。**仅推荐本机 / 可信网络运行**，不要对公网暴露 8788 端口。
- 连接字符串里如果包含密码，请确认你的浏览器 localStorage 是可信的。

## 目录结构

```
mongodb_runner/
├── server/
│   └── index.js            # Express + mongodb driver + LLM proxy
├── src/
│   ├── App.vue             # 三栏布局
│   ├── main.ts
│   ├── api/                # 前端 → 后端调用
│   │   ├── mongo.ts
│   │   └── llm.ts
│   ├── components/
│   │   ├── SidebarMenu.vue
│   │   ├── ConnectionDialog.vue
│   │   ├── QueryEditor.vue
│   │   ├── ResultPanel.vue
│   │   ├── JsonTreeView.vue
│   │   ├── ChatPanel.vue
│   │   └── ThemeSwitcher.vue
│   ├── composables/
│   │   ├── useConnections.ts
│   │   ├── useHistory.ts
│   │   ├── useChat.ts
│   │   └── useTheme.ts
│   ├── styles/
│   │   └── global.css
│   └── utils/
│       └── mongoCommand.ts
├── index.html
├── package.json
├── tsconfig.json
├── vite.config.ts
└── .mongo-runner.env.example
```

## 致谢

- 代码组织借鉴自 [`curl_display`](../curl_display)。
- LLM 调用方式（OpenAI 兼容 + cursor-agent 兜底）借鉴自 [`git_commit.py`](../../php/v-game-api/tools/git_commit.py)。
