# 24 — UI round 8: login link, sync progress, nav bottom pin, ⌘-, accent/theme swap, status bar

Status: resolved
Reported: 2026-10-23 (user, 6 items)

## 1. Login 卡残留注册链接

**根因**: `render_login` 仍渲染指向 leanote.com 注册页的 Link；桌面端无账号注册流程，纯装饰且误导。
**修复**: 删掉链接行；i18n 键 `Key::Register` 替换为 `Key::SyncingContents`（"正在拉取笔记内容" / "Downloading note contents"）供同步屏使用，`ALL_KEYS` 测试表同步（`i18n.rs:220`）。
**Evidence**: 登录卡 Sign In 按钮（y≈1223-1278px）与窗口底（y1794px）之间零内容像素。

## 2. 左栏笔记本/标签全折叠时废纸篓不贴底

**根因**: nav v_flex 内 notebook/tag 两组折叠后无 flex_1 填充元素，trash 行+footer 被内容流顶到中间。
**修复**: trash_row 前插常驻 spacer `gpui_kit::div().id("nav-spacer").flex_1().min_h_0()`（`app_shell.rs` ~2824）。
**Evidence**: 两种状态像素验证——两组全关（prefs `notebooks_open:false, tags_open:false`）与一组开，footer 均为最后内容带且最后一行内容 = 窗口底行（dark: 1900-1963/1963；light: 1950-1963/1963）。组开启时 vlist 与 spacer 分摊剩余空间，trash+footer 仍贴底。

## 3. 切 accent 后 kit 菜单/hover 色不同步

**根因（两处）**: (a) boot 顺序——`Theme::update(apply_wake_theme)` 在 `theme::set_accent(&prefs.accent)` 之前执行，首帧 accent 用裸 kit 色（`main.rs` 原 293-297 顺序）；(b) 切换不回灌——`set_accent` 只调 `theme::set_accent(id)`（改 app 侧色值函数），kit 组件（menu_item hover 读 `tokens.accent`、ghost 按钮 hover）不重刷。
**修复**: (a) `main.rs:293-297` 调换为先 `set_accent` 再 `apply_wake_theme`；(b) `set_accent`（`app_shell.rs:983-997`）末尾 `Theme::update(cx, |t| apply_wake_theme(t, &palette))` 回灌 kit 主题。
**Evidence**: 真机：设置对话框内点绿色 accent swatch → 整窗蓝 accent 像素 2691→39，绿色 accent 像素 2333 出现在 app chrome 与打开中的 kit 对话框（单次重绘，无重启）。menu hover 走同一 `tokens.accent` 管线。

## 4. 主题切换出现中间错色帧

**根因**: `set_appearance` 用 `Theme::change`（先以裸 kit 色刷一帧，再第二次 refresh 应用 app 调色板），两帧之间可见默认色。
**修复**: `set_appearance`（`app_shell.rs` ~918-972）删 `Theme::change`，改单个 `Theme::update` 闭包：闭包外取 `appearance`/`sys_mode`（cx 被 move），闭包内 match mode → clone `dark_theme`/`light_theme` 的 `Rc<ThemeConfig>` → `apply_config` → `apply_wake_theme`。一次 `refresh_windows`。
**Evidence**: 真机连拍（14 帧 @~0.04s）系统外观 Light→Dark 与 Dark→Light：全程仅两个稳态色（nav bg 27 / 内容 18），nav 行结构自第 1 帧起完整，无裸 kit 默认色/白闪帧，饱和像素数稳定（~530，无 accent 默认蓝爆量）。

## 5. ⌘, 不能呼出设置

**根因**: 只绑了 `ctrl-,`，macOS 用户按 ⌘,。
**修复**: `main.rs:265` 增 `KeyBinding::new("cmd-", search_overlay::OpenSettings, None)`（action 处理器已存在）。
**Evidence**: 真机 `keystroke "," using command down` → 全屏 overlay + 设置对话框渲染（vs 基线 diff 3.9M px）。

## 6. 编辑态状态栏贴边 + r:c 多余括号

**根因**: 状态栏 h_flex 无垂直 margin（border_t 贴窗口底，内容贴 border）；`format!("{mode}  {}:{}")` 处历史版本曾有孤立 `")"`。
**修复**: `app_shell.rs:4311-4312` 加 `.mt(px(5.)).mb(px(5.))`；格式串确认为 `"{mode}  {}:{}"`（4353），无括号。
**Evidence**: 真机编辑态（markdown 笔记自动进编辑）：border 线 y1900 → 文本带 1921-1941 → 窗口底 1963，文本上下各 ~21/22px 对称间距（5pt×2 retina ✓）；r:c 簇后无孤立的 `)` 字形 glyph run。

## 回归修复（2026-10-24 用户报告）

**症状**: markdown 编辑器/预览选中文本的蒙层透明，看不出选中了什么；深色主题正常。复现路径：切换主题 → 修改强调色。

**根因**: #4 修复里 `apply_wake_theme`（main.rs）加了 `colors.selection = gpui_kit::white()`（实心白）。该函数在 boot、每次主题切换（`set_appearance`）、**每次改强调色**（#3 加的 `set_accent` 回灌）都会重跑。Light 模式下白色蒙层叠在浅色底材上 = 白上白，视觉上"选中消失"；Dark 模式白蒙层反而可见，所以"深色正常"。用户复现的时序（先切主题再改强调色）正好是把 `apply_wake_theme` 带进 light 会话的两条路径。

**修复**: 删除覆盖行，selection 回落 kit 默认半透明 wash（light `0x1d4ed8@0.3`、dark `0x55a0fc@0.3`），两模式均可见。

**回归测试**: `selection_tests::selection_stays_translucent_in_both_modes`（main.rs）——对 Light/Dark 各构造 Theme 跑 `apply_wake_theme`，断言 selection 必须半透明且 light 模式不能是实白。已验证：加回 `white()` 行时测试失败，删除后通过。

**验证**: `cargo test -p leanote-app -p leanote-core -p leanote-fakeserver` 全绿（58+11+5+10+24）。真机 dark 模式编辑器拖选出现 `rgb(34,40,47)` 半透明 wash 带。


## 回归修复（2026-10-24 晚，用户报告布局 3 项：手风琴/首登宽度/状态条见 #7）

- **左栏手风琴互斥失效**（展开任一组不再独占填充到底部）: trash 前的永久 `nav-spacer`（`flex_1`）与展开组的 vlist（`flex_1`）平分剩余高度，把 trash+footer 顶到中部。修复：spacer 仅在两组都折叠时渲染（app_shell.rs :2828-2835）。用户判断正确——该 spacer 是改垃圾篓 icon（Issue 19）时引入的回归。
- **首登（无历史 prefs）左/中栏过宽**: 首跑默认 `LEFT_W=224/MID_W=336`，超出 drag range 下限直觉。修复：改为 `LEFT_W=180/MID_W=240`（与 `LEFT_RANGE.0/MID_RANGE.0` 对齐，app_shell.rs :37-38）。已登录用户 prefs 落盘宽度不受影响。

**验证**: `cargo build -p leanote-app` + 三包测试全绿；smoke 启动正常。

## 回归修复（2026-10-24 下午，用户报告 3 项；2026-10-24 晚二次修复 #7/#8/#9）

### 7. 编辑态状态条与底部"贴近，不要有间距"

**一轮口径（作废）**: #6 先按 5px、再按"严格 5px"调整 mt/mb。用户最终口径是零间距。
**真根因**: 状态条上方 `mt(px(5.))` 是唯一的间距来源（父 col 已 `.pb_0()`、`.mb_0()` 已设）。同列上方 `gap_2()` 只作用于兄弟元素之间，不作用于最后贴底元素。
**修复**: 删 `mt(px(5.))`（app_shell.rs 状态条，原 :4320）。状态条 border_t 直接接编辑区，底边贴窗口。
**验证**: 编译通过；间距来源只有显式 mt/mb/gap，全部已归零（源码推理，无像素验证手段）。

### 8. 缩放（100%–160%）"根本不起作用"

**一轮结论（作废）**: 把 `notify()` 换成 `refresh_windows()`。方向错了——问题不在刷新，在 rem 基准从未改到。
**真根因（两层）**:
1. kit `Root` 的 `RootPlugin::prepare` **每帧**执行 `window.set_rem_size(cx.theme().font_size)`（gpui-component-0.7.3 root.rs:440）。app 侧任何 `window.set_rem_size`（boot 恢复 + set_zoom）在下一帧布局前就被覆盖回 `theme.font_size`（恒 14px）。zoom 从未生效过。
2. `apply_wake_theme`（main.rs）硬编码 `theme.font_size = px(14.)`，每次主题/accent/palette replay 都把 font_size 重置。
**修复**: zoom 全部改走 `theme.font_size`：`apply_wake_theme(theme, palette, zoom)` 设 `theme.font_size = px(14. * zoom)`；`set_zoom` 重写为一次 `Theme::update`（尾部自带 `sync_base` + `refresh_windows`，当帧生效；Root prepare 下一帧自动把新值刷进 window rem）；boot zoom 并入 main.rs 的首个 `Theme::update`；删除 app_shell.rs 的 boot `set_rem_size` 死代码。`window_min_size` 仍按 zoom 缩放（px 量纲，不走 rem，不受 prepare 影响）。
**为什么成立**: kit 所有 spacing/字号 helper（`px_3/gap_1/h_7/text_sm`…，gpui-pre-macros styles.rs suffix 表）全部 rem 基——`theme.font_size` 一变，整套 UI 同帧缩放。
**验证**: 编译 + 全测试通过；smoke 启动正常。无像素验证手段。

### 9. 主题切换"依然卡顿，要等一段时间"

**一轮结论（部分作废）**: 调整 `set_appearance` 内顺序 + 依赖 observer 二次 replay 兜底。该路径本身当帧没错，但漏了渲染侧的 stale 数据源。
**真根因（两条）**:
1. **app 侧背景读的是过时的 `window.appearance()`**: panes/弹层的 `dark()` helper 读 window 缓存字段，该字段只在 AppKit `view_did_change_effective_appearance` → **deferred** 回调（gpui-pre-macos window.rs:3532 → gpui-pre window.rs:2909）之后才更新。所以 kit 控件（读全局 Theme）当帧变色，而 app 自绘的材质背景（`theme::material(Material::*, dark)`）晚一拍才跟——肉眼即"卡顿/闪一下才变"。
2. **固定 Light/Dark 下 observer 的冗余 replay**: `NSApplication.setAppearance` 同样触发该 deferred 回调 → observer 再跑一次 `sync_system_appearance`（= `Theme::change` reload，更重）+ `apply_wake_theme`。结果正确但晚一拍，与首帧路径重叠。
**修复**:
1. `AppShell::dark(cx)` 改读 `cx.theme().mode.is_dark()`（Theme mode 在 `Theme::update` 内当帧翻转），所有 5 个调用点（render、settings、history、attachments、search overlay）改传 cx。`App::window_appearance()`（直接读 `NSApp.effectiveAppearance`，同步）仍用于 boot/observer 的模式判定，不受影响。
2. observer（main.rs）加 Follow-only guard：闭包内 `Prefs::load`，`appearance != Follow` 直接 return——固定模式下系统不会变，只有 Follow 需要跟随系统 replay。
**验证**: 编译 + 全测试通过；smoke（fake server + autosubmit）启动无 panic。点击当帧变色的像素级确认无手段（此前显示器睡眠），由机制推理覆盖：Theme mode 翻转与 `refresh_windows` 同在 `Theme::update` 尾部，同帧。

### 环境事故记录

- 验证用的实例 pid 78512 实际以**真实数据目录**启动（无 `LEANOTE_RUST_DATA`），测试点击写入了真实 `~/Library/Application Support/leanote-rust/prefs.json`（appearance/zoom/accent/vim_default）。已手工恢复为 Follow/1.0/""/false，其余字段（recents/pane_widths/nav）为应用自身合法写入，保留。数据（1194 notes）未受影响。
- 后续 smoke 实例反复秒退的原因：后台进程随宿主命令结束被回收 + 显示器 13:17 起睡眠锁定（电池供电、displaysleep=2、无真实 HID 唤醒），非代码问题。下次真机验证需长持有会话或改用 `caffeinate` + 真实输入设备。

## 附注

- 验证环境: `LEANOTE_RUST_DATA=/tmp/leanote-smoke LEANOTE_FAKE_SERVER=1`（5 notebooks/12 notes/5 tags）。
- `cargo test -p leanote-app -p leanote-core -p leanote-fakeserver` 全绿（58+11+5+10+24，含新增 selection 回归测试）。
- kit（gpui-kit 0.7.3）只读，全部修复在 app 侧。
