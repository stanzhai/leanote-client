# Issue 12: Xcode 调试启动崩溃（Metal 验证层两连坑）

状态:已完成(2026-11-04)

## 需求

真机报告：Xcode Run 启动即停，`Thread 1 Queue : com.apple.main-thread (serial)`，栈帧 `GPUITextView.init App.swift:13`（即 `gpui_ios_run_demo()`）。simctl 裸启动完全正常，10/10 存活。

## 根因（模拟器系统日志 `log show` 抓到 panic 原文）

1. **selector 缺失 panic**：`objc2-metal MTLDevice.rs: invalid message send to -[CaptureMTLDevice supportsShaderBarycentricCoordinates]: method not found`。Xcode Debug scheme 默认开 Metal API Validation，设备被包成 `CaptureMTLDevice` 包装器；wgpu-hal 29.0.4 在 adapter 探测时直接发该 selector，objc2 生成代码按契约 panic → abort。wgpu-hal **30.0.1 已修**（respondsToSelector 守卫），但 gpui-pre-wgpu 0.3.9 锁 wgpu "29.0" 升不上去。
2. **buffer 对齐违规**（修完 1 后暴露）：`fs_mono_sprite: gamma_params offset 128 must be multiple of 256`。wgpu-hal metal 后端在 iOS 报 `buffer_alignment: 64`（上游 30.0.1 仍是 64，真机硬件接受），但 iOS 模拟器的验证层强制 256 → 中止。

## 实现

vendor `wgpu-hal` 29.0.4（`rust/vendor/wgpu-hal`）+ `[patch.crates-io]`，共两处改动，均带注释：

1. `src/metal/adapter.rs` shader_barycentrics 探测加 `device_class_responds_to` 守卫（30.0.1 的修法回移）。
2. `src/metal/adapter.rs` `buffer_alignment` iOS 64 → 256（保守值，macOS 本来就是 256；多花几字节 padding）。

## 验证

- 复现钥匙：`SIMCTL_CHILD_MTL_DEBUG_LAYER=1 simctl launch`（无需 Xcode 即可注入验证层）。
  - 补丁前：启动即死（panic method not found）；
  - 只带守卫：过启动，死在 gamma_params offset 128 验证错误；
  - 两补丁齐：**ALIVE**，截图正常渲染（最近列表 + Tab 栏），日志 0 PANIC。
- iOS 26.2 / 27.2 双模拟器普通启动存活；`cargo check` host（leanote-app 桌面同受益）+ aarch64-apple-ios + aarch64-linux-android 全绿。

## 残留

- 真机 Xcode 调试需实测（模拟器已覆盖两条死因；buffer_alignment 256 对真机更保守无风险）。
- vendor 升级 wgpu/gpui-pre-wgpu 时按 Cargo.toml 注释 Re-vendor（两处 leanote 改动需重放）。
