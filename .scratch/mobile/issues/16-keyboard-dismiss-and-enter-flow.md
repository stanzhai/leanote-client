# Issue 16: 输入框弹出系统键盘后无法收起

状态:已完成(2026-11-07;真机复验待用户——iOS 键盘 Return 键到 Enter 事件的映射是平台层行为，无头测试覆盖不到)

## 需求

真机报告：文本框输入弹出系统键盘后键盘一直在，无法取消。期望：
- 搜索：回车确认搜索并把焦点切走（收键盘）；
- 登录：输入框类似处理。

## 根因

移动端两层缺口：

1. **UI 层从未主动交还焦点**：搜索框只挂了 `InputEvent::Change`（防抖搜索），登录表单
   完全没挂事件——没有任何路径把焦点移出输入框。iOS 端键盘与焦点绑定
   （vendored gpui-pre-mobile `TextInputStateChange::FocusGained → show_keyboard` /
   `FocusLost → hide_keyboard`，ios/window.rs），焦点不离开，键盘永远不收。
2. **收键盘的通道问题**：`InputEvent` 订阅回调只有 `Context<T>`（拿不到 `Window`），
   而经 `cx.active_window()` 取窗口在 iOS 上是**返回 None 的 stub**
   （ios/platform.rs:153，注释自认 "would need to track the current key window"）——
   直接走它会在真机上静默空转。

## 实现

- `lib.rs` 两个共享 helper：`blur_active_input(wh, cx)`（`Window::blur` → FocusLost →
  hide_keyboard）、`focus_input(wh, &target, cx)`（表单「下一项」）。窗口一律用
  **构造期捕获的 `AnyWindowHandle`**（Copy），不依赖 iOS 平台 stub。
- **搜索**（search_tab.rs）：
  - 订阅加 `InputEvent::PressEnter` → 丢弃残留 250ms 防抖任务、`run()` 立即定稿、
    `blur_active_input` 收键盘；
  - 结果行点击跳阅读屏前直接 `window.blur(cx)`（listener 里有 window）。
- **登录**（login.rs）：
  - 回车链 host→email→password（`focus_input` 移焦点，键盘保持），password 回车 =
    `submit()`；
  - `submit_with` 开头统一 `blur_active_input`——按钮点击或回车进入都收键盘，
    失败回错误提示时键盘不再压在界面上。

## 验证

- `login::tests::enter_chains_fields_and_submits`（真实测试窗口 + 真实 helper）：
  host 回车 → email 获得焦点；email 回车 → password 获得焦点；password 回车 →
  提交路径已走（busy 或 error）且 password 不再持有焦点。**修复前红**（第一版走
  `cx.active_window()` 的实现在同构环境即失败，暴露了 iOS stub 问题）。
- `search_tab::tests::enter_finalizes_search_immediately`：真实 Store + 预置笔记，
  `set_value`（不发 Change、无防抖）后回车 → hits 同步就绪（=1）——回车路径真的
  跑了搜索而不是等防抖。
- `cargo test -p leanote-mobile` 9/9 绿；`cargo check --target aarch64-apple-ios` 绿。
