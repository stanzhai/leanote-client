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

### 窗口控制：左栏顶行一条，三平台三种形态

macOS 保持系统红绿灯与现状顶行（仅搜索钮，无应用名——issue 20 Q7 维持）。Windows/Linux 在**左栏顶行右端**自绘按钮：应用图标 + 应用名 + 搜索钮 + 分隔线 + 最小化/最大化/关闭，Windows 11 线 icon 风格，关闭钮 hover 红底。按钮注册 `WindowControlArea::{Min,Max,Close}` hitbox；gpui Windows 后端已把这些映射为 `HTMINBUTTON`/`HTMAXBUTTON`/`HTCLOSE` 并补发对应消息（`gpui-pre-windows` events.rs:955-1148），点击行为免费，只有像素归我们画。

备选方案（按钮贴窗口最右上角、笔记标题工具钮左移）要动右栏标题行并新增第二条 drag strip；每平台一条 44px 顶行优于它。Win 上画 mac 红绿灯（违反用户习惯）与仅 Linux 自绘（gpui `window_decorations()` 对 Win/Linux 同构）均否决。

Win/Linux 标题栏正确性以交叉编译 + gpui 事件链代码审查验证；真机验证留给 CI 产物 + 用户回传。该边界是刻意的。

### 同轮其余裁决（留痕）

- **斜杠面板取代工具栏**：markdown 工具栏与设置开关一并删除；4×4 icon 网格（16 格 = 14 插入 + 图片 + 帮助）是唯一格式化入口。插入模式行首/空格后 `/` 触发，vim normal/visual 不触发（`/` 留给未来 vim 搜索）；继续键入按命令名中英模糊过滤；ESC 取消且 `/` 保留，插入执行时删去 `/` 及过滤词。修订 ADR-0004 第二行工具栏设计。
- **斜杠面板修订**：格子改纯 icon 紧凑格（去文字标签）；方向键是 kit 的 `Move*` 动作而非 key_down，经 `capture_action::<MoveUp/Down/Left/Right>` 在面板打开时截下做网格导航（面板关时不拦，编辑器正常移动光标）；ESC 取消时 `/`+过滤串回填编辑器（此前触发键被吃掉导致 `/` 无法输入），backspace 到头/过滤空关闭不回填。
- **⌘Q = 最小原生应用菜单**：`cx.set_menus()` 建标准模板（About / Services / Hide / Hide Others / Show All / 退出 ⌘Q）。此前唯一退出入口是 Dock 右键（tray 已于 issue 20 删除）。Win/Linux 无菜单栏概念不建。
- **打包与图标**：DMG 加 `/Applications` symlink + PIL 纯色底两个圆角槽位提示背景 + AppleScript 图标布局，AppleScript 失败降级为仅 symlink（仍可拖拽）；NSIS 默认创建桌面快捷方式、卸载时删除；图标从单一基准 `logo/icon-black-white.png` 经 `gen-icons.sh` 内嵌 Python/PIL 步骤产出——macOS 一套中性设计（无运行时明暗切换：macOS 无应用图标主题化机制，运行时换会闪 Dock），Windows 黑底白图案圆角矩形。产物提交仓库，CI 无需图形工具链。

[merman]: https://github.com/Latias94/merman
