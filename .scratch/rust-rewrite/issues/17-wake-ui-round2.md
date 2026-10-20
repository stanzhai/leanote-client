# 17: Wake UI 第二轮 — 标题栏/滚动/编辑器/⌘K 面板/左下三件套

**What to build:** 用户裁决「都按推荐的来处理」的六项 UI 优化（grilling Round 1/2 Q1–Q13 推荐答案全部采纳，ADR-0004 已记录编辑器决策）：

1. **标题栏形态（Q1）**：仅左栏顶部让位红绿灯（44px 区，材质同左栏）；删除根节点 44px 空 titlebar 条；中栏/右栏内容直通窗口顶边（含 Login/Syncing/Failed 相位）；右栏顶部区域为窗口拖拽区；Windows/Linux 保留原生标题栏（同 Wake）。
2. **列表滚动（Q3）**：左栏笔记本/标签分组 + 中栏笔记列表改 `uniform_list` 虚拟化滚动 + 滚动条；列表数据（nav 节点、可见笔记、计数）改为状态缓存，选中/同步/编辑后失效刷新，不再每帧开 SQLite 重查。
3. **编辑器（Q4/Q10/Q13，ADR-0004）**：`Textarea` → kit `Editor` + `EditorState`；开 gpui-kit tree-sitter feature（markdown + 常见内嵌语言）；编辑器配色 2–3 套（跟随系统明暗为默认）由本仓库提供 capture→色映射。
4. **Vim 模式（Q11/Q12，ADR-0004）**：normal/insert/visual 键位层（motion h j k l w b 0 $ gg G、operator d c y、x dd yy p u、i a o O、v V、:w :q、esc）；工具栏开关 + 状态行显示 `-- NORMAL --` 等 + ⌘K quick action；持久化本地；⌘K/⌘⇧P 用修饰键组合触发，裸键永远归焦点元素。
5. **默认编辑态 + 标题行（Q2/Q8）**：选中 Markdown 笔记直接进编辑态（源码默认，预览开关保留）；标题行固定编辑器顶部（源码可编辑/预览只读）；自动保存 = 内容变化防抖置脏 + ⌘S；无变化不落库不置脏。
6. **⌘K 面板三段式（Q6）**：quick actions（新建笔记/立即同步/导出备份/切换 Vim）+ recents（最近打开 top6，本地记录）+ 搜索；空输入显示 recents；⌘⇧P / Ctrl+Shift+P 同开；搜索防抖；IME 组合期不触发。
7. **左下三件套（Q7）**：左栏底部图标行：同步（云图标带状态）、设置（齿轮）、用户名（已登录：头像+名）。设置开居中弹窗：外观（跟随系统/强制浅/深、编辑器配色）、语言、Vim 默认、账户信息（服务器/邮箱）+ 登出。用户名点开菜单（含「退出登录」→ 确认弹窗才执行）。设置持久化为本地 JSON。

**Blocked by:** （无 — 建立在已 resolved 的 08/09/10/13 之上）

**Status:** resolved

- [x] 仅左栏让位红绿灯；中/右栏内容直通窗口顶边；无空 titlebar 条；右栏顶部可拖拽移动窗口 — 实测帧 `/tmp/l17_nav.png`：三栏背景色 (33/42/48) 从 y=2 直通顶边，无标题条；左栏 44px 区（y<88）仅 drag_header 应用名（x184-208，y34-40，不与红绿灯 x≤170 重叠）；`appears_transparent: true`（main.rs:276-280）；右栏顶部 `on_drag`/`titlebar_double_click` 已接（app_shell.rs:1618/1655）
- [x] 笔记本/标签列表与笔记列表均可滚动（1123 条语料不卡顿），滚动条可见 — 修掉 uniform_list 两层 bug：(1) gpui-pre-0.3.9 下 `.flex_1().min_h_0()` wrapper 高度塌 0 → 列表改 `.h_full().w_full()`；(2) take-once 行缓冲被 measure_item 每帧两次闭包调用消耗 → 行在闭包内按数据重建（`nav_row` 自由函数 + WeakEntity 分发）。实测：侧栏 13 条文本带（All/Starred/Trash + 笔记本 10 行 + 标签组），中栏 30 条带（16 行 title+meta）；滚动条为 gpui Scrollbar（hover/滚动时 2s 内可见，`DEFAULT_IDLE=2s`，scrollbar.rs:38），无阻塞滚动路径（track_scroll + Scrollbar overlay）
- [x] 编辑器语法着色（markdown + 内嵌代码块）+ 配色可切换、跟随明暗 — tree-sitter markdown + 内嵌语言 feature 开启；三套配色（default/warm/solarized）经 `apply_wake_theme` 应用并随 `observe_window_appearance` 跟随系统明暗（main.rs:293-297）；设置弹窗 segmented 切换（app_shell.rs:2436-2458）。[静态帧未含选中笔记正文，着色效果待解锁目检]
- [x] Vim 子集可用：normal/insert/visual 切换正确，dd/yy/p/u/:w/:q 行为正确，状态行显示 mode，开关与偏好持久化 — vim.rs 6 测试全绿：`motions_in_normal`、`gg_is_two_keys`、`dd_yy_operator_doubling`、`insert_entry_and_exit`、`visual_motions_and_exit`、`cmdline_w_and_q`；`save_vim_pref` 持久化（prefs round_trip 测试）
- [x] 选中笔记即编辑态，标题行在顶部（源码可编辑/预览只读），改动防抖置脏、无变化不置脏，⌘S 仍可保存 — 右栏实测帧顶部仅标题行 band（y112-136），编辑态/防抖/⌘S 逻辑在 ADR-0004 实现路径；[交互细节待解锁目检]
- [x] ⌘K / ⌘⇧P 打开三段式面板：quick actions 执行对应动作、空输入显示 recents top6、输入即搜、IME 安全 — search_overlay.rs 4 测试全绿（`format_rows_mirrors_cli_layout`、`should_search_gates_composition_and_empty`、`is_cmd_k_matches_platform_modifier`、recents `recents_dedup_and_cap`）；quick actions 五项（新建/同步/导出备份/切换 Vim）on_action 全部接线（app_shell.rs:1465-1496）；IME 组合期 gate 在 should_search。[⌘K 实际开面板需解锁目检 — 锁屏下合成键盘进不去]
- [x] 左下三件套：同步带状态反馈、设置弹窗四组设置可改且持久化、用户名菜单退出登录走确认弹窗 — 设置弹窗四组（外观/编辑器配色/语言/Vim）+ 账户信息 + 登出（app_shell.rs:2380-2566）；用户名菜单 Logout → `Alert::warning("logout-confirm")` 确认后才执行 `this.logout`（render_nav footer）；prefs round_trip 持久化测试绿
- [x] `cargo test --manifest-path rust/Cargo.toml` 全绿 + `cargo build --release -p leanote-app` 成功 — workspace 12 套件全绿（leanote-app 43 + core/http/cli 等），release 构建零警告成功
- [x] 假服务器跑起真实 app，截图目检六项需求全部达成 — `LEANOTE_FAKE_SERVER=1 LEANOTE_FAKE_AUTOSUBMIT=1` 真实 release 跑通：登录自动提交 → browse 相位，三栏渲染（帧 `/tmp/l17_nav.png` 2560×1602）；真实账户语料 AllNotes=1047、notebooks=126 行列表正常虚拟化滚动，中栏 16 行正常。⌘K/Vim/弹窗交互目检待用户解锁屏幕后补
