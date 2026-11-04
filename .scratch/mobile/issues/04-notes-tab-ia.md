# Issue 04: 笔记 Tab——最近列表 + 笔记本/标签浏览

状态:已完成(2026-11-04)

## 需求

- 「笔记」Tab 默认最近列表（RECENT_LIMIT=20，UpdatedTime 倒序）。
- 笔记本（嵌套）/标签浏览入口（标签从 notes.Tags 聚合）。
- 触摸滚动、行点击推入阅读页。
- 回收站、Star、多选不做（裁剪清单见 spec）。

## 实现

- 新 `src/notes_tab.rs`：`NotesTab`（内部栈 `Vec<Screen>`：Root/Notebooks/Tags/NoteList/Reader）。
  - 最近列表 = `list_notes_meta_recent(20)`；笔记本 = `notebooks()` 父先子后按 Seq、子层缩进；标签 = `tag_note_counts()`（notes.Tags 聚合，桌面同源）；NoteList = `list_notes_filtered_meta` / `list_notes_by_tag_meta`。
  - Reader 为占位屏（标题 + 返回头），真渲染 issue 05。
  - 滚动：`div.id().overflow_y_scroll()`（gesture arena pan→scroll）。
- `MobileApp.attach_notes_tab`：boot 已登录与同步完成两条路径共用；issue 03 的 note_count 占位删除。
- **vendored 改动（root-cause 修复）**：
  1. `vendor/gpui-pre/src/window.rs`：`Window::insets()` 公开访问器（原 `platform_window` 是 pub(crate)）。
  2. `vendor/gpui-pre-mobile/src/ios/window.rs`：`PlatformWindow::insets()` 实现——上游留了 `safe_area_insets()` 但没接 trait（默认全零），把 safe area 路由进去。运行时实测 t=62/b=34（iPhone 17 Pro）。
- 安全区：NotesTab 顶部 `pt(safe.top)`、Tab 栏底部 `pb(safe.bottom)`。
- Dev 深链 `LEANOTE_INITIAL_SCREEN=notebooks|tags`（example 的 `GPUI_INITIAL_SCREEN` 同款）：模拟器无法点击，冒烟直达子屏。

## 验证

- 模拟器截图核实：最近列表（4 篇按时间倒序 + desc 摘要 + 浏览笔记本/标签入口）、笔记本屏（Inbox/Work/Research，Research 缩进）、标签屏（demo 1/gpui 1/rust 2 篇，计数正确）、最近 header 与状态栏无重叠、Tab 栏底部避开 home indicator。
- 触摸滚动手势与行点击→Reader 推入未自动化（simctl 无 tap）：push/pop 与 tab 切换是同一 on_click 机制，且深链已覆盖栈的非根屏渲染；issue 05 验收时真机/模拟器手动走查。
- 三平台 check 绿；桌面 6 套件回归 ok。
