# Issue 02: leanote-mobile crate 骨架 + iOS 宿主工程

状态:已完成(2026-11-04)

## 需求

- 新 crate `rust/crates/leanote-mobile`（workspace member），`rust/mobile/ios/` XcodeGen 宿主工程，照抄 gpui-mobile example 结构。
- 登录页：host 输入（placeholder `https://leanote.com`，自托管可配）+ email/密码，复用 `leanote-core` `Api::login` 语义（app_shell.rs:313 的桌面行为对齐）。
- 空壳底部 Tab（笔记/搜索/设置）+ 推入栈导航 + `set_back_handler`（iOS 先用边缘滑返）。

## 实现

- `crates/leanote-mobile/src/`：`lib.rs`（iOS 入口 `leanote_ios_register_app` + `apply_theme` Wake accent 覆盖）、`login.rs`（InputState 三字段 + `Api::login` 后台线程，对齐桌面 `run_login_sync` 模式）、`app.rs`（登录门 `MobileApp` + `TabShell` 底部 Tab）、`fake.rs`（`LEANOTE_FAKE_SERVER=1` 进程内 fake server + 自动登录——模拟器无法程序化键入，冒烟走真实登录路径；桌面同款约定）。
- `rust/mobile/ios/`：App.swift（gpui-mobile embedded 模式，UIKit CADisplayLink 驱帧）、Embedding.h、Info.plist、project.yml（XcodeGen；prebuild 脚本带 SDKROOT/CC 环境变量兜底 xcode-select 指向 CLT 的问题）。
- **关键坑（记录给后续 issue）**：kit 主题覆盖（`Theme::update`）必须在 `open_window` **之后**调用——窗口创建时按当前 mode 重载注册主题表，先覆盖会被冲掉（选中 Tab 曾渲染成白色）。`apply_theme` 设 accent/primary/button_primary + tokens 双写（对齐桌面 apply_wake_theme 的 reconcile 语义）。完整主题族/明暗随系统在 issue 07。
- 推入栈导航本体顺延到 issue 04（有真实列表页才有栈）；Tab 壳的 on_click 状态机已就位。

## 验证

- `cargo check -p leanote-mobile` 三平台（host/aarch64-apple-ios/aarch64-linux-android）全绿。
- iPhone 17 Pro 模拟器：冷启动登录页截图验证（三字段中文标签 + 登录钮）；`LEANOTE_FAKE_SERVER=1` 自动登录 → Tab 壳截图 + 像素采样：选中 Tab 文字 `#0a84ff`（Wake 蓝），未选中 115 灰。
- 桌面回归：`cargo test -p leanote-app -p leanote-core` 6 套件全 ok。

## Comments

### 验证限制

- 模拟器无法程序化点击/键入（simctl 无 tap API），Tab 切换的点击未自动化验证。切换是单行 on_click 状态翻转 + notify，与已验证的「登录事件 → Entity 切换」同一渲染机制；issue 04 接真实列表时一并手动走查。
- `xcode-select` 仍指向 CommandLineTools；构建走 project.yml 里的显式 SDKROOT/CC。正式切换 developer dir 后可删兜底。
