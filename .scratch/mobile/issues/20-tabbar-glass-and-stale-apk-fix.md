# Issue 20: Tab 栏改 iOS 26/27 悬浮玻璃胶囊；修复 Android APK 一直打进旧 Rust 库

状态:已完成(2026-11-11;iOS 深色模式跟随在模拟器未生效为既有问题，见残留)

## 需求

1. 移动端进笔记阅读屏要**完全隐藏**底部面板——用户报告"底部还是有内容，不确定是不是框架 UI"。
2. 主界面底部 Tab 改 iOS 26/27 风格：圆角长条**悬浮**矩形、玻璃磨砂质感。

## 根因排查（需求 1）

iOS 模拟器（iPhone 18 Pro, iOS 27.2）装 11-07 构建实测：`reader:n1` 深链进阅读屏，底部 460px 纯
背景色（像素级全 255），Tab 栏本就隐藏、`chrome_hidden` 逻辑无 bug。**真凶在 Android 构建链**——
`./build.sh` 全绿但 APK 里打的是 11-08 的旧 `libleanote_mobile.so`（用户手机上的"底部内容"=
旧版 Tab 栏），三层叠加：

1. **输出路径错误**：根 `build.gradle.kts` 的 `buildRustDebug` workingDir 是 `mobile/android/gradle`，
   `-o mobile/android/gradle/app/src/main/jniLibs` 相对路径解析到嵌套的不存在目录
   （`mobile/android/gradle/mobile/...`，cargo-ndk 自动建目录所以不报错）——新 .so 从未落进
   jniLibs，APK 一直用 cargo-ndk 早期手动跑留下的 11-08 版。
2. **native merge 缓存不失效**：jniLibs 里的 .so 被 cargo 覆盖写（mtime 变、任务指纹不变），
   gradle `mergeDebugNativeLibs`/`stripDebugDebugSymbols` 判 UP-TO-DATE——即使路径修对，
   不清中间产物 APK 仍旧库。
3. **strip 从未生效**：AGP 找不到 NDK（未设 `ndkVersion`）→ "Unable to strip" → 1.4GB 未剥离
   debug .so 进 APK（1.78GB），模拟器 `INSTALL_FAILED_INSUFFICIENT_STORAGE`（静默装不上去，
   设备上还是旧 APK）。另 packageDebug 原地重写 zip 会留 1.5GB 死空间（删产物重打包即消）。

## 实现

### Tab 栏（crates/leanote-mobile/src/app.rs TabShell）

- 旧样式（全宽 sidebar 底色 + 顶缘发丝线 + pb(safe_bottom)）→ **悬浮胶囊**：外层容器
  `pb(safe_bottom + 8) + px_4 + pt_4`（离屏缘悬浮），内层 `h(60) + rounded_full` 胶囊
  （`border_1` + `shadow_lg` + 半透明底色），图标 + 0.65rem 标签、选中 primary / 未选
  muted_foreground 不变。
- 玻璃底色：浅色 `white().opacity(0.92)`（白页上半透明白不可见，近实白磨砂），深色
  `white().opacity(0.10)`（iOS 深色玻璃比背景亮的薄雾）。**ponytail 上限**：gpui-pre 0.3.9
  无 backdrop blur（全 vendor 无 BackdropFilter），磨砂用半透明近似，胶囊在流式布局内、
  内容不从其下滚过；框架支持背景模糊后升级。
- 阅读屏隐藏逻辑（`chrome_hidden` + 内容 wrapper `pb(safe_bottom)`）本就正确，未动。

### Android 构建链（三处成对修复）

1. `build.gradle.kts`（root）：`buildRustDebug/Release` 的 `-o` 改 `app/src/main/jniLibs`
   （workingDir 相对路径），并删掉已生成的嵌套垃圾目录。
2. `build.sh`：gradle 前先 `rm -rf` 三个 native 中间产物目录（merged_native_libs /
   merged_jni_libs / stripped_native_libs），杜绝 UP-TO-DATE 旧库。
3. `app/build.gradle.kts`：`ndkVersion = "28.2.13676358"`——AGP 由此找 llvm-strip，
   1.4GB debug .so 剥到 188MB（APK 1.78GB → 204MB，模拟器可装）。

## 验证

- **Android 模拟器**（leanote-test AVD，重装 204MB 新 APK）：真触摸点笔记行 → 阅读屏底部
  像素带扫描：y1920-2360 纯背景色，仅 2364-2374 系统手势条——Tab 栏完全隐藏（用户所报
  "底部内容"消失）；返回根屏：悬浮白色胶囊 + 选中 Tab 蓝色（vision 复核）；深色模式
  （`cmd uimode night yes`）胶囊 ≈42 vs 页面 19 = white@0.10 公式吻合；深色阅读屏同样无栏。
- **iOS 模拟器**（iPhone 18 Pro）：新构建截图——悬浮胶囊渲染（vision 复核），左右留边、
  底部悬浮于 safe area 之上；阅读屏（深链）底部 460px 纯白无任何 chrome。
- `cargo test -p leanote-mobile --lib` 12/12 绿；host / aarch64-apple-ios /
  aarch64-linux-android 三 target `cargo check` 绿（CI 同款命令）。
- APK 内容哈希 == jniLibs 新 .so 哈希（`unzip -p … | md5sum` 双端比对）。

## 残留

- iOS 模拟器 `simctl ui appearance dark` 后 App 不切深色（prefs 跟随系统路径未生效）——
  既有问题非本次引入（深色玻璃公式已在 Android 深色实测），留待主题 issue。
- llvmpipe 模拟器首次 `screencap` 常黑一帧（surface 尚未 post），等一两帧再截即正常，
  非应用 bug。
