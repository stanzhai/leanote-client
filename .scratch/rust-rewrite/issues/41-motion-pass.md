# 41: 动效 pass——hover/选中过渡、同步骨架屏

**What to build:** 基于 gpui-kit 官方动效 API（`motion::transition` / `Skeleton`）改善整体手感：左栏与笔记列表行 hover、选中态过渡、登录全量同步期骨架屏。范围经 grilling 裁定：仅此几处 + 统一动效参数；弹窗/面板动效不在本轮。第二/三轮（用户反馈）：切换笔记右栏淡入与列表滑动选中药丸均撤销——右栏与列表选中恢复瞬时切换，仅保留 hover 过渡与同步骨架屏。

**Change**（`theme.rs`、`app_shell.rs`）:
- `theme.rs`：新增 `MOTION_MS = 120` 与 `motion()`（`motion::Transition` + `ease_out_cubic`）——动效单一参数源。
- 行 hover/选中：每行一个 `motion::transition` bg 通道（键 `("nav-<id>"|"note-<id>", "bg")`）；选中 = accent、hover = `cx.theme().list_hover`（Navop 主题已接）、空闲 = hover 色 alpha 0（插值全程同色系，不穿透明黑）。hover 状态经 `on_hover` 写回 `AppShell.hover_nav/hover_note`（行闭包逐帧重建，状态必须住 shell，同 `notebook_drag_target` 先例）；槽语义收敛在 `AppShell::hover_slot`：进入占槽、他行过期 leave 不清当前槽。
- ~~切换笔记右栏淡入~~（第二轮撤销，用户反馈闪烁不丝滑）；~~列表滑动选中药丸~~（第三轮撤销，用户裁定仅移除笔记切换动画后整体回退，最终仅保留 hover/骨架）。右栏 `render_reader` 直接渲染。
- `Phase::Syncing`：左/中栏由 `visible(false)` 改为真三栏壳 + kit `Skeleton`（nav：固定 pill + 分组头 stub + 树 pill；列表：标题/摘要/节选三线，65px 行距），右栏保留原进度卡。唯一真实等待 = 登录全量同步；本地 SQLite 读瞬时完成，列表切换不做骨架（永不显示的死代码，用户裁定 Q3）。
- 减少动效：kit `init` 已读系统「减弱动态效果」偏好且全部动效自动静态化——零代码、无手动开关（用户裁定 Q5）。

**Verification:** `cargo test -p leanote-app` 102 绿（含 `hover_slot_enter_claims_and_leave_frees_only_owner`、`syncing_phase_renders_skeleton_panes`——`debug_bounds` 断言同步期骨架栏可见且并排）；`cargo fmt --check` 干净。视觉确认（120ms 手感、骨架屏观感）留给用户。

**Blocked by:** (无)

**Status:** resolved (2026-11-11)
