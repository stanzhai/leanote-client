# Spec: Rust 重写 leanote-client（原生 GPUI 三栏客户端）

Status: ready-for-agent
Created: 2026-10-19
Governing docs: GLOSSARY.md, docs/adr/0001～0003

## Problem Statement

现有 leanote 桌面客户端是 Electron 壳 + 移植的 leanote web 前端：jQuery/Bootstrap/zTree/TinyMCE/35k 行 StackEdit 式 markdown 编辑器，渲染进程 `nodeIntegration:true`，本地 HTTP 服务、CLI、MCP server 全部寄生在 Electron 渲染进程里。用户要的是：去掉整个 web UI 依赖栈，用 Rust + gpui-kit 做原生客户端；同时 CLI / 本地 HTTP API / MCP / 与 leanote 服务器的同步这些外部契约**一个字节都不能变**——外部工具（含 agent 工作流）与服务器都感知不到底下换了引擎。

## Solution

仓库内并行新增 Rust 实现（Electron 版冻结保留为参照与回退）。一次交付四个面：

1. **原生 GUI**：Wake 式三栏（左导航 / 中列表 / 右编辑阅读面，参考：https://github.com/iAmCorey/Wake），中英双语，跟随系统深浅色；Markdown 笔记原生编辑（源码 + 预览），富文本笔记只读渲染并可显式转存为 Markdown。
2. **同步引擎**：与 leanote 服务器线协议 100% 兼容，USN 全量/增量 + 脏推送 + 冲突呈现；本地存储换 SQLite，不迁移旧 NeDB 缓存，登录后全量同步重建。
3. **本地 HTTP 服务**：`127.0.0.1:8912` 上 `/api/*`、`/mcp`（stateful + stateless）、`/sse`+`/messages` 逐端点逐字段兼容；现有 `tests/` 脚本不改一行跑通即为主验收。
4. **CLI**：`leanote` 命令双模式（HTTP / 离线直写新存储），参数与输出兼容。

## User Stories

### 账户与首启

1. As a leanote 服务器账户持有者, I want 在主窗口内嵌表单里用 Email/密码登录（未登录时右栏即登录面）, so that 不需要额外的登录小窗。
2. As a leanote 服务器账户持有者, I want 登录成功后自动执行一次全量同步并看到进度, so that 换用 Rust 版后我的 23 个笔记本、127 个标签、1123 条笔记完整到位。
3. As a leanote 服务器账户持有者, I want 在设置里登出并切换到另一个服务器账户, so that 多账户数据互不污染。
4. As a 新用户, I want 登录面上的注册入口跳转 leanote.com 注册页, so that 没有账户时知道去哪创建。
5. As a 笔记用户, I want Rust 版不读取也不破坏 Electron 版的 NeDB 数据目录, so that 切换失败时旧版随时可用（切版前我会先把旧版脏数据同步干净）。

### 三栏浏览

6. As a 笔记用户, I want 左栏顶部有身份区（账户名 + 同步状态）, so that 一眼知道当前账户与同步健康度。
7. As a 笔记用户, I want 左栏固定导航 All Notes / Starred / Trash, so that 三类最高频入口一步可达。
8. As a 笔记用户, I want 笔记本以可折叠分组呈现且保留嵌套层级与 Seq 排序, so that 我按既有层级找笔记。
9. As a 笔记用户, I want 左栏 Tags 分组列出全部标签并可折叠, so that 横向入口与笔记本入口同级可见。
10. As a 笔记用户, I want 左栏底部工具栏（导航切换 + 设置）, so that 主功能区不占用内容空间。
11. As a 笔记用户, I want 中栏是两行笔记列表（标题 + 元信息：笔记本/标签/更新时间）, so that 高密度扫读。
12. As a 笔记用户, I want 选中任一导航节点（All/Starred/Trash/某笔记本/某标签）后中栏即时过滤, so that 列表永远反映当前上下文。
13. As a 笔记用户, I want 右栏阅读面 720px 居中且可切到编辑, so that 长文阅读不散焦。
14. As a 笔记用户, I want 窗口尺寸位置记住、macOS 上 44px 透明标题栏与左栏同材质, so that 每次打开都在熟悉的位置和样子。

### 笔记 CRUD 与编辑

15. As a 笔记用户, I want 新建 Markdown 笔记（当前笔记本上下文内）, so that 记录想法不用先选格式。
16. As a 笔记用户, I want Markdown 源码编辑 + 渲染预览（CommonMark + GFM 表格/删除线/围栏代码高亮）, so that 写作与确认排版一体完成。
17. As a 笔记用户, I want 每次保存自动置脏标记并在同步时推送服务器, so that 断网也能继续写、联网自动补推。
18. As a 笔记用户, I want 富文本笔记（IsMarkdown:false）以只读排版呈现（标题/段落/列表/表格/链接/图片）, so that 我的 20 条存量富文本笔记不丢、能看。
19. As a 笔记用户, I want 对富文本笔记显式执行"转存为 Markdown", so that 需要改写时把它变成可编辑且接受转换损失。
20. As a 笔记用户, I want 删除笔记先进回收站、回收站内可恢复或彻底删除、可清空, so that 误删有退路、真删有终局。
21. As a 笔记用户, I want 给笔记打星（Star，纯本地字段）并从 Starred 视图直达, so that 高频笔记一跳可达（星不上服务器，与现版一致）。
22. As a 笔记用户, I want 给笔记添加/移除标签并自动创建不存在的标签, so that 组织方式与现版一致。
23. As a 笔记用户, I want 移动笔记到其他笔记本, so that 整理不受创建时的位置束缚。
24. As a 笔记用户, I want 每次保存留一条本地历史（每笔记上限 20 条、去重）并从版本列表预览+恢复, so that 手滑覆盖可回滚。
25. As a 笔记用户, I want 笔记被彻底删除时其本地历史一并清除, so that 存储不残留孤儿数据。

### 笔记本与标签管理

26. As a 笔记用户, I want 新建/重命名/删除笔记本（删除时笔记入回收站）, so that 结构可演化。
27. As a 笔记用户, I want 拖拽调整笔记本顺序与嵌套父级（整组兄弟 Seq 重排）, so that 排序即所见且推服务器。（已交付形态：右键菜单「移动到…」替代拖拽——GPUI 无内置 DnD，语义等价且差异已接受）
28. As a 笔记用户, I want 重命名/删除标签并在全部笔记上生效, so that 标签体系可维护。

### 搜索

29. As a 笔记用户, I want ⌘K 打开全局搜索 overlay（输入框 + 结果列表 + 快捷键脚注）并即输即搜, so that 任何界面状态下一跳进搜索。
30. As a 笔记用户, I want 搜索语义统一为大小写不敏感子串匹配 Title+Content+Tags 且按权重排序（Title 3 / Tags 2 / Content 1）并带命中摘录, so that 与 CLI/MCP 搜索结果一致且对中文输入友好。
31. As a 笔记用户, I want 搜索结果选中后右栏打开该笔记, so that 搜索即到达。

### 同步与冲突

32. As a leanote 服务器账户持有者, I want 启动时与登录后自动增量同步（沿用现版触发时机，不新增定时器）, so that 多端使用不靠手动刷新。
33. As a leanote 服务器账户持有者, I want 手动"立即同步"入口, so that 关键修改立刻上服务器。
34. As a leanote 服务器账户持有者, I want 本地脏数据（新建/修改/删除）在同步时逐一推送并获服务端确认（含本地↔服务端 ID 建立映射）, so that 离线工作最终一致。
35. As a leanote 服务器账户持有者, I want 服务器在同步中途推进 USN 时自动再来一轮直至收敛, so that 并发修改不丢。
36. As a 笔记用户, I want 冲突发生时右栏顶部出现冲突条（本地 vs 服务器两侧摘要）并可选用本地/用服务器/两边都保留为新笔记, so that 冲突永远人工裁决、绝不静默覆盖。
37. As a 笔记用户, I want 服务器端删除与我本地修改冲突时按现版语义落地（置非脏不删）, so that 与 Electron 版行为一致。
38. As a 笔记用户, I want 断网时一切本地功能可用、恢复联网后自动补推, so that 网络状态不改变工作流。

### 导出（导入已裁撤）

39. As a 笔记用户, I want 单笔记导出为 HTML 与 Markdown 文件, so that 内容可脱离 leanote 使用。
40. As a 笔记用户, I want 单笔记导出 PDF（复用服务器 exportPdf 端点，端点不可用则降级导出 HTML 并提示）, so that 存档分享有纸质形态。
41. As a 笔记用户, I want 全库导出 .leanote JSON 备份（含笔记与内嵌文件 base64）, so that 有厂商无关的完整备份。

### CLI

42. As a 命令行用户, I want `leanote add -t 标题 -c 内容 -g 标签 -n 笔记本`（含 stdin 管道）行为与现版一致, so that 既有脚本零修改。
43. As a 命令行用户, I want `leanote list -n -g -v -N`、`show <id前缀>`、`search <关键词>`、`edit <id前缀>` 参数与输出格式与现版一致, so that 管道与别名继续工作。
44. As a 命令行用户, I want 应用在运行时 CLI 走本地 HTTP、未运行时直接写存储并置脏标记（应用启动后自动推送）, so that 任何时刻都能记笔记。
45. As a 命令行用户, I want ID 前缀匹配语义与现版一致（唯一前缀定位、歧义报错）, so that 短 ID 引用习惯保留。（JS 实测：精确 id 优先；前缀命中按 rowid 顺序取第一条 live 笔记，非歧义报错——Rust 按实测实现）
46. As a 命令行用户, I want 无指定笔记本时自动写入「AI札」（不存在则创建）, so that agent 产出有固定落脚点。
47. As a 命令行用户, I want 安装后 `~/bin/leanote` 指向 Rust 二进制（首启自动替换 PATH 垫片）, so that 外部工作流无感切换。

### MCP 与本地 HTTP

48. As an AI agent, I want MCP 端点 `http://127.0.0.1:8912/mcp` 支持 Streamable HTTP（有状态会话，上限 50、30 分钟 TTL）与 stateless 直发两种模式, so that 现有 MCP 客户端配置零修改。
49. As an AI agent, I want 8 个 MCP 工具（list_notebooks / list_tags / add_note / list_notes / get_note / search_notes / update_note / delete_note）名称与 schema 与现版一致, so that 已写的工具调用不破。
50. As an AI agent, I want search_notes 返回与 CLI 同语义的加权结果与摘录, so that 检索体验一致。
51. As an AI agent, I want delete_note 把笔记送进回收站而非硬删, so that agent 失误可挽回。
52. As an 外部工具开发者, I want `/api/ping`、`/api/note/addNote`、`/api/note/updateNote` 路由、PascalCase JSON 字段、CORS `*`、5MB 请求上限、仅绑定 127.0.0.1 全部与现版一致, so that 脚本级兼容。
53. As an 外部工具开发者, I want 遗留 `/sse`+`/messages` 端点继续工作, so that 老浏览器扩展不断。
54. As an AI agent 调试者, I want `LEANOTE_MCP_DEBUG=1` 时写调试日志文件, so that 排障手段保留。

### 平台与外观

55. As a macOS 用户, I want 全功能验收（编辑、同步、冲突、CLI、MCP、托盘、单实例）, so that 主力平台无降级。
56. As a Linux/Windows 用户, I want 编译通过 + 启动 + 同步 + 编辑 + CLI/MCP 可用（字体/材质细节差异不阻塞）, so that 三平台承诺可验证。
57. As a 笔记用户, I want 明暗双材质跟随系统（四层材质色、无窗格边框、持久 UI 零阴影、8 级字号、4px 间距栅格、分级圆角、强调色仅用于选中与动作）, so that 得到 Wake 级视觉秩序。
58. As a 中文用户, I want 界面中/英双语跟随系统切换, so that 语言不是升级障碍。
59. As a 笔记用户, I want 单实例锁（二次启动聚焦既有窗口）与最小托盘（显示/退出两项）, so that 与现版习惯一致。

### 维护者

60. As a 维护者, I want Rust workspace 与 Electron 版并行共存（JS 冻结只修致命 bug）, so that 重写期间随时可回退。
61. As a 维护者, I want 现有 tests/ 脚本（MCP/API/性能冒烟）不改一行对 Rust 版跑通, so that "功能不变"有机器判定。
62. As a 维护者, I want 兼容性用对拍验证（同一脚本分别打两版比对响应）, so that 字段级漂移当轮发现。
63. As a 维护者, I want 存储、同步、HTTP、CLI 各自是独立 crate 而共享一个 core, so that CLI 离线直写与 App 用同一套存储和脏标记逻辑。
64. As a 维护者, I want 崩溃时写本地日志（不迁移远端崩溃上报）, so that 排障有迹可循而无外部依赖。

## Implementation Decisions

- **范围**：Q1=A 全 Rust 原生，无 webview、无 TinyMCE、无 StackEdit 移植；这是重构主目标。多轨壳方案（GPUI+webview）已被用户否决。
- **兼容等级**：Q2=A。本地 HTTP 端点路径、JSON 字段（含 PascalCase 命名）、CLI 子命令/旗标/输出、MCP 工具名与 schema 全部严格兼容；`tests/` 现有脚本即验收测试。
- **服务器协议**：Q3=A+B。线协议逐字段兼容（同一账号可与 Electron 版互换使用）；本地存储全新（SQLite，经 ADR-0002），不读不写 NeDB；首启全量同步重建。切换前置条件：用户先把 Electron 版脏数据同步完。
- **信息架构与视觉**：Q4=B（ADR-0003）。Wake 骨架：左栏 224px（身份区+搜索入口+All/Starred/Trash+Notebooks/Tags 折叠分组+底部工具栏）、中栏 336px 两行列表、右栏剩余宽 720px 居中阅读列；无顶部通栏；裸 flex 布局不引 dock 框架；组件库 gpui-kit（longbridge gpui-component），锁定 Wake 同源版本。11 主题体系由明暗双材质取代；i18n 只留中英（Q12）。强调色照 Wake 蓝（Q16）。
- **富文本**：Q9=A（ADR-0001）。IsMarkdown:false 只读渲染（HTML→GPUI 文本树），显式"转存为 Markdown"后可编辑；CLI/MCP 写入一律 Markdown。
- **编辑器**：Markdown 源码编辑 + 预览（CommonMark + GFM 表格/删除线/代码高亮）；math/toc 等 StackEdit 扩展不承诺。可编辑右栏是 Wake 没有的结构性新增，实现期设计编辑/阅读切换。
- **搜索语义统一**：废弃 GUI 的 regex 语义，统一到 CLI/MCP 语义（大小写不敏感子串、Title+Content+Tags、权重 3/2/1、摘录窗口）。
- **附件**：Q10=B 显式缺口。笔记内既有远程图片按服务器 URL 显示；上传新图片/附件、md5 去重缓存、leanote:// 协议不实现（语料 0 附件）。
- **Star**：字段 `Star` 纯本地、不上服务器（服务端无对应 API），与现版一致。
- **本地账户**：创建入口在现版 UI 已禁用，不复活；`user1` 仅作未登录回退值保留在 API 层语义中。
- **历史**：纯本地，每笔记上限 20 条、连续保存去重、恢复=把旧版存为最新版；笔记彻底删除时连带清除。
- **导出**：单笔记 HTML/Markdown 本地生成；PDF 走服务器既有 exportPdf 端点（现版 JS 中存在但无调用方），失败降级 HTML；全库 .leanote JSON 备份。8 个导入/导出插件中 import_leanote/import_evernote/import_html/export_evernote 裁撤（Q13=B）。
- **CLI 双模**：Q14=A。探测本地 HTTP（短超时）失败则直写 SQLite（置脏标记），App 启动同步时推送；首启自动把 `~/bin/leanote` 垫片指向 Rust 二进制（Q6=B 切换语义）。
- **同步时机**：沿用现版（启动、登录后、手动触发）；不新增定时器。USN 回环保护（服务器中途推进则再来一轮）与本地↔服务端 ID 映射照现版语义重写。（已实现：已登录启动后台 incr_sync、登录后 full_sync、侧栏「立即同步」；失败不阻塞 UI，仅日志+身份区提示）
- **冲突**：右栏顶部冲突条（用本地/用服务器/两边保留为新笔记），不弹独立窗口（Q20）。
- **进程形态**：App 单进程内起本地 HTTP 服务（与现版"服务寄生在应用里"一致）；单实例锁 + 最小托盘（显示/退出）+ 窗口状态持久；无全局快捷键（现版本就无注册）；登录主窗内嵌（Q18）。
- **Workspace**：仓库内 `rust/` 并行目录（Q5=A），crate 划分 core（存储+同步+服务器客户端）/ http（本地服务+MCP）/ cli / app（GPUI）。Electron 版冻结保留。
- **平台**：Q7=B 三平台，验收分层（mac 全项；Linux/Win 编译+启动+同步+编辑+CLI/MCP，视觉细节不阻塞）。（现状：`mac_addr()` 的 AF_PACKET 分支覆盖 Linux；win32 无取 MAC 实现，回退 `00:00:00:00:00:00`——服务器仅记录该字段，不阻塞）

## Testing Decisions

- **好测试的标准**：只断言外部可观察行为——HTTP 响应的字段与状态码、CLI 的 stdout 与退出码、假服务器收到的请求序列。不测内部结构、不测私有函数的行为影子。
- **接缝 1（主验收，既有接缝）**：`tests/` 现有脚本（MCP server/SSE/stateless、API 性能冒烟）不改一行，直接打 Rust 版的 127.0.0.1:8912。Prior art：这些脚本本身；它们以独立 node 进程对 HTTP 边界断言。
- **接缝 2（新增）**：进程内假 leanote 服务器，按真实线协议（录制语义重放）：全量同步、增量拉取、脏推送、ID 映射、USN 中途推进回环、删除冲突置非脏、离线直写后的补推。
- **接缝 3（新增）**：CLI 子进程级测试：双模式（起/不起 App）下的 add/list/show/search/edit、stdin、ID 前缀歧义。
- **对拍**：兼容性断言通过对拍实现——同一测试脚本分别指向 Electron 版与 Rust 版，比对响应形状。字段落名漂移当轮暴露。
- **纯逻辑单测**（不新增接缝）：HTML→Markdown 转换器、搜索打分与摘录、USN/脏标记状态机。
- **GUI**：手工验收清单（mac 全项、Linux/Win 分层），不做 UI 自动化。

## Out of Scope

- 富文本（WYSIWYG）编辑与任何 webview 内嵌——被 ADR-0001 排除，升级路径是独立立项。
- 旧 NeDB 缓存读取/迁移（ADR-0002）。
- 11 个 leanote 主题、de/zh-hk/ja 界面语言。
- 导入插件（.leanote/.enex/HTML 导入）与 Evernote 导出。
- 附件/图片上传、本地附件缓存、md5 去重、`leanote://` 协议（显式缺口，Q10=B；显示既有服务器 URL 图片在范围内）。
- leanote web 侧的博客发布/分享/知识库等服务器功能入口（桌面版菜单曾有入口，本轮不实现——如需保留请明确提出）。
- StackEdit 扩展语法（math/toc）、TinyMCE 内 Ace 代码块编辑。
- 全局快捷键（现版无注册）、远端崩溃上报（改本地日志）、自动更新。
- Electron 版的功能性改动（冻结，仅致命 bug）。

## Further Notes

- 语料事实（2026-10-19 扫描）：单账户 stanzhai；1123 条笔记中 1103 条 Markdown、20 条富文本（存档性质标题）；0 附件；23 笔记本、127 标签。这些数字是降级决策（富文本只读、附件缺口）的证据基础。
- 三个事实侦察报告（repo 全景 / Wake 布局与组件 / GUI 边缘功能）是本 spec 的事实底座；ADR-0001～0003 与 GLOSSARY.md 是术语与决策权威。
- 最大风险序：① 富文本 HTML→GPUI 渲染保真（20 条存量）；② 可编辑右栏是 Wake 模式之外的新结构；③ gpui-kit 在 Linux/Windows 的成熟度；④ MCP/HTTP 字节级兼容的对拍工作量。
- 实现票据按 issue-tracker 约定另开（`.scratch/rust-rewrite/issues/`），本 spec 是唯一范围权威。
