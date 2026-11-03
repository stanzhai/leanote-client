# 更新日志 / Changelog



## 3.1.10（2026-11-03）

- **修复：CI macOS 签名第二轮失败（code object is not signed at all / In subcomponent: leanote-cli）**：签名循环按 glob 顺序先签主程序 `Leanote`——现代 `codesign` 把「签主程序」视同签整个 `.app`，会校验 `Contents/MacOS/` 下全部同级可执行文件，遇到尚未签名的 `leanote-cli` 即报错。改为先签辅助可执行文件（跳过主程序），`.app` 整体签名时自会签上主程序。本地以 ad-hoc 身份逐字执行新代码块验证：helper → bundle → `--verify --deep --strict` 全通过。

## 3.1.9（2026-11-03）

- **修复：CI macOS 签名必败（SecKeychainItemImport: Unknown format in import）**：`package-macos.sh` 曾把证书写入 `mktemp` 生成的**无扩展名**临时文件再 `security import`；现代 macOS 的 `security import` 按文件扩展名判定格式，无 `.p12` 后缀时对完全合法的证书同样报 "Unknown format in import"——与 secret 内容损坏的报错一字不差，极具误导性。改为 `$(mktemp).p12`（本地实测：合法 p12 → `1 identity imported`；垃圾内容 → 仍报 Unknown format；openssl 默认参数生成的 p12 → 报 MAC verification failed）。`import` 失败时现按 security 的错误信息分类输出排查提示（secret 不是 .p12 的 base64 / 密码不符或 p12 需 legacy 参数 `-macalg sha1 -certpbe PBE-SHA1-3DES -keypbe PBE-SHA1-3DES`），README「发布签名」补充 secret 本地自查命令。

## 3.1.8（2026-11-02）

- **文本光标粗细与颜色（对齐 Zed）**：① 粗细——vendored 输入组件的光标宽度此前按系统分裂（macOS 1.5px / Windows 2px 逻辑像素，且绘制不做像素对齐），macOS 视觉过细、Windows 在 125%/150% 缩放下渲染发虚偏粗；现采用 Zed 的做法：全平台统一 2 逻辑像素 + 绘制时 `pixel_snap_bounds` 对齐到整数设备像素（细而清晰、跨平台一致）。② 颜色——光标由正文前景色改为**强调色**（`tokens.colors.accent`，即用户可选的 Wake 强调色，默认蓝），vim 块状光标同步变为强调色 45% 透明覆盖。
## 3.1.7（2026-11-01）

- **修复：Windows 下霞鹜文楷等宽不生效（Issue 39）**：旧安装器把 ttf 写进 `~/Library/Fonts` —— 一个 Windows 根本不扫描的 macOS 路径（Windows 的用户字体需落 `%LOCALAPPDATA%\Microsoft\Windows\Fonts` 且在 HKCU `...\CurrentVersion\Fonts` 注册表登记，两者都没做）。根除而非补丁：ttf 下载一次进应用数据目录，启动时经 `TextSystem::add_fonts` **进程内注册**（DirectWrite 内存字体引用 / CoreText CGDataProvider / cosmic-text 三平台同构），当次启动即生效、无需重启，也不再触碰系统字体目录/注册表。首次运行后台下载完成后 live 注册并 `refresh_windows()` 重绘；本地 PDF 导出的 Typst 字体商店同步注入该 ttf（排在系统字体前，用户字体优先）。删除「重启后生效」提示（i18n `FontRestartHint`），设置页仅在下载中/失败时提示。

- **窗口位置记忆完善（Issue 39）**：① 几何持久化不再只在窗口关闭钩子里 —— `observe_window_bounds`（移动/resize 均触发）防抖 500ms 落盘，⌘Q/崩溃/注销等不走 should-close 的退出路径不再丢最后一次位置；② 新增 `maximized` 字段，最大化关闭后下次启动恢复最大化（旧状态文件缺字段按普通窗口加载，serde default 兼容）；③ 显示器列表主屏排首：上次所在显示器已拔除时，`clamped()` 的回退居中落点固定为主屏幕（此前落 `displays.first()`，顺序无保证）。顺带清理 `start_window_drag` 里遗留的调试探针写入。

- **Vim 模式补齐 `I`/`A` 行首、行尾插入（Issue 40）**：NORMAL 模式下 Shift+i 在**行首第一个非空白字符**处进入插入（整行空白回落行首，与 `^` 同语义），Shift+a 在**行尾**进入插入；此前两键被当作未知键吞掉无动作。实现上将 `EnterInsert` 的三个布尔字段重构为 `InsertAt` 枚举（`i`/`a`/`o`/`O` 行为不变），`^` 的首个非空白计算抽为 `first_non_blank` 供 `I` 共用；visual 模式刻意不进（子集无 block-insert）。

## 3.1.6（2026-11-01）

- **修复：Windows 窗口控制按钮位置不对、点击失效（ADR-0011 修订）**：三层根因。① 自绘最小化/最大化/关闭按钮从左栏顶行右端（窗口左上角约 240px 处，违反 Windows 惯例）移到**根级 overlay，贴窗口最右上角**，登录/同步期（侧栏隐藏）也存在；每钮 `occlude()` 截断下方 Drag hitbox，点击恢复原生 `HTMINBUTTON`/`HTMAXBUTTON`/`HTCLOSE` 应答（旧布局按钮嵌在 Drag 区里被抢答成 HTCAPTION，点击只会拖窗口）。② 编辑器标题行整体曾是 Drag 区 → 标题输入与右侧工具钮（预览/帮助/更多）全部不可点；标题行改为纯 client 区，右端为 caption band 预留。③ **Windows 高 DPI 视口错位**（175% 实测）：创建窗口时 OS 的 DPI 上下文翻转（96↔168）使后端把物理像素尺寸记成逻辑尺寸——布局视口 1.75 倍于可见窗口，右 ~43% 界面画在窗外且鼠标永远点不到（工具钮「点击失效」的真实根因）。`main.rs win_dpi_resync` 启动时 Win32 实测 client 尺寸，`视口×scale ≠ 实际 client` 时以 ±2px `SetWindowPos` 强制 WM_SIZE 按当前 scale 重算（物理尺寸不变）；修复介入时退出几何按设备像素保存，跨启动窗口尺寸稳定。

- **修复：拖动 markdown 标题栏直接崩溃退出（0xC0000409）**：标题行拖窗口的按压交接必须用**异步 `PostMessageW`**。首版同步 `SendMessageW(WM_NCLBUTTONDOWN)` 把 DefWindowProc 的模态移动循环嵌进 gpui 正在分发鼠标事件的 wndproc——消息泵持借重入（WER dump 实锤 `window_procedure → handle_nc_mouse_down_msg → draw_window → window_procedure` 循环）→ 主线程栈溢出 → `__fastfail`。`PostMessageW` 把按压排队，移动循环改在消息循环层进入（与原生 HTCAPTION 按压同层）。同轮修复偶发启动栈溢出：`main()` 在 Windows 上把应用搬到 64MB 栈工作线程运行（MSVC 默认 1MB 主栈被编辑器渲染深度击穿，抓栈定位在 `render_editor` 标签条，无循环、纯预算耗尽；栈页按需提交零常驻开销）。

- **编辑器标题输入框支持拖动窗口**：按下后位移超过 4px 阈值即把按压交给系统拖拽（Windows 走上述异步交接，macOS/Linux 保持 `start_window_move`）；纯点击仍定位光标、按钮照常。侧栏顶行拖拽区同样加阈值，顺带消除「点按钮手抖 1px 拖走窗口」。

- **左栏顶行补应用身份（Windows/Linux）**：拖拽区最左端新增应用图标 + 「Leanote」名称（非交互，可随 Drag 区拖动，Windows 标题栏惯例）；macOS 维持红绿灯让位原状。

- **应用图标统一 + 去锯齿**：Windows exe 图标（`packaging/icon.ico`）与标题栏内嵌图标（`assets/app-icon.png`）改由 `gen-icons.sh` **同帧产出，128px 条目逐字节一致**；形态为不透明纯黑方块 + 白蚁形字标（用户裁决），边缘 LANCZOS 全灰阶抗锯齿（16px=68 级、32px=138 级灰度；此前阈值化 alpha 只有 0/255 二值边缘，小尺寸锯齿明显）。

- 回归测试 +3：`win_controls_overlay_top_right_in_all_phases`（登录期/编辑期断言 overlay 贴右上角、工具钮避让 caption band、身份行在位，macOS 反断言）、`title_input_press_drag_triggers_window_drag`（2px 摆动不拖、40px 恰好交接一次、输入组件不吞监听）、`startup_restores_notebook_selection_and_last_note`（恢复语义）。leanote-app 102/102，ADR-0011 修订留痕。

- **修复：Linux CI 测试崩溃（`title_input_press_drag_triggers_window_drag` panic "not implemented"）**：vendored `gpui-pre` 测试平台（`TestWindow`）的 `start_window_move` 是 `unimplemented!()`，而标题行拖拽测试在非 Windows 平台走 gpui 的 `start_window_move` 路径（macOS/Linux 无 WM_NCLBUTTONDOWN 交接）——macOS 本地同样复现，非 Linux 特有。改为无操作：headless 测试窗没有 OS 拖拽循环，计数由 app 侧 `BEGIN_DRAG_CALLS` 测试钩子完成，能证明「交接请求已发出」即可，真正的拖拽是 OS 手递手。

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
