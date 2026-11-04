# Issue 05: 阅读页——渲染管线复用

状态:已完成(2026-11-04)

## 需求

- 复用桌面阅读态：markdown（高亮、Mermaid、图片 data-URL）、HTML 富文本只读（ADR-0001）。
- 主题族/明暗随系统/缩放；字体 iOS SF Pro。
- 全屏滚动 + 安全区 insets。

## 实现

- **新 crate `rust/crates/leanote-reader`**（桌面与移动共享，不复制第二份）：
  - `mermaid.rs` 自 leanote-app 原样搬入；`install_full` 的 AppShell 耦合改为 `Option<ImageMenuHandler>`（Arc 回调），桌面用 `mermaid_view` 包装接回 `on_image_menu`，移动端传 None。
  - `reader.rs`：`reader_markdown_src`（HTML→md）、`resolve_note_images`（leanote:// → data URL + 远程图补 token）、`markdown_view_style(accent)`（accent 改传参）、`markdown_view(id, src, dark, accent)` 一行构造。相关测试随迁。
  - 桌面 leanote-app：调用点改 import、`mod mermaid` 删除、browse.rs 移除已迁函数、merman/usvg/typst-kit 依赖转出。
- **移动端阅读屏**（`notes_tab.rs` Screen::Reader{note_id}）：`get_note` → `resolve_note_images` → `reader_markdown_src` → `markdown_view`，滚动容器 + 顶部安全区。深链扩为 `reader:<noteId>`。
- **明暗随系统**：启动时读 `window.appearance()`（embedded 模式 keyWindow 未建，App 级 sync 会误落 Light——踩坑记录）；`observe_window_appearance` 用窗口外观而非 App 级 sync 重放 mode+accent。
- **vendored 修复**：`gpui-pre/src/svg_renderer.rs` 的 SYSTEM_FONT_DB 在 iOS 上补 `load_fonts_dir("/System/Library/Fonts")`——fontdb 无 iOS 后端，SVG `<text>` 全丢（Mermaid 节点曾空白）。

## 验证

- 模拟器截图核实（fake 语料 n4/n5/n10/n11/n12）：
  - Mermaid 流程图完整（中文节点 笔记/渲染、边标签 md/mermaid）；
  - TinyMCE HTML 只读（标题/加粗/嵌套列表/表格头；行内 color 丢失是 html2md 管线固有属性，桌面一致）；
  - 本地图片（1×1 红点 PNG 经 `leanote://` → data URL）；
  - GFM 表格/引用块/行内 code/链接；
  - 深色模式（simctl appearance dark）：暗底亮字、表格/Tab 栏对比度正常。
- 三平台 check 绿；`cargo test`（app+core+reader）8 套件全 ok。
- 缩放三档（100/110/120%）移入 issue 07 随设置面板交付（prefs 未接，验收偏差记录在案）。

## Comments

- 触摸滚动手势（pan→scroll）在阅读长文时的手动走查并入 issue 09 验收（simctl 无 tap/drag API）。
- 用户并行 WIP（ADR-0014 Navop）与本次搬迁的冲突已顺手理顺：theme.rs 重复 CURRENT_ACCENT 清理、main.rs Theme import 恢复——均按用户 diff 意图最小修复。
