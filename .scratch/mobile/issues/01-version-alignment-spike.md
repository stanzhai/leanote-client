# Issue 01: 版本对齐 spike——vendor gpui-mobile 对上 vendored gpui-pre 0.3.9

状态:已完成(2026-11-03)

## 需求

- vendor `longbridge/gpui-mobile` 源码到 `rust/vendor/gpui-pre-mobile`，`[patch.crates-io]` 指向它，依赖改为 vendored `gpui-pre` 0.3.9。
- 验证 `cargo check --target aarch64-apple-ios`（+ android 目标 check）能否收敛；评估 Platform trait 0.3.4→0.3.9 的漂移成本。
- 顺手盘点 gpui-kit 0.7.3 组件在移动端的触摸可用性（哪些组件可直接用、哪些要绕）。
- 失败 fallback：vendored gpui-pre 降级 0.3.4 + 重放 IME 补丁（`prefers_ime_for_printable_keys = false`），并跑桌面回归（`cargo test -p leanote-app`）。

## 实现

主路径（A：向上 patch）成功，无需 fallback：

- Vendor 源：`longbridge/gpui-mobile@main`（2026-11-03，git main 已指向 gpui-pre =0.3.10，比 crates.io 0.1.0 的 =0.3.4 新）。
- 新 workspace member `crates/leanote-mobile`（spike 骨架，issue 02 的种子）直接 path-dep vendored crate；`rust/Cargo.toml` 不加 patch（path patch 必须满足 `=0.3.10` 版本要求，故改为直接把 vendored crate 的依赖降到 0.3.9）。
- vendored crate 本地改动（全部记录在其 Cargo.toml 注释与 leanote-mobile/Cargo.toml）：
  1. `offscreen` feature + `src/offscreen.rs` 移除——需要 patched gpui-pre-wgpu（headless renderer），v1 无此需求。
  2. `gpui-pre`/`gpui-pre-wgpu` 依赖 `=0.3.10` → `=0.3.9`。
  3. `wgpu` 依赖 `30.0.1` → `29`（与 gpui-pre-wgpu 0.3.9 统一，否则双 wgpu 版本类型冲突）。
  4. ios/android `platform_info()` Platform trait 实现删除——0.3.9 trait 无此方法（0.3.10 新增，我们不消费）。
  5. ios `platform_view.rs` 的 camera/video_player 引用 feature 门控（default-features=false 下编译）。
- `leanote-core/src/sync.rs` MAC 地址 cfg 守卫扩到移动端：`macos`→`any(macos, ios)`（AF_LINK 同 BSD 栈）、`linux`→`any(linux, android)`（AF_PACKET 同栈）。

## 验证

- `cargo check -p leanote-mobile --target aarch64-apple-ios` ✅（需 SDKROOT 指 Xcode 的 iPhoneOS.sdk——本机 xcode-select 指向 CommandLineTools，见 Comments 环境注记）
- `cargo check -p leanote-mobile --target aarch64-linux-android` ✅（NDK 28.2，prebuilt 为 darwin-x86_64，api24 clang）
- 桌面回归：`cargo check -p leanote-app` ✅，`cargo test -p leanote-app -p leanote-core` 105+52 全绿

## Comments

### 组件触摸可用性盘点（gpui-component 0.7.3 / gpui-pre 0.3.9）

- ✅ 可直接用：所有 `on_click` 组件（56 处）——gpui-pre gesture arena 把 tap 映射成 `ClickEvent::Touch`；滚动容器（pan→scroll，含动量）；TextView/markdown 渲染（纯绘制，v1 阅读页核心）。
- ⚠️ 桌面假设需绕：hover 交互大量存在（仅 input 目录 63 处 hover 引用）——tooltips/悬浮态在触摸上无意义，v1 自建壳不复用依赖 hover 的桌面 widget；右键菜单、ScrollWheel 语义、drag-resize 同理。
- ❌ 已知缺口：移动 IME 组合输入未完成（gpui-mobile 自述）——v1 搜索/登录输入框是唯一暴露面；Android 宿主须用 `GpuiInputActivity` 接 multistage IME。中文拼音输入实测是 issue 06 的验收项。
- 结论：v1 的 Tab/列表/阅读页自建触摸壳 + 复用 kit 的渲染类组件，是低风险组合；kit 的桌面输入/交互类组件谨慎复用。

### 环境注记（issue 02/09 需要）

- iOS：`xcode-select` 当前指向 CommandLineTools（无 iphoneos SDK）。构建需 `SDKROOT=/Applications/Xcode.app/Contents/Developer/Platforms/iPhoneOS.platform/Developer/SDKs/iPhoneOS.sdk CC_aarch64_apple_ios=<xctoolchain>/clang`；正式方案是 `sudo xcode-select -s /Applications/Xcode.app`（需人工执行）。
- Android：NDK 28.2.13676358（prebuilt/darwin-x86_64），`CC_aarch64_linux_android=<ndk>/bin/aarch64-linux-android24-clang`；`cargo-ndk` 未装（issue 10 时再装）。
