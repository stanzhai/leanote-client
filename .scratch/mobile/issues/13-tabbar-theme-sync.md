# Issue 13: Tab 栏重样式 + 主题配置对齐桌面（ADR-0014 修订）

状态:已完成(2026-11-05)

## 需求

- 底部 Tab 栏按移动端常见风格重做（原样式：选中项整格涂 primary 底色，丑）。
- 设置页主题配置同步桌面最新实现（单 `ui_theme` 族干 + 强调色跟随/覆盖），移动端当时还停在 ADR-0014 修订前的双字段模型。

## 实现

### Tab 栏（app.rs TabShell）

- iOS 惯例：图标（lucide，kit `Icon`）+ 小字号标签（0.65rem），选中 primary、未选 muted_foreground，顶缘发丝线，高 56 + safe bottom。
- 坑 1：`theme().accent` 是浅蓝容器色（`accent.background #e0f2fe`），做 tint 会近隐形——选中 tint 用 `theme().primary`。
- 坑 2：iOS 嵌入式 Application 没有 asset source，`svg()` 图标全部不渲染。vendored `gpui-pre-mobile/src/ios/ffi.rs` 加 `set_asset_source`（OnceLock 存 `Arc<dyn AssetSource>`，run_app 里 `with_assets` 应用，镜像 set_app_callback 模式）；iOS 入口注册 `gpui_kit::assets::AllAssets`，Android 入口链上 `.with_assets(...)`。

### 主题配置（prefs.rs / theme.rs / settings_tab.rs / i18n.rs）

- prefs：`light_theme`/`dark_theme` 双字段 → 单 `ui_theme`（"" = Navop）+ `accent`（"" = 跟随）；load 时旧格式 `light_theme` 去 " Light" 后缀迁移（桌面同款规则）。
- theme.rs：`resolve()` 桌面解析链（精确名且模式匹配 → "{trunk} Light/Dark" 变体 → **按名回落 "Navop Light/Dark"**——kit `default_light_theme()` 是内置 shadcn 默认而非 Navop，选中 tint 会变近黑）；`theme_trunks()` 成对主题合并族干；`ACCENTS` 8 色板 + `apply_accent_override`（Navop 配方：hover lighten 0.1 / active darken 0.1 / selection 25%，前景亮度阈值 0.55，写 colors+button tokens，必须在 `Theme::change` 之后——模式切换重载配置会覆盖）。
- 设置页：「浅色主题/深色主题」两行 → 单「主题」循环行 + 「强调色」行（彩虹轮 = 跟随，8 纯色圆片，选中白勾，轮子画法照抄桌面 app_shell）。

## 验证

- 模拟器（iPhone 18 Pro, iOS 27.2）截图 + 像素分析：
  - 笔记 Tab：选中 icon+label 蓝 (#3b82f6 Navop primary)，未选灰 (#6b7280)，顶缘细线；图标正常渲染（像素级确认 326 蓝像素，修正前 0）。
  - 设置页：主题行 "Navop"、强调色行 9 圆片（彩虹轮带白勾）。
  - 端到端：直写 prefs（Gruvbox + dark + orange）→ Gruvbox Dark 暗棕 UI、Tab 橙色、按钮琥珀——族干解析、变体匹配、accent 覆盖全链路真值。
- `cargo check -p leanote-mobile`：host / aarch64-apple-ios / aarch64-linux-android 全绿（桌面 leanote-app 未动）。

## 残留

- Android 真机视觉走查（图标资产已接线，未跑 APK）。
- 主题下拉在移动端是循环 chip（桌面为下拉弹层）——长列表点多次，后续可换 bottom sheet。

## Comments

### 2026-11-05 第二轮（用户反馈三项）

- 主题改下拉：循环 chip → kit `Select`（`SearchableVec<SharedString>`，`SelectState` 首帧惰性建——需要 Window，trunk 列表启动后不变；`SelectEvent::Confirm` → prefs）。
- Tab 栏图标越上边线：根因是固定 `h(56)` 把 `pb(safe_bottom)` 计入，内容区只剩 ~22px、图标上下溢出。改内容驱动高度（item `py(8)`，bar 无固定 h），像素级验证：边线下 0 个越线像素。
- 阅读屏隐藏 Tab 栏：`NotesTab::is_reader()` + TabShell 渲染时条件裁掉 bar（内容 wrapper 补 `pb(safe_bottom)`）；`attach_notes_tab` 里 `cx.observe(&notes)` 让 push/pop 联动 shell 重渲染。搜索结果跳阅读屏同路径生效。
- 验证：设置页下拉（Navop + 折叠箭头截图）、bar 包裹（像素）、`reader:n1` 深链无栏截图；host/iOS/Android 三 target check 绿。

### 2026-11-05 第三轮（搜索输入 + Tab 栏居中）

- 搜索框无法输入——根因（vendored `gpui-pre-mobile/src/ios/window.rs`）：`IosWindow` 未实现 `PlatformWindow::text_input_state_changed`（Android 同文件有桥），聚焦后 UIKit 键盘永不弹出。补桥：FocusGained → `show_keyboard_with_type`（deferred becomeFirstResponder），FocusLost → `hide_keyboard`。
- 逐层定位（临时插桩，已全撤）：tap→MouseDown 合成 ✓（201,96）→ `window.focus()` 被调 ✓ → 但 input handle 未聚焦——测试坐标打在框底缘、被 GPUI div 自动聚焦（div.rs track_focus mousedown 自动抢焦）的 Root 拿走。改打字段中心后全链路通。
- 端到端验证（临时 XCUITest，已撤）：注入点击 + `typeText("rust")` → 日志 `text_input_state_changed FocusGained` / `handlers=1` / `handle_input focused=true`，截图字段含 "rust"、搜索结果实时更新为两篇 rust 笔记。
- Tab 栏居中：cell 改固定 `h(52)+justify_center`（此前 py(8) 顶部仅 8pt、视觉偏上、底部空白大）。像素验证：三 tab 字形块顶距 21-24px / 底距 27-28px（±2pt 内居中）。
- host / iOS / Android 三 target check 绿。

### 2026-11-06 第四轮（用户反馈：强调色不可点/不生效/非圆片）

- 根因一（不生效）：`theme::apply()` 每次都 `ThemeRegistry::global_mut` 重复装载主题——kit 的 `observe_global::<ThemeRegistry>` 在 flush 期按当前名 `Theme::change` 重载配置，把刚写入的 accent 覆盖整批抹掉（update 内同步看是好的，flush 后打回原形）。修复：注册拆成幂等 `register()`（已含 "Navop Light" 即跳过，不触发 observer），apply 不再动注册表；强调色写入走 `cx.defer` 重放，排在 observer 之后执行（首次注册场景），后续偏好变更为纯同步路径。
- 根因二（方块 + 勾不渲染）：圆片实现嵌套了内层 `div().size_full().bg()`——gpui 内容裁剪不裁圆角，内层 bg 渲染成 18×18 直角方块。照抄桌面 app_shell 结构：纯色片 bg 直接涂在 `rounded_full()` 自身、选中勾 Icon 直接作子元素（前景按亮度取黑/白）；彩虹轮保留 canvas + 白勾浮层。
- 桌面 c0110a96 对齐：跟随模式 `colors.accent/accent_foreground/link` 无条件取活动主题 primary（文件 `accent.background` 是浅容器色）；覆盖分支补 `list_active_border/drag_border`；两分支均 `tokens = (&colors).into()` 双写过 reconcile。
- 主题列表：`theme_trunks()` 原从 `registry.themes()`（HashMap）取序——每次启动乱序。改为解析 BUNDLED 文件序（桌面 `choices()` 同款）：Navop 对合并显示 **"Default"**，单模主题（Matrix/Tokyo Moon…）原名列出；设置页旧值 "Navop" 归一显示。
- 验证：host `cargo test -p leanote-mobile --lib` 3 测（文件序/跟随-覆盖-恢复/圆片点击-清除）全绿；iOS/Android target check 绿。模拟器（iPhone 18 Pro）：存 prefs dark+Catppuccin Mocha+red → 启动即红勾红 tint（Tab/芯片/按钮）；临时 XCUITest 真触摸点 blue → prefs 落盘、重启蓝勾蓝 tint；清 prefs 冷启 → 下拉 "Default"、彩虹勾、Navop 浅色。圆片像素级 corner_drop≈20（圆）对比修复前 0（方）。
- 临时 XCUITest 与 UITests target 已删。

### 2026-11-06 第五轮（笔记本切换抽屉）

- 需求：原来切笔记本要滚到「最近」列表底部点「浏览笔记本」——入口太深。改为「最近」栏左侧 ☰（lucide Menu）开左侧抽屉：280px 面板列出笔记本（子级缩进），160ms ease-in-out 滑入，点笔记本直达其笔记列表，点遮罩关闭；栏右侧 ⟳（RefreshCw）保留手动刷新。
- 两个坑：
  - 遮罩 Normal 命中盒不拦截点击——一次 tap 穿透叠加的每一层（笔记本行+遮罩+底下的笔记行全触发，表现为莫名进阅读页）。遮罩 `.occlude()`（BlockMouse）+ 面板 `.block_mouse_except_scroll()` 解决；absolute 遮罩必须显式 `top_0().left_0()`，否则停在流式位置（屏外）。
  - `attach_notes_tab` 在同步完成后重复挂载会换新 NotesTab 实体，把浏览栈/抽屉状态整屏重置——加已挂载守卫。
- 验证：host `drawer_toggle_opens_and_closes`（开/遮罩关/尺寸）+ 3 旧测全绿；三 target check 绿。模拟器 XCUITest 真触摸：tap ☰ → 抽屉开（Inbox/Work/Research 缩进、遮罩变暗像素级确认）→ tap Inbox → 「返回 Inbox」笔记列表截图确认；临时 UITests target 已删。
