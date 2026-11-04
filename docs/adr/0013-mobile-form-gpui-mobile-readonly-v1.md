# ADR-0013: 移动端形态——gpui-mobile 单栈 + 纯阅读 v1

- 状态：Accepted（2026-11-03，grill 会话裁决）
- 关联：GLOSSARY「移动端」「移动端导航」；`.scratch/mobile/spec.md`（裁剪清单与执行拆解）

## 背景

要在 Rust 客户端形态上引入 iOS/Android 移动端，阅读为主。备选路线：Tauri 2 mobile（web UI 第二技术栈，与 ADR-0001 刻意放弃 web 富文本的路线相悖）、Swift/Kotlin 原生（rust 内核过 FFI、双端双栈，与「基于 Rust 客户端形态」意图最远）。

事实基础：vendored `gpui-pre` 0.3.9 无任何移动后端（lockfile 仅 macos/windows/linux/web）；Longbridge（gpui-kit 同一作者）的 `gpui-pre-mobile` 0.1.0 提供 iOS（Metal）+ Android（Vulkan）平台层，但锁 `gpui-pre` **=0.3.4**，且自称 experimental——移动 IME 组合输入未完成、无 accessibility、lifecycle hooks 未完成。桌面 `app_shell.rs` 全是桌面假设（960×600 最小窗、全局键绑定、自绘标题栏、tray、单实例锁），不可复用。

## 决策

1. **单栈 gpui-mobile**：新 crate `rust/crates/leanote-mobile` + `rust/mobile/{ios,android}/` 宿主工程（XcodeGen + Gradle，照抄 gpui-mobile example 结构）。复用资产 = `leanote-core`（SQLite 存储 + ureq 同步）、`theme.rs`、markdown/Mermaid 渲染管线、领域模型；不复用 `app_shell.rs`。
2. **v1 纯阅读**：只拉不推。无编辑、无 Vim、无 Star、无标签管理、无回收站、无历史版本、无导出/PDF、无多选、无 ⌘K/斜杠面板、无专注/全窗口预览、不跑本地 HTTP 服务/CLI/MCP、单账户。搜索框是唯一文本输入点——gpui-mobile 的 IME 缺口由此规避。
3. **数据架构**：复用 leanote-core，设备端 SQLite 全量+增量同步，离线可读。手机 = 同一账户的**第二个同步客户端**；桌面是唯一写端，故无冲突面。
4. **版本对齐**：spike 先 vendor gpui-mobile 源码并 `[patch.crates-io]` 指向它、依赖改到 vendored 0.3.9；失败 fallback = vendored gpui-pre 降级 0.3.4 + 重放本地 IME 补丁（`prefers_ime_for_printable_keys = false`，ADR-0005）。
5. **平台顺序**：iOS 先行（macOS 开发闭环最短，签名基础已有 ADR-0012）；Android 是验收后的跟进 issue，同 crate 同 UI。
6. **发布与凭据**：TestFlight + 直装 APK 自用，不上架。token v1 与桌面同语义（明文存 app 沙箱内 SQLite；leanote 协议本身 token 走 URL query）。Keychain/Keystore 是**上架前置项**，不是 v1 前置项（ponytail: 沙箱内明文，商店发布前升级安全存储）。
7. **触摸交互**：底部 Tab（笔记/搜索/设置）+ 推入栈 + 边缘滑返/Android `set_back_handler`；列表下拉刷新 + 启动自动增量同步；纯前台同步，无后台刷新。

## 后果

- 早期采用者风险自担：gpui-pre-mobile 86 下载、47 star；平台层坑（无障碍、lifecycle）自己填。**移动端编辑功能的解锁前置条件 = gpui-mobile IME 组合输入成熟**。
- Star 缺口显式化：Star 是纯本地字段（GLOSSARY），手机与桌面收藏互不可见；跨端收藏需要服务端支持（leanote 无此 API），不在本 ADR。
- CI 增加 `cargo check --target aarch64-apple-ios`。
- 视觉语言复用：六套主题族 + 主题色 + 明暗随系统 + 界面缩放三档原样生效；字体 iOS SF Pro / Android Noto（gpui-mobile 自带方案）。
