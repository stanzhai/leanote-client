# Issue 17: 笔记详情支持屏幕左缘右滑返回

状态:已完成(2026-11-07;真机手感复验待用户——阈值 24/48px 是逻辑值，观感可再调)

## 需求

查看笔记详情（及一切推入栈的浏览屏）时，除点返回按钮外，支持从屏幕左缘右滑返回
（spec「移动端导航」承诺的「边缘右滑或系统返回键回退」，此前只实现了返回按钮）。

## 方案

平台层（vendored gpui-pre-mobile）没有边缘手势支持，也没有返回导航事件；但触摸
pan 会被手势识别器转成 **ScrollWheel 事件走常规派发**（ios/events.rs：
touch → gesture recognizer → `RecognizedTouchGesture::Scroll` → 元素派发）。
因此纯 GPUI 层即可实现识别，零 vendoring，iOS/Android 同一份代码。

### 识别器（notes_tab.rs）

- 非根屏（栈深 > 1）的屏容器挂 `on_scroll_wheel`；根屏不挂。
- 起手门（`TouchPhase::Started`）：位置在左缘 24px 内 **且** delta 是 `Pixels`
  （触摸 pan 的单位；真实滚轮/触控板是 `Lines`，天然排除）。
- 累计（`Moved`）：累计 (dx, dy)；`dx ≥ 48px` 且横向主导（`dx > 2·|dy|`）→ `pop()`；
  纵向主导（`|dy| > dx`）→ 废弃手势交给列表竖滚；Ended/Cancelled → 复位。
- **只观察不消费**：不 stop_propagation，滚轮事件继续冒泡，阅读页竖滚、表格横向
  滚动不受影响；push/pop 均复位手势状态。
- 无 overlay 元素：手势识别挂在屏容器本身，不挡任何点击（返回按钮在左上角仍可用）。

## 验证

- `edge_swipe_right_pops_non_root_screen`（真实测试窗口）：注入**原始 TouchEvent**，
  走与真机手指完全相同的管线（手势识别器 → pan 转滚轮派发 → 监听器）：
  - 屏幕中部（x=200）起手右滑 80px → 不返回；
  - 左缘（x=8）起手右滑 72px → pop 回根屏。
- `cargo test -p leanote-mobile` 10/10 绿；`cargo check --target aarch64-apple-ios` 绿。

## 已知边界

- 阈值 24/48px 为逻辑像素；iOS 原生边缘手势约 20pt 起手。观感不对就在
  `EDGE_SWIPE_START`/`EDGE_SWIPE_MIN` 两个常量上调。
- Android 系统返回键：gpui-pre-mobile 的 android 层目前未把 back 键转成 GPUI 事件，
  属平台层缺口（另立 issue，不与本手势混做）。
