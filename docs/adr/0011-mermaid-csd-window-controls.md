# ADR-0011: 无头 Mermaid 渲染与客户端自绘窗口控制

- 状态：Accepted（2026-10-29，第十一轮裁决）
- 关联：修订 ADR-0004 第二行工具栏（由斜杠面板取代）；GLOSSARY「斜杠面板」「Mermaid 图」「应用菜单」「三栏布局」

## 背景

第十一轮交付五项用户可感知改动：斜杠命令面板、mermaid 预览渲染、macOS ⌘Q 退出、Windows/Linux 自绘窗口按钮、打包与图标管线。其中两项难以逆转或脱离代码无法解释：

1. **Mermaid 渲染没有 JS 运行时可依赖。** Rust 客户端是纯 gpui——无 webview、无 Node。Electron 时代「引入 mermaid.js」的路径全部失效；渲染引擎的选择约束整个依赖树与预览永远支持的 mermaid 语法子集。
2. **Windows/Linux 窗口控制从「系统绘制」变为「应用自绘」。** `appears_transparent: true` 使窗口走 gpui 客户端装饰路径后，两个本地无法测试的平台上的标题栏布局由应用负责。

## 决策

### Mermaid：merman（纯 Rust），block-parser 扩展，不用 ELK

fenced `mermaid` 块用 [merman][merman]（MIT OR Apache-2.0，Zed 同款引擎）渲染：解析为 SVG，交给 gpui 原生 `Image::from_bytes(ImageFormat::Svg)`（usvg 渲染管线本就是 gpui 依赖），经 `TextView::markdown_block_parser` + `markdown_block_renderer` 扩展点接入，不改 gpui-kit。

1. **`default-features = false`**：保留 `all-diagrams`、`svg`、`layout-cytoscape`，**去掉 ELK**（EPL-2.0、体积大；笔记规模的图用不上）。复杂 flowchart 布局不佳时再开该 feature。
2. **配色仅随系统明暗**（深色 dark 基调/浅色 default），不逐六套主题族定制——没有用户提出这个需求。
3. **解析失败回退代码块 + 一行错误**：不留空白洞，也不让整篇预览挂掉。
4. **覆盖全部渲染面**：阅读态、编辑内预览、⌘T 全窗口、历史预览、帮助弹窗共用一个 TextView 构造 helper；HTML 导出内联 `<svg>`、PDF 导出经 typst 嵌入。源码编辑态永远是文本——渲染是预览期的事。
5. 否决：为一个块类型引入 webview；`mermaid-render` crate（474 下载、近月停滞）；错误占位（那是 bug 报告不是修复）。
6. **SVG ink 裁剪**：merman 对 gantt/pie/mindmap 等族在自身 viewBox 内部预留恒定空白（gantt 底部 ~50px 轴带与任务数无关），预览元素按 canvas 尺寸预留空间 → 图下大面积空白。修复在渲染出口 `render_svg` 统一做：usvg 1x 光栅化 → 扫 alpha>0 的 ink bbox → viewBox 连同 width/height 重写为裁剪框（含 viewBox origin 平移，git/mindmap 有负 origin），2px 呼吸边，逐像素 ink 数裁剪前后相等（0 损失）。解析/光栅化失败保持原 SVG。**二次修复**：裁剪光栅化必须与 gpui 展示光栅化同字体条件——`usvg::Options::default()` 自带空 fontdb，`<text>` 全被丢弃、bbox 漏掉全部文字标签（gpui 侧加载系统字体，文字画在 bbox 外被裁）；`ink_viewbox` 同样 `load_system_fonts()`，两侧一致。教训：两次光栅化共享同一 SVG，任何 options 差异都会变成「裁掉实际画出的内容」这类静默错位。

语法覆盖从此跟随 merman 而非 mermaid.js；缺口以代码块回退呈现。这是接受的边界，不是缺陷。

### 窗口控制与高 DPI 视口：根级右上角 overlay + 启动 DPI 重同步（2026-10-31/11-01 修订）

初版（第十一轮）把 Windows/Linux 自绘按钮放在左栏顶行右端。用户在 175% 缩放的 Windows 工作站上回传两条 bug（按钮位置不对、markdown 标题行右上角工具钮点击失效），排查发现**两层独立根因**：

1. **首要根因：gpui-pre-windows 高 DPI 视口错位。** 窗口创建期间 OS 的 DPI 上下文发生翻转（96 ↔ 显示器 168，远程/虚拟化桌面常见），某次 WM_SIZE 在后端 scale=1.0 时被处理，把**客户端设备像素尺寸记成了逻辑尺寸**。结果：窗口报告 `scale_factor()=1.75`、屏幕上只有 1280×800 设备像素的 client，而布局视口却是 1280×800 逻辑（=2240×1400 设备像素）。场景比后缓冲大 1.75 倍：约 43% 的界面（右侧）画在窗口外，且鼠标命中坐标按 ÷1.75 折算 → 永远到不了逻辑 x>731 的区域。老版布局里标题行工具钮恰好落在该死区（看得见的位置其实已裁切，点击全被窗口拖拽语义吞掉），自绘按钮落在左栏（可见但位置不对）——两条用户回传皆由此起。修复（`leanote-app/src/main.rs` `win_dpi_resync`）：启动后用 Win32 直测活窗口 client 尺寸，当 `视口×scale ≠ 实际 client` 时以 ±2 设备像素的 SetWindowPos 强制 WM_SIZE 按当前 scale 重算逻辑视口（物理尺寸不变），布局/像素/命中测试重新一致；关闭时若修复曾介入，几何按设备像素保存（该环境下创建路径把请求的逻辑值当设备值用，存设备值才能跨启动保持窗口尺寸）。真机验证：caption band 在窗口右上角应答 `HTMINBUTTON`/`HTMAXBUTTON`/`HTCLOSE`，标题行工具区应答 `HTCLIENT`，reader strip 应答 `HTCAPTION`。
2. **次级根因：Drag 区域命中栈语义。** 平台 hit-test 回调（gpui-pre `window.rs:2050`）按「鼠标命中栈中第一个 window-control hitbox」应答 NCHITTEST，父级 hitbox 先于子级入栈。初版按钮嵌在 drag_header 的 Drag hitbox 里 → 按钮区域被父级 Drag 抢答成 HTCAPTION，点击只会拖窗口。修复：按钮移出一切 Drag 区域并改放**根级 overlay（`absolute + top_0 + right_0`，root 最后一个 child）**，贴窗口最右上角（Windows/Linux 惯例），登录/同步期（nav 隐藏）也存在；每个按钮 `occlude()`，BlockMouse hitbox 把下方 drag strip 的 Drag hitbox 从鼠标栈截断，恢复原生应答。关闭钮补上初版裁决的 hover 红底白 icon。
3. **Drag 区域铁律**：Windows 上 WindowControlArea::Drag 的每一像素都是 HTCAPTION，Drag 区域内不得有可交互子元素。同轮修复：编辑器标题行整体曾是 Drag 区 → 标题输入与右侧工具钮全部失效；标题行改为纯 client 区（右端为 caption band 预留 `pr(WIN_CONTROLS_W)`），macOS/Linux 仍经 `start_window_move` 监听拖拽（Windows 该 API 为 no-op）。nav 顶行的搜索钮同理移出 Drag 命中（拖拽区改为搜索钮的兄弟节点）。

2026-11-01 第二轮回传（左栏顶行太空、标题输入框要能拖窗口）补两条：

4. **Windows/Linux 左栏顶行补身份**：图标 + 「Leanote」名称放拖拽区最左端（`nav-app-name`），非交互、可随 Drag 区拖动（Windows 标题栏惯例）。图标与 Windows exe 图标（`packaging/icon.ico`）**同源同字节**（用户 2026-11-01 裁决：锯齿投诉后的最终形态）——`gen-icons.sh` 对 1690px 源图直接 LANCZOS 缩放：不透明纯黑方块 + 白蚁形字标，边缘保留全灰阶抗锯齿（32px 下 138 级灰度）；早先两版（阈值化 alpha 的圆角深色 tile、直缩整图）都因 0/255 二值边缘在小尺寸下呈锯齿被否决。`.ico` 各尺寸与 `assets/app-icon.png`（128px 条目逐字节一致）同管线产出，标题栏经 `include_bytes!` + gpui `img()` 内嵌（LazyLock 缓存）。macOS 维持 issue 20 Q7（红绿灯 + 80px 让位，无应用名）。
5. **标题输入框支持拖窗口**：标题行监听按下位置，位移超过 4px 阈值（`DRAG_THRESHOLD`）即把按压交给系统拖拽——Windows 走 `ReleaseCapture` + `WM_NCLBUTTONDOWN(HTCAPTION)` 接力（`win_dpi_resync::start_window_drag`，client 区无 HTCAPTION 的唯一正解），macOS/Linux 仍 `start_window_move`。**接力必须异步 `PostMessageW`**：首版用同步 `SendMessageW`，把 DefWindowProc 的模态移动循环直接嵌进 gpui 正在分发鼠标事件的 wndproc——消息泵在持借状态下重入 window_procedure（dump：`window_procedure → handle_nc_mouse_down_msg → draw_window → window_procedure` 循环）→ 主线程栈溢出 → `__fastfail` 0xC0000409（用户回传的拖动崩溃）。`PostMessageW` 把按压排队，移动循环改在消息循环层进入——与原生 HTCAPTION 按压同层，gpui 早已兼容（nav strip 拖拽即此路径）。纯点击（<4px）不触发拖拽，光标定位/按钮照常；代价是输入框内拖选文本让位于拖窗（显式需求裁决）。回归测试 `title_input_press_drag_triggers_window_drag`：2px 摆动不拖、40px 拖动恰好交接一次，并证明输入组件不吞标题行按压监听。nav 拖拽区同样加阈值。

**2026-11-01 附记（启动崩溃，已修复）**：恢复上次笔记启动时偶发（后转为必现）`0xC0000409` 栈溢出。运行中进程抓栈（ReadProcessMemory 扫栈 + llvm-symbolizer）定位溢出最内层在 `render_editor` 的标签条 `Div::children`（SmallVec::extend）——**无循环、纯栈预算耗尽**：MSVC 主线程默认 1 MB 栈，编辑器渲染路径（标题行 + 标签 chips + 斜杠面板，各自是 gpui 递归元素子树）debug 构建下本就逼近上限，本轮加深的标题行把它推过线。A/B 确认：HEAD shell 稳定、新 shell 必崩、二者仅差标题行/身份行元素深度。修复：`main()` 在 Windows 上把整个应用搬到 64 MB 栈的工作线程上跑（`thread::Builder::stack_size(64MB)`，主线程 park 等待）——栈页按需提交，不占常驻内存；release 构建树更浅，同样零成本。回归测试 `startup_restores_notebook_selection_and_last_note` 固化恢复语义。

另：排查早期观测到的 gpui-pre-windows 消息泵重入缺陷（元素构造自嵌）为上游问题，vendored `draw_in_progress` 守卫只覆盖 request_frame 路径；本轮栈扩容后该路径也不再触线。

初版「按钮贴窗口最右上角要动右栏标题行并新增第二条 drag strip」的否决理由随之作废：caption band 不新增 strip，靠 overlay 覆盖既有 strip 右端 + 标题行右填充避让。回归测试 `win_controls_overlay_top_right_in_all_phases`：登录期与编辑器 shell 两相断言 overlay 贴右上角、工具钮避开 caption band、nav 身份行在位（macOS 断言无 overlay、无身份行）。

--- 以下为初版裁决留痕 ---

### 窗口控制（初版，已被上节修订）：左栏顶行一条，三平台三种形态

macOS 保持系统红绿灯与现状顶行（仅搜索钮，无应用名——issue 20 Q7 维持）。Windows/Linux 在**左栏顶行右端**自绘按钮：应用图标 + 应用名 + 搜索钮 + 分隔线 + 最小化/最大化/关闭，Windows 11 线 icon 风格，关闭钮 hover 红底。按钮注册 `WindowControlArea::{Min,Max,Close}` hitbox；gpui Windows 后端已把这些映射为 `HTMINBUTTON`/`HTMAXBUTTON`/`HTCLOSE` 并补发对应消息（`gpui-pre-windows` events.rs:955-1148），点击行为免费，只有像素归我们画。

备选方案（按钮贴窗口最右上角、笔记标题工具钮左移）要动右栏标题行并新增第二条 drag strip；每平台一条 44px 顶行优于它。Win 上画 mac 红绿灯（违反用户习惯）与仅 Linux 自绘（gpui `window_decorations()` 对 Win/Linux 同构）均否决。

Win/Linux 标题栏正确性以交叉编译 + gpui 事件链代码审查验证；真机验证留给 CI 产物 + 用户回传。该边界是刻意的。（修订注：用户回传即上面两条 bug，印证了「代码审查代替真机」的边界代价。）

### 同轮其余裁决（留痕）

- **斜杠面板取代工具栏**：markdown 工具栏与设置开关一并删除；4×4 icon 网格（16 格 = 14 插入 + 图片 + 帮助）是唯一格式化入口。插入模式行首/空格后 `/` 触发，vim normal/visual 不触发（`/` 留给未来 vim 搜索）；继续键入按命令名中英模糊过滤；ESC 取消且 `/` 保留，插入执行时删去 `/` 及过滤词。修订 ADR-0004 第二行工具栏设计。
- **斜杠面板修订**：格子改纯 icon 紧凑格（去文字标签）；方向键是 kit 的 `Move*` 动作而非 key_down，经 `capture_action::<MoveUp/Down/Left/Right>` 在面板打开时截下做网格导航（面板关时不拦，编辑器正常移动光标）；ESC 取消时 `/`+过滤串回填编辑器（此前触发键被吃掉导致 `/` 无法输入），backspace 到头/过滤空关闭不回填。
- **⌘Q = 最小原生应用菜单**：`cx.set_menus()` 建标准模板（About / Services / Hide / Hide Others / Show All / 退出 ⌘Q）。此前唯一退出入口是 Dock 右键（tray 已于 issue 20 删除）。Win/Linux 无菜单栏概念不建。
- **打包与图标**：DMG 加 `/Applications` symlink + PIL 纯色底两个圆角槽位提示背景 + AppleScript 图标布局，AppleScript 失败降级为仅 symlink（仍可拖拽）；NSIS 默认创建桌面快捷方式、卸载时删除；图标从单一基准 `logo/icon-black-white.png` 经 `gen-icons.sh` 内嵌 Python/PIL 步骤产出——macOS 一套中性设计（无运行时明暗切换：macOS 无应用图标主题化机制，运行时换会闪 Dock），Windows 黑底白图案圆角矩形。产物提交仓库，CI 无需图形工具链。

[merman]: https://github.com/Latias94/merman
