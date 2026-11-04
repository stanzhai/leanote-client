# Issue 11: iOS 列表溢出——Tab 栏被推出屏 + 无法滚动

状态:已完成(2026-11-04)

## 需求

真机报告：首页笔记列表下方没有 Tab 切换按钮，列表无法滚动，因此「浏览笔记本」入口（切笔记本的唯一路径）不可达。

## 根因

`TabShell`（app.rs）内容区 wrapper `div().flex_1()` 缺 `min_h_0()`。flex item 的自动最小尺寸 = 内容 min-content：最近列表（RECENT_LIMIT=20 + 2 入口行）超过一屏后，wrapper 被撑到整列表高——

1. TabShell 纵向列溢出，Tab 栏（h 50 + safe bottom）被推出屏幕底部；
2. 滚动容器 `size_full` = 超高 wrapper 的 100% = 内容等高，无溢出可滚。

issue 04 截图走查未暴露：fake 数据当时仅 9 篇，一屏装得下。三个 Tab（notes/search/settings）共用同一 wrapper，一处修全修。

## 实现

- `app.rs` TabShell：`.flex_1()` → `.flex_1().min_h_0()`（一行；附根因注释）。
- `fake.rs`：fake 笔记 9 → 27（18 条填料），冒烟数据从此超过 20 条上限，同类溢出回归在模拟器即可见。

## 验证

- 模拟器（iPhone 17 Pro, iOS 26.2）复现：27 篇 → 修复前截图无 Tab 栏、列表底缘截断；修复后 Tab 栏固定可见。
- 触摸滚动（simctl 无 tap/swipe、本机无 Simulator.app GUI）：临时 XCUITest target 注入真实拖拽（press-for-duration-then-drag），xcresult 附件对比——拖前顶行「填料 18」，拖后可见「填料 01 … 启动增量新笔记 图片笔记 浏览笔记本 标签」，Tab 栏全程固定。走完 UIKit touches → GPUI gesture arena → scroll 全链路。脚手架用毕已删（project.yml/pbxproj 还原）。
- `cargo check -p leanote-mobile`：aarch64-apple-ios、aarch64-linux-android 双绿（CI 同款门禁）。
