# 更新日志 / Changelog



## 3.1.5（2026-10-30）

- **发布产物签名与公证（CI secrets 门控）**：GitHub Releases 的 mac 包此前仅 ad-hoc 签名，浏览器下载后带隔离属性，Gatekeeper 报「已损坏，应移到废纸篓」（macOS 15 起右键打开绕过也已移除）；Windows 安装器未签名触发 SmartScreen「Windows 已保护你的电脑」。两个打包脚本现支持正式签名：macOS 在 `MACOS_CERT_P12_BASE64`/`MACOS_CERT_PASSWORD`/`APPLE_ID`/`APPLE_APP_SPECIFIC_PASSWORD`/`APPLE_TEAM_ID` 齐备时，Developer ID + hardened runtime 签名 → notarization → staple（.app 与 DMG 各自公证装订；Info.plist 版本号剥离 tag 前缀 `v` 满足公证纯数字要求；keychain 创建幂等，失败重跑不再卡死）；Windows 在 `WINDOWS_CERT_PFX_BASE64`/`WINDOWS_CERT_PASSWORD` 存在时用 signtool 对两个 exe（打 zip 前签名，便携包内同为签名件）与 NSIS setup 做 SHA-256 + RFC3161 时间戳签名。secrets 未配置时行为与从前完全一致；证书已配但 Apple 凭据缺失直接失败，不静默回退未签名发版。生效需购置 Apple Developer Program（$99/年）与 Windows 代码签名证书，详见 ADR-0012。
## 3.1.3（2026-10-29）

- **导出 Word（.docx）+ 导出二级菜单**：笔记右键菜单与编辑器「更多 (⋯)」菜单中，原先平铺的导出 HTML / Markdown / PDF 三项折叠为「导出为…」二级子菜单（Markdown / HTML / PDF / Word 四项），并新增 **导出 Word**：本地纯 Rust 转换（knit-md-docx：pulldown-cmark 解析 + docx-rs 生成），支持标题/粗斜体/链接/图片/列表/任务列表/代码块/引用/GFM 表格/脚注；富文本笔记走与 PDF 相同的 HTML→Markdown 桥。标题中的尖括号会被转义后再拼接，避免被 markdown 解析器当作内联 HTML 吞掉（`T & <title>` 之类标题此前导出只剩 `T &`）。

- **跨平台拖拽窗口（标题行）**：编辑器标题行（含标题输入框的整行）成为窗口拖拽区——Windows 走 OS 原生 HTCAPTION（`window_control_area(Drag)` 命中），Linux 走 `start_window_move`，macOS 维持 AppKit 标题栏拖拽。标题输入框本身不吃鼠标按下（文本选择不受影响），拖拽生效于行的空白与按钮间隙；与导航头 44px、阅读条 28px/编辑条 10px 三个既有拖拽区行为一致。

## 3.1.2（2026-10-29）

- **UI 批次六项**：① **Markdown `code` 强调淡化**——行内代码底色改为主题强调色 10% 透明度（保色调不换色，正文可读），覆盖全部 5 个预览面；② **跟随系统主题实时生效**——原先 `let _ = observe_window_appearance(...)` 被立即丢弃导致退订，现 `detach()` 常驻，系统切换明暗无需重启；③ **全屏弹窗不透明**——历史/语法帮助/附件 4 个对话框补 `Material::Popover` 背景（主窗口毛玻璃不受影响）；④ **编辑器更多 (⋯) 菜单**——标题工具栏新按钮整合笔记右键菜单（复用 `note_row_menu`：移动/星标/博客/导出等）+ 附件 + 历史版本；⑤ **预览图片右键菜单**——阅读/预览/全屏/历史/帮助面的图片支持 **复制图片 / 另存为…**（复制写系统剪贴板；另存为走原生保存面板，结果在工具栏下方状态条反馈）；⑥ **记住窗口位置/大小**——退出记忆窗口几何，重启还原；记录的显示器已拔除时自动在剩余显示器居中。

- **UI 批次三项（第二批）**：① **修复 vim `x` 行尾删除**——NORMAL 模式块状光标停在行尾之后时 `x` 原先无动作，现按 vim 语义向左删除末字符（空行仍为无操作）；② **「源码」按钮改为「编辑」**——标题工具栏该按钮本义即「以 Markdown 编辑此笔记」，文案与图标（Code → SquarePen）同步更正，tooltip 中英双语同改；③ **设置面板 macOS 系统设置风格**——左侧分区改图标+文字左对齐的胶囊行（选中底色用用户强调色而非黑色，行内保留真实内边距），左右两栏之间加 1px 竖向分隔线（与主窗口分栏一致），栏宽 110→148；**右侧选中项同改强调色（含闪白修复）**——外观/磨砂/字体/语言/字号五处分段控件的选中底色原为 kit 默认近黑（neutral-900/50），根因是 `tokens.button_primary` 是独立于 `colors.primary` 的字段、主题覆盖只写了后者；首修只写 token 侧，被 `reconcile` 在每次明暗切换时用 `colors.button_primary` 的 kit 分模式默认值（暗色下近白）盖回，表现为选中色在强调色与白色间跳动，现 colors+tokens 两侧同值写入（同背景透明化的既有手法），并加 reconcile 回归测试；**强调色圆圈选中态改对号**——6 个色块的选中标记由描边圆环改为色块内白色对号（macOS 系统设置同款），选哪个色块哪个显示对号。

- **全屏预览去玻璃磨砂**：全屏预览（⌘T）容器此前不画背景，窗口级毛玻璃（NSVisualEffectView）直接透出，长文阅读时文字浮在桌面模糊层上。现补 `Material::Background` 不透明表面（与阅读/编辑栏同源），仅主窗口左侧导航保留毛玻璃。

- **修复：Mermaid 图表右键菜单不出现**——右键菜单此前只挂在 Markdown 图片节点通路（node.rs 的 `![]()` 渲染）上，而 Mermaid 图由自定义块渲染器直接构造 `img()`，永远经过不了该通路。现渲染器持有同一处理闭包并直接挂到图表 `img` 的 `on_click`（内存 SVG 无真实 URL，用合成 `mermaid://diagram.svg`，另存为文件名稳定）。五个预览面共用该渲染器，一处修复全部生效。


- **导出/另存为记住上次目录，默认 Downloads**：HTML / Markdown / PDF / .leanote 备份导出与图片「另存为…」的保存面板此前总是从家目录起步。现首次默认打开 `~/Downloads`（目录不存在时回退家目录），导出成功后记住所选目录写入 `prefs.json`（`export_dir`），下次面板从该目录打开；记住的目录已被删除/卸载时自动回退 `~/Downloads`。取消导出不改记忆。


- **使用帮助迁移 + 纯文本化**：① 「语法帮助」从斜杠（`/`）面板的命令格迁出（15 项，4×4 格），改为编辑器标题栏工具栏按钮（预览右侧，CircleQuestionMark 图标，tooltip「使用帮助」），随时可达、不再占命令格；② 帮助内容改为纯文本显示（等宽字体 + 行距排版，不再经 markdown 渲染——速查表里的语法示例本身会被解析成真实标题/粗体/列表，显示混乱），并扩充为三节：Markdown 语法速查（标题/强调/列表/引用/代码/链接/图片/分隔线/表格/任务/脚注/转义，按通用定义）、`/` 快捷菜单用法（过滤/方向键/Enter/Tab/Esc/Backspace）、其他提示（保存、Mermaid）。中英双语。对话框加宽至 520、限高 420 内滚动。

## 3.1.1（2026-10-29）

- **修复：斜杠面板交互四连**：① 面板改纯 icon 紧凑格（去文字标签），格 36×36 再收到 32×32、gap 4→2、内边距 6→4；② 方向键接管——方向键在输入组件层是 `Move*` 动作而非按键事件，此前面板收不到、光标只在编辑器里动，现经 `capture_action` 截下做 ←→↑↓ 网格导航；③ **回车/Tab 执行**——Enter 与 Tab 同样是动作（`Enter`/`IndentInline`），不经过按键事件层，上一轮的按键分支永远收不到，现于 `capture_action` 顶部加面板分支，选中命令真正可执行；④ Esc 取消时 `/` 及已输入过滤串回填编辑器（此前触发键被吞，`/` 无法正常输入）。

- **斜杠面板交互补全（issue 37）**：① **格子点击执行**——每格接 `on_click`（回调经 shell 实体 `update` 回写：`execute` 后关面板），面板容器 `occlude()` 防点击穿透到编辑器；② **强调色选中/hover**——选中格与 hover 态改用 `theme::accent()`/`accent_foreground()`（与常规菜单 MenuItem 同源，可随用户换强调色），替换原中性灰高亮；③ **面板随 icon 自适应**——格子 32→28，icon 显式 `Size::XSmall`（16px，不再跟随编辑器环境字号约 20px），面板尺寸公式同步；④ **即时提示**——hover 格子零延迟出原生 tooltip（`tooltip_show_delay(ZERO)` + 组件 `Tooltip` 视图），方向键/鼠标切换选中时面板底部标签条同步显示命令名（原生 tooltip 只跟鼠标，键盘切换看不到）。

- **修复：Mermaid 图下方大面积空白（二次根因）**：上一轮裁剪已生效，但裁剪用的光栅化以 `usvg::Options::default()`（**空字体库**）计算 ink 包围盒——所有 `<text>` 被丢弃，包围盒漏掉全部文字标签；而实际渲染（gpui 光栅化）加载系统字体，文字画在包围盒之外被裁掉。gantt 底部日期轴是纯文字带，恰是首当其冲的图族。现 `ink_viewbox` 同样加载系统字体，两侧光栅化一致。另：merman 对 gantt/pie/mindmap 等图族在自身 viewBox 内部预留恒定空白（gantt 底部约 50px 轴带与任务数无关），渲染出口统一按实际 ink 包围盒裁剪 viewBox/width/height（含负 origin 平移）；全部预览面与 PDF/HTML 导出共用该出口。**进程内缓存**：同源码+明暗的 SVG 结果有缓存，升级后首次渲染即用新逻辑，无需操作。

## 3.1.0（2026-10-29）

- **斜杠命令面板**：编辑器内输入 `/` 呼出 4×4 命令面板（16 项：标题/粗斜/代码/链接/列表/表格/图片/帮助等），↑↓ 选择、Enter 执行、Esc 关闭；取代原 Markdown 工具栏及其设置项（ADR-0011）。
- **Mermaid 图表**：预览、阅读视图、历史弹窗、全屏预览、导出 PDF/HTML 全链路渲染 mermaid 代码块（纯 Rust merman 实现）；解析失败回退为代码块并附一行错误。
- **macOS ⌘Q 退出**：原生应用菜单（服务/退出），⌘Q 直接退出应用。
- **Windows/Linux 窗口控制按钮**：左侧导航头部自绘最小化/最大化/关闭按钮，区域交给平台层做原生点击处理。
- **GitHub Release 打包**：macOS DMG 带拖入 Applications 的安装背景与图标布局；Windows NSIS 安装器新增桌面快捷方式。
- **图标重绘**：统一从 `logo/icon-black-white.png` 生成 —— mac 中性黑 glyph，Windows 黑底白色 glyph 圆角矩形，多尺寸 ico/icns/hicolor。
