# Issue 39 — Windows 霞鹜文楷不生效 + 窗口位置记忆完善

- 状态：done
- 日期：2026-11-01
- 模块：`rust/crates/leanote-app/src/{fonts,main,window_state,pdf,app_shell,i18n,theme,prefs}.rs`
- 版本：3.1.7

## 现象（用户反馈）

1. Windows 平台霞鹜文楷等宽字体不生效（怀疑安装位置问题）。
2. 要求记住窗口位置：下次启动从最后一次的位置和大小展示；无记录时默认主屏幕居中；多显示器下上次所在显示器已不存在时恢复主屏幕居中。

## 根因

1. `fonts.rs::install_path()` 硬编码 `~/Library/Fonts`：macOS 专属路径。Windows 不扫描任何 HOME 相对字体目录，且其用户字体机制要求 ttf 落 `%LOCALAPPDATA%\Microsoft\Windows\Fonts` **并**在 HKCU `Software\Microsoft\Windows NT\CurrentVersion\Fonts` 写注册表项——两者都没做，字体从未被系统/后台枚举到。另 macOS 路径要求重启（CoreText 进程启动时扫描字体目录）。
2. 窗口几何只在 `on_window_should_close` 保存：⌘Q（`cx.quit()`）、AppleScript quit、崩溃、注销都不走该钩子（冒烟实测：优雅退出未写 window-state.json）。显示器消失回退用 `displays.first()`，顺序无主屏保证；最大化状态未持久化（`if let Windowed` 丢弃 Maximized 分支）。

## 修复

1. **字体改进程内注册**：ttf 下载一次进数据目录（`LXGWWenKaiMono-Regular.ttf`，`.part` 临时文件 + 魔数校验 + rename 保持不变），启动时 `cx.text_system().add_fonts()` 注册（DirectWrite 内存字体引用 / CoreText CGDataProvider / cosmic-text，三平台同构 API）。缓存命中时开窗前注册（零闪烁）；缺失时后台 `smol::unblock` 下载，成功后回主线程注册 + `refresh_windows()`。`InstallOutcome` 枚册删除（无消费者）；设置页提示仅剩下载中/失败两态，`FontRestartHint` i18n 键删除；`pdf.rs` Typst `FontStore` 头部注入同一 ttf（`Font::new` + `f.info().clone()`，先于系统字体，用户字体在 PDF 回退中优先）。
2. **几何持久化**：提取 `save_window_state(window, dir)`（含 Windows 设备像素 fixup + `maximized` 判定 `!matches!(Windowed)`），`on_window_should_close` 与新 `observe_window_bounds` 观察者（移动/resize 均触发，`Cell<Instant>` 防抖 500ms）共用。`WindowState` 增 `maximized: bool`（`#[serde(default)]` 兼容旧文件）；启动 `WindowBounds::Maximized(bounds)` 恢复最大化。显示器列表构建时主屏排首（`sort_by_key(|b| *b != primary_bounds)`），`clamped()` 回退居中落点即主屏。顺带删除 `start_window_drag` 遗留调试探针写入（硬编码 Temp 路径）。

## 验证

- `cargo test -p leanote-app`：104/104（新增 `pre_maximized_field_file_still_loads`、`clamped_keeps_maximized_flag`）。
- GUI 冒烟（mac，`LEANOTE_DATA=/tmp`）：首启后台下载 25.9MB ttf 落数据目录、无报错；二启缓存注册路径运行正常；AppleScript 移动窗口后 window-state.json 落盘 `{x:116,y:108,w:1280,h:801,maximized:false}` —— 防抖保存链路端到端打通；优雅退出不写文件（证实 ⌘Q 缺口由观察者覆盖）。
- Windows 交叉检查：本机 `cargo check --target x86_64-pc-windows-msvc` 被 `ring`/`gpui-pre` 构建脚本（需 Windows 工具链）阻塞；Windows 侧代码与既有编译通过的 close 钩子同型（`fixup_engaged`/`bounds_to_device`/`scale_factor` 同签名），CI release 构建兜底。
