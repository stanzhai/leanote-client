# Issue 10: Android 跟进（iOS v1 通过后启动）

状态:代码完成(2026-11-04)；模拟器走查通过(2026-11-08，修复渲染/数据目录/kit init 三处后登录页正常渲染)

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
- **模拟器运行验证（2026-11-08 通过）**：AVD android-34 google_apis arm64 + emulator 37.2.13（`-gpu host`，本机 M2 Max）。曾卡 splash 三层原因，均已修：
  1. GL 回退被拒：模拟器 host 翻译器仅 GLES 3.0（无 vertex storage、compute 限值 0），gpui-pre-wgpu 固定请求 `downlevel_defaults()`（storage=4/compute=65535）必败 → vendor gpui-pre-wgpu：无 `VERTEX_STORAGE` downlevel flag 时用 `downlevel_webgl2_defaults()`，`uses_webgl_instance_data()` 同 flag 判定（原 wasm-only）。
  2. `app_data_dir()` 在 Android panic（无 HOME）→ `LEANOTE_DATA_DIR` env 覆盖，android_main 从 `AndroidApp::internal_data_path()` 注入。
  3. Android 入口漏 `gpui_kit::init(cx)`（iOS 有）→ ThemeRegistry panic。
  验证：`init_done=true`、无 panic、截图登录页（服务器/邮箱/密码/登录）完整渲染。注意：模拟器 Vulkan 不可用（MoltenVK device lost / SwiftShader pipeline hang），实际走 GL；真机 Vulkan 待验。
  安装坑：1.4G debug 包需 `llvm-strip --strip-debug` 两枚 .so，否则 2.7G 装不进；连装失败先 `adb uninstall` + `adb reboot`（PM staging 泄漏）。
  - 真机验证路径（自用 v1 本来就是真机侧载）：`adb install rust/mobile/android/gradle/app/build/outputs/apk/debug/app-debug.apk`（strip 后 1.4G；release 构建走 `mobile/android/build.sh --release` 显著缩小）。

## Comments

- APK 签名：debug 签名可直接侧载；正式自用签名（APK 自签）待真机首装时补。
- 2026-11-08 后续：顶栏与状态栏重叠（笔记本切换/详情返回键点不到）——Android 窗口未实现 `PlatformWindow::insets()`（upstream 默认 0，iOS 有同款 leanote 补丁），app `.pt(safe.top)` 全为 0。vendor/gpui-pre-mobile android/window.rs 补 insets()（`safe_area_insets_logical()`），模拟器截图验证内容下移至状态栏之下。upstream gpui-mobile 同样缺此实现，升级 re-vendor 时保留。
- 2026-11-08 后续 2：模拟器 macOS 物理键盘不能输入——① AVD `config.ini` `hw.keyboard=no` 默认（命令行 avdmanager 建的），改 yes 重启模拟器；② gpui-pre-mobile Android 键事件只发裸 KeyDown，gpui 编辑器插入文字仅走 input handler（IME 路径），裸 KeyDown 永不插入（iOS `handle_text_input` 一直先走 handler）。vendor 补：可打印字符 DOWN 直接 `replace_text_in_range` 进聚焦输入框，控制键保持 KeyDown。注意 if-let 判别式里的 `RefCell::borrow_mut()` 在 2021 edition 活到块尾——take 要先拆出来，双 borrow 直接 panic（已踩）。re-vendor 保留。
