# Glossary

本项目词汇以 leanote 域模型为准。术语区分**服务器概念**与**本地缓存副本**；重写（Rust）不得引入第二套叫法。

## 账户与同步

- **账户 (Account)** — leanote 服务器上的一个用户身份（Email/Username + Token），或离线本地账户（无 Token，不同步；未登录时 API 层回退 userId/token `"user1"`）。服务器账户参与同步。Rust 版的本地存储是其缓存，不是权威源。
- **USN (Update Sequence Number)** — 服务器侧全局递增版本号；每账户记 `LastSyncUsn`，增量同步据此拉取 delta。
- **全量同步 (Full Sync)** — 从 USN −1 拉齐 notebooks→notes→tags 的过程。
- **增量同步 (Incr Sync)** — 对比服务器 USN 与本地 `LastSyncUsn`，先拉 delta 再推送本地脏数据（push）。
- **脏标记 (Dirty flags)** — `IsDirty`（内容已改待推）、`LocalIsNew`/`IsNew`（本地新建未上服务器）、`IsDeleted`（标记删除待推）。服务端确认后清除。
- **待同步 (Pending sync)** — 任一表存在 `IsDirty=1` 行的状态（`Store::has_dirty()`，三表 COUNT）。驱动底部 sync 按钮的第四态 icon（RefreshCw）与「有更新待同步」提示；重查点统一在 `invalidate_lists()`（所有本地变更后的失效点）。显示优先级 Err > Pending > Ok > None。
- **同步摘要 (Sync summary)** — 登录后首同步完成的界面摘要卡：笔记本/笔记/笔记内容/标签四行计数 + 可选失败警示行 + 「即刻开始」进入主界面。内容拉取失败不中断同步（计入 `SyncStats.content_failures`），失败笔记保 `Content=''`+`ServerNoteId`，下次同步经 `notes_needing_content` 自动重试。
- **冲突 (Conflict)** — 本地与服务器对同一条目都有未对齐修改的状态；产品语义是**呈现冲突并人工裁决**，绝不静默覆盖。
- **服务端 ID 映射 (Server ID mapping)** — 本地 `NotebookId/NoteId` 与 `ServerNotebookId/ServerNoteId` 的对应关系；本地新建条目在首次推送前没有服务端 ID。

## 笔记组织

- **笔记本 (Notebook)** — 服务器上的笔记容器，可嵌套（子笔记本），有 `Seq` 排序。见“运行形态”节：GUI 中笔记本与标签共同构成左栏导航分组。
- **笔记 (Note)** — 一条笔记，含 `Title`、`Content`（HTML 或 Markdown）、`IsMarkdown` 标志、`Usn`、所属 NotebookId、Tags。
- **标签 (Tag)** — 跨笔记本的笔记横向组织；一篇笔记可有多个标签。本地 `notes.Tags` 列为 **CSV 格式**（`rust,gpui`）；个别服务端会把标签数组序列化成 JSON 字符串（`["rust"]`）下发，同步层在写入前解析为 CSV，`user_version` 迁移也会把存量 JSON 行归一化。标签导航列表**纯从 notes.Tags 聚合**（去重+计数），不读 tags 表（其 Count 经同步后失真）。
- **收藏 (Starred)** — 笔记的收藏标志，字段名 `Star`，**纯本地字段，从不上服务器**（服务端无对应 API；置 Star 不置 IsDirty，也不更新 UpdatedTime——第九轮起与内容编辑解耦，收藏不改变笔记的新鲜度）。是笔记列表平级的入口（Starred 视图）。
- **回收站 (Trash)** — 已删除笔记的停留区；从 Trash 恢复或彻底删除（`deleteTrash`）。
- **IsMarkdown** — 笔记内容格式标志。`true` = Markdown 文本；`false` = TinyMCE 富文本 HTML。见 ADR-0001 关于富文本在 Rust 版中的降级。
- **发布为博客 (IsBlog)** — 笔记的博客公开标志：置位即发布到 leanote 博客（leanote.com/博客名），行内 Globe icon 指示；**同步字段但不碰 UpdatedTime**（置/清除置 IsDirty 走下次推送，时间戳不动——发布是元数据不是内容，不该把笔记顶到最近列表顶部；对齐 JS `Note.setNote2Blog`）。与收藏（Star，连 IsDirty 都不置）相对。

## 运行形态

- **本地 HTTP 服务 (Local HTTP API)** — 运行于 `127.0.0.1:8912` 的本地接口，三组端点：`/api/*`（走应用内实时数据，服务 CLI/MCP）、`/mcp`（MCP Streamable HTTP）、`/sse`+`/messages`（遗留 SSE）。是外部工具（CLI、MCP、agent 工作流）的主要入口，Rust 版必须逐端点、逐字段兼容。
- **CLI (`leanote`)** — 命令行工具；双模：应用运行时走本地 HTTP API，应用未运行时直接读写存储层（写入带脏标记，待应用同步）。
- **MCP 服务 (MCP server)** — 本地 HTTP 服务上暴露的 Model Context Protocol 端点，工具集覆盖笔记本/标签/笔记的增删改查与搜索。
- **AI札 (AI notebook)** — CLI/MCP 无指定笔记本时自动创建并写入的目标笔记本（“AI札”）。

## 存储与文件

- **存储层 (Store)** — 本地持久化。Rust 版实现为 SQLite（`~/Library/Application Support/leanote/leanote.db`；win `%APPDATA%\leanote`、linux `~/.config/leanote`，可用 `LEANOTE_DATA` 覆盖），与 Electron 遗留 NeDB（同 mac 目录的 `nedb55/` 子树）永不互读写；不迁移旧缓存，重登录后全量同步重建（ADR-0002、ADR-0008）。
- **附件 (Attachment)** — 笔记内图片/文件的服务器对象（`file/getImage`、`file/getAttach`）；本地 `attachs.db` 为下载缓存。当前语料 0 附件，Rust 版将其列为显式缺口而非隐式删除。

## 布局与界面（Rust 版）

- **三栏布局 (Three-pane layout)** — Wake 式信息架构：左栏导航（搜索入口 + 固定导航 + 可折叠分组）+ 中栏笔记列表 + 右栏编辑/阅读面。无顶部通栏；窗口用系统透明标题栏，仅左栏顶部让位红绿灯（44px），中/右栏内容直通窗口顶边（ADR-0003，UI 优化轮修正）。用户拖好的左/中栏宽度在窗口 resize 时保持不变，增量全部由右栏（阅读/编辑面）吸收（gpui-base resizable 定制，issue 35）。窗口控制：macOS 用系统红绿灯；Windows/Linux 走自绘客户端装饰——左栏顶行右端挂最小化/最大化/关闭三钮（Windows 11 风格线 icon，关闭钮 hover 红底），经 gpui `WindowControlArea` hitbox 接平台事件链；非 mac 顶行同排应用图标 + 应用名 + 搜索钮，macOS 顶行维持现状（红绿灯让位 + 搜索钮，无应用名，issue 20 Q7）。
- **最近 (Recent)** — 左栏固定导航首项：最近编辑的 `RECENT_LIMIT=20` 条笔记（UpdatedTime 倒序）；导航徽章计数与列表共用同一上限常量（`min(20, live)`），不是全库存量。第五轮起取代「全部笔记」入口；浏览全部走笔记本/标签分组（ADR-0003 第 6 条）。
- **视觉语言 (Visual language)** — 取自 Wake 的“现代 macOS”风格：材质分层（titlebar/sidebar/list/background/popover 四层色）、无窗格边框线、持久 UI 零阴影、SF Pro 系统字、8 级字号、4px 间距栅格、分级圆角。与现 Electron 版的多主题体系是替代关系（ADR-0003）。**UI 主题 (Theme family)** 修订了 ADR-0003 第 3 条：六套整体配色族（默认/豆沙绿/暖棕/Solarized/Nord/墨黑）同时重染四层材质与编辑器高亮，一个偏好值驱动，明暗仍随系统（ADR-0010）。**主题色 (accent)** 是视觉语言的参数：6 色板本地偏好，用于选中态与动作色（ADR-0005）。
- **⌘K 面板 (⌘K palette)** — 顶部悬浮式全局面板（宽 560px，2026-10-27 自 680 收窄），三段式：快捷动作（新建笔记/立即同步）+ 最近打开（本地记录 top6）+ 即输即搜（统一搜索核心），⌘K / ⌘⇧P 触发，空输入显示最近打开（Wake 同款交互；语义 = Electron 版 command palette 的复刻超集，旧词条「⌘K 搜索」已并入）。应用内搜索仅此入口。导出备份/Vim 切换已移至设置面板（第十轮）。快捷键绑定统一用 gpui `secondary-` 前缀（macOS ⌘ / 其他 Ctrl），一条绑定双平台且 Kbd 提示符号正确。
- **编辑器 (Editor)** — 右栏源码编辑面：kit `Editor` 组件 + tree-sitter 语法着色（全部 kit 语言 feature，含 markdown 内嵌代码块注入着色），高亮配色取自当前 UI 主题族；第一行 = 标题 + 工具按钮（预览/附件/历史，仅 icon），markdown 格式化走斜杠面板。选中 Markdown 笔记**默认即编辑态**，源码为默认模式、预览为开关；改动防抖自动保存，⌘S 保留；标题行兼作窗口拖拽区（ADR-0004，工具栏后由斜杠面板取代）。
- **斜杠面板 (Slash panel)** — markdown 编辑态的格式化命令面板：插入模式下行首/空格后键入 `/` 触发（vim normal/visual 不触发，留给未来 vim 搜索；路径/URL 中的 `/` 不触发），4×4 纯 icon 紧凑格（无文字标签，28px 格 + 16px icon，面板尺寸随 icon 自适应），←→↑↓ 网格移动焦点、回车或 Tab 执行（方向键与回车/Tab 在输入组件层是 `Move*`/`Enter`/`IndentInline` 动作而非按键事件，面板打开时经 `capture_action` 截下，关时不拦编辑器原行为）、ESC 取消且 `/`+过滤串回填编辑器（backspace 到头/过滤空关闭不回填）、鼠标可点（点击即执行，hover 同步选中）；选中/hover 格用主题强调色（`theme::accent()`，随用户强调色偏好联动），hover 格零延迟出 tooltip 功能提示，方向键/鼠标切换选中时面板底部标签条同步显示命令名；继续键入按命令名（中/英）模糊过滤，选中执行时删去 `/` 及过滤词。收录原 markdown 工具栏全部功能（14 格插入 + 图片 + 帮助 = 16 格），工具栏本身与设置中的「编辑器工具栏」开关随之删除——面板是唯一入口（取代 ADR-0004 第二行工具栏设计）。
- **Mermaid 图 (Mermaid diagram)** — fenced `mermaid` 代码块的图形渲染：预览管线下用 merman（纯 Rust，无 JS/webview）解析为 SVG，经 gpui 原生 SVG 图像支持内联显示；SVG 在渲染出口按实际 ink 包围盒裁剪 viewBox/width/height（gantt/pie/mindmap 等族自带恒定画布内空白，gantt 底部约 50px 轴带），裁剪用的光栅化加载系统字体——与展示侧 gpui 光栅化同字体条件，否则文字标签落在包围盒外被裁掉（gantt 底部日期轴为纯文字带，首当其冲），逐像素零 ink 损失；配色仅随系统明暗切换（浅色 default / 深色 dark 基调），不逐 UI 主题族定制。渲染覆盖全部 markdown 渲染面：阅读态、编辑内预览、⌘T 全窗口预览、历史版本预览、帮助弹窗，以及 HTML 导出（内联 `<svg>`）与 PDF 导出（typst 嵌入）。解析失败回退为代码块 + 一行错误提示，不阻断整篇预览。编辑源码态永远是文本（不渲染）。
- **应用菜单 (App menu)** — macOS 原生菜单栏的应用菜单（标准模板：About / Services / Hide / Hide Others / Show All / 退出 ⌘Q），由 `cx.set_menus()` 构建。⌘Q 由此获得系统菜单项与真实退出路径；此前退出入口只有 Dock 右键（tray 已删）。Win/Linux 无菜单栏概念，不建。
- **Vim 模式 (Vim mode)** — 编辑器的键位层：normal/insert/visual 状态机 + 常用子集（motion、operator、yank/paste、`:w`/`:q`），默认开启、可关闭、持久化。visual 分 charwise（`v`）与 linewise（`V`，整行选择、motion 按行扩展、`d`/`y` 行级操作，行级复制粘贴复用 `yy`/`p` 路径）。模式反馈 = 光标形状（normal=block、insert=竖线、visual=block+选区）**+ 编辑态底部状态栏的模式段**（`-- NORMAL/INSERT/VISUAL --`，vim 关闭时显示 `-- 普通 --`）。刻意不承诺完整 vi 兼容（无寄存器语法、无宏、无插件），不兼容行为是决策不是缺陷（ADR-0004/0005）。
- **专注模式 (Focus mode)** — 应用内无干扰模式：⇧⌘F 隐藏左/中栏，右栏占满整窗，再点或 ESC 退出。非 macOS 系统原生全屏。原名"全屏"，第六轮更名以区分全窗口预览。
- **全窗口预览 (Full-window preview)** — ⌘T 在应用内将当前笔记的**渲染预览**铺满整窗（不触发 OS fullscreen），右上角关闭 icon，ESC/再按 ⌘T 退出；编辑态/阅读态均可用。与专注模式（隐藏栏但保留编辑面）正交。
- **笔记图片 (Note image)** — 拖放/选择/斜杠面板插入图片 → 字节存本地 images 表（`LocalId`/`ServerId`/`Bytes`），笔记内容引用 `leanote://file/getImage?fileId=<24hex>`（保持 local id，不改写内容）；渲染前替换为 data URL。脏图片（无 ServerId）随笔记以 multipart 推送（`note/addNote`/`updateNote`），服务器 `Files` 回写 ServerId；同步拉取时 `pull_missing_images` 下载缺失字节（复刻 Electron `src/file.js` 语义，ADR-0006）。
- **历史版本 (History)** — 笔记的服务端历史快照（每账户最近 20 条）；在居中大弹窗中列出+预览+恢复（ADR-0005）。区别于本地撤销。
- **设置 (Settings)** — 左栏底部齿轮唤起的居中弹窗（宽 640px），两栏三分区：左栏分区导航（外观/编辑器/数据）+ 右栏设置项。**外观**：跟随系统/浅/深、语言（中/英，切换即时生效）、界面缩放（100/110/120% 三档，rem 级整体缩放）、界面字体（系统默认/霞鹜文楷，首次选用后台下载 ttf 装入 `~/Library/Fonts`，重启生效）、主题（下拉选 UI 主题族，即时生效）、主题色。**编辑器**：Vim 开关（默认开）、编辑器字号（工具栏开关已随斜杠面板移除，ADR-0011）。**数据**：账户信息、导出备份、登出。仅本地 UI 偏好，不涉及服务器概念。
- **编辑器状态栏 (Editor status bar)** — 编辑态（非预览）右栏标题行下方一行：左侧 Tag icon + 标签胶囊（✕ 删除 + 输入回车添加，改动置脏走同一保存管线）；右侧 Clock icon + 本地更新日期。仅 Markdown 编辑态显示；预览与阅读态无此栏。第九轮起 Vim 模式段与 `行:列` 已移除；第十轮起移除下边框（行自身可读，不再框住编辑区）。
- **多选 (Multi-select)** — 中栏笔记列表的批量选择：⇧ 点选范围（锚点→当前行，按列表顺序）、⌘/⌃ 点选增删；选中超 1 条时右栏变为计数遮罩，右键出批量菜单（移动/收藏/发布博客/删除——复用单条 store 调用循环）；批量导出/PDF 刻意不做。多选是视图内瞬态：切换导航即清空；普通点击即回到单选并恢复阅读/编辑。
