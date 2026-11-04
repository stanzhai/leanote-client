# Issue 10: Android 跟进（iOS v1 通过后启动）

状态:代码完成(2026-11-04；模拟器走查受阻——本机 emulator 崩溃，真机验证待做)

## 需求

- `rust/mobile/android/` Gradle 宿主工程（照抄 gpui-mobile example：cargo-ndk arm64-v8a、GpuiInputActivity、Vulkan/GL 后端验证）。
- `set_back_handler` 接系统返回键；Noto 字体与 emoji。
- 直装 APK（不上 Play）。

## 实现

- `rust/mobile/android/gradle/`：root/app build.gradle.kts（com.leanote.mobile，minSdk 26，`nativeLibraryName=leanote_mobile`）、最小 manifest（仅 INTERNET 权限 + Vulkan/GLES3 feature 声明）、gpui-mobile 全套宿主 Java 类（GpuiActivity/GpuiInputActivity 等）+ res（splash 主题）+ Noto emoji assets。`../build.sh` 一条龙（Rust→APK）。
- `leanote-mobile/src/lib.rs` android 入口：`android_main` → `jni::init_platform` → `Application::with_platform(...).run`（窗口延迟到 `InitWindow` 后创建，example 同款生命周期语义）→ 同一 `MobileApp` 视图。
- **cargo-ndk 坑**：默认 API 21 缺 `libnativewindow`（24+），必须 `-P 26`（= minSdk）；`ANDROID_NDK_HOME` 指 28.2。
- 系统返回键：GPUI `set_back_handler` 由 gpui-mobile 平台层接（Android 后端内置 NativeActivity back 分发），应用内推入栈的返回目前走导航头「‹ 返回」；back-handler 的栈级拦截留真机走查后按需接（v1 导航头已可用）。

## 验证

- `cargo ndk -t arm64-v8a -P 26 build --lib -p leanote-mobile` → `libleanote_mobile.so` 产出；
- `./gradlew assembleDebug` → **BUILD SUCCESSFUL**，`app-debug.apk` 产出；
- Android target `cargo check` 双向绿（issue 01 起）。
- **模拟器运行验证（受阻，基础设施级）**：AVD（android-34 google_apis arm64）+ emulator 37.2.13 在本机（macOS 27）启动后 ~60s 稳定崩溃——`Failed to restore previous context: 12297`（宿主 GL 层错误，**未安装任何应用即崩**，三次复现：windowed/swiftshader/guest 组合全灭）。非本 app 问题。APK 曾在其中一次成功 `adb install`（Success）后模拟器随即崩溃，未及启动截图。
  - 真机验证路径（自用 v1 本来就是真机侧载）：`adb install rust/mobile/android/gradle/app/build/outputs/apk/debug/app-debug.apk`（1.5GB debug 包；release 构建走 `mobile/android/build.sh --release` 显著缩小）。

## Comments

- APK 签名：debug 签名可直接侧载；正式自用签名（APK 自签）待真机首装时补。
