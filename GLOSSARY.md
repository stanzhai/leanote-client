# Glossary

本项目词汇以 leanote 域模型为准。术语区分**服务器概念**与**本地缓存副本**；重写（Rust）不得引入第二套叫法。

## 账户与同步

- **账户 (Account)** — leanote 服务器上的一个用户身份（Email/Username + Token），或离线本地账户（无 Token，不同步；未登录时 API 层回退 userId/token `"user1"`）。服务器账户参与同步。Rust 版的本地存储是其缓存，不是权威源。
- **USN (Update Sequence Number)** — 服务器侧全局递增版本号；每账户记 `LastSyncUsn`，增量同步据此拉取 delta。
- **全量同步 (Full Sync)** — 从 USN −1 拉齐 notebooks→notes→tags 的过程。
- **增量同步 (Incr Sync)** — 对比服务器 USN 与本地 `LastSyncUsn`，先拉 delta 再推送本地脏数据（push）。
- **脏标记 (Dirty flags)** — `IsDirty`（内容已改待推）、`LocalIsNew`/`IsNew`（本地新建未上服务器）、`IsDeleted`（标记删除待推）。服务端确认后清除。
- **冲突 (Conflict)** — 本地与服务器对同一条目都有未对齐修改的状态；产品语义是**呈现冲突并人工裁决**，绝不静默覆盖。
- **服务端 ID 映射 (Server ID mapping)** — 本地 `NotebookId/NoteId` 与 `ServerNotebookId/ServerNoteId` 的对应关系；本地新建条目在首次推送前没有服务端 ID。

## 笔记组织

- **笔记本 (Notebook)** — 服务器上的笔记容器，可嵌套（子笔记本），有 `Seq` 排序。见“运行形态”节：GUI 中笔记本与标签共同构成左栏导航分组。
- **笔记 (Note)** — 一条笔记，含 `Title`、`Content`（HTML 或 Markdown）、`IsMarkdown` 标志、`Usn`、所属 NotebookId、Tags。
- **标签 (Tag)** — 跨笔记本的笔记横向组织；一篇笔记可有多个标签。本地 `notes.Tags` 列为 **CSV 格式**（`rust,gpui`）；个别服务端会把标签数组序列化成 JSON 字符串（`["rust"]`）下发，同步层在写入前解析为 CSV，`user_version` 迁移也会把存量 JSON 行归一化。标签导航列表**纯从 notes.Tags 聚合**（去重+计数），不读 tags 表（其 Count 经同步后失真）。
- **收藏 (Starred)** — 笔记的收藏标志，字段名 `Star`，**纯本地字段，从不上服务器**（服务端无对应 API；现版置 Star 不置 IsDirty）。是笔记列表平级的入口（Starred 视图）。
- **回收站 (Trash)** — 已删除笔记的停留区；从 Trash 恢复或彻底删除（`deleteTrash`）。
- **IsMarkdown** — 笔记内容格式标志。`true` = Markdown 文本；`false` = TinyMCE 富文本 HTML。见 ADR-0001 关于富文本在 Rust 版中的降级。

## 运行形态

- **本地 HTTP 服务 (Local HTTP API)** — 运行于 `127.0.0.1:8912` 的本地接口，三组端点：`/api/*`（走应用内实时数据，服务 CLI/MCP）、`/mcp`（MCP Streamable HTTP）、`/sse`+`/messages`（遗留 SSE）。是外部工具（CLI、MCP、agent 工作流）的主要入口，Rust 版必须逐端点、逐字段兼容。
- **CLI (`leanote`)** — 命令行工具；双模：应用运行时走本地 HTTP API，应用未运行时直接读写存储层（写入带脏标记，待应用同步）。
- **MCP 服务 (MCP server)** — 本地 HTTP 服务上暴露的 Model Context Protocol 端点，工具集覆盖笔记本/标签/笔记的增删改查与搜索。
- **AI札 (AI notebook)** — CLI/MCP 无指定笔记本时自动创建并写入的目标笔记本（“AI札”）。

## 存储与文件

- **存储层 (Store)** — 本地持久化。现实现为 NeDB JSONL 文件（`~/Library/Application Support/leanote/nedb55/`）；Rust 版换用新本地存储，**不迁移旧缓存**，重登录后全量同步重建（ADR-0002）。
- **附件 (Attachment)** — 笔记内图片/文件的服务器对象（`file/getImage`、`file/getAttach`）；本地 `attachs.db` 为下载缓存。当前语料 0 附件，Rust 版将其列为显式缺口而非隐式删除。

## 布局与界面（Rust 版）

- **三栏布局 (Three-pane layout)** — Wake 式信息架构：左栏导航（搜索入口 + 固定导航 + 可折叠分组）+ 中栏笔记列表 + 右栏编辑/阅读面。无顶部通栏；窗口用系统透明标题栏，仅左栏顶部让位红绿灯（44px），中/右栏内容直通窗口顶边（ADR-0003，UI 优化轮修正）。
- **最近 (Recent)** — 左栏固定导航首项：最近编辑的 20 条笔记（UpdatedTime 倒序）。第五轮起取代「全部笔记」入口；浏览全部走笔记本/标签分组（ADR-0003 第 6 条）。
- **视觉语言 (Visual language)** — 取自 Wake 的“现代 macOS”风格：材质分层（titlebar/sidebar/list/background/popover 四层色）、无窗格边框线、持久 UI 零阴影、SF Pro 系统字、8 级字号、4px 间距栅格、分级圆角。与现 Electron 版的多主题体系是替代关系（ADR-0003）。**编辑器配色**是独立概念：仅作用于源码着色的 2–3 套配色（默认跟随系统明暗），不等于全局多主题。**主题色 (accent)** 是视觉语言的参数：6 色板本地偏好，用于选中态与动作色（ADR-0005）。
- **⌘K 面板 (⌘K palette)** — 顶部悬浮式全局面板（宽 680px），三段式：快捷动作（新建笔记/立即同步/导出备份…）+ 最近打开（本地记录 top6）+ 即输即搜（统一搜索核心），⌘K / ⌘⇧P 触发，空输入显示最近打开（Wake 同款交互；语义 = Electron 版 command palette 的复刻超集，旧词条「⌘K 搜索」已并入）。中栏另有**列表搜索框**：作用于当前列表的即输即搜，与 ⌘K 面板共用搜索核心。
- **编辑器 (Editor)** — 右栏源码编辑面：kit `Editor` 组件 + tree-sitter 语法着色 + 编辑器配色；第一行 = 标题 + 工具按钮（预览/附件/历史/全屏，仅 icon），第二行 = markdown 格式工具栏 + 帮助。选中 Markdown 笔记**默认即编辑态**，源码为默认模式、预览为开关；改动防抖自动保存（内容有变才置脏），⌘S 保留（ADR-0004）。
- **Vim 模式 (Vim mode)** — 编辑器的键位层：normal/insert/visual 状态机 + 常用子集（motion、operator、yank/paste、`:w`/`:q`），默认开启、可关闭、持久化。模式反馈仅靠光标形状（normal=block、insert=竖线、visual=block+选区），无状态行。刻意不承诺完整 vi 兼容（无寄存器语法、无宏、无插件），不兼容行为是决策不是缺陷（ADR-0004/0005）。
- **全屏 (Focus mode)** — 应用内无干扰模式：隐藏左/中栏，右栏占满整窗，再点或 ESC 退出。非 macOS 系统原生全屏。
- **历史版本 (History)** — 笔记的服务端历史快照（每账户最近 20 条）；在居中大弹窗中列出+预览+恢复（ADR-0005）。区别于本地撤销。
- **设置 (Settings)** — 左栏底部工具栏唤起的居中弹窗：外观（跟随系统/强制浅/深 + 编辑器配色 + 主题色）、语言（中/英）、Vim 开关、账户信息与登出。仅本地 UI 偏好，不涉及服务器概念。
