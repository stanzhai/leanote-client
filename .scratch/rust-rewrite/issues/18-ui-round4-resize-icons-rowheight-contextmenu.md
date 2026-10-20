# 18: UI 第四轮 — 三栏可拖拽调宽（持久化）/ icon 修复 / 行高统一 / 笔记行右键菜单

**What to build:** 用户四项需求（2026-10-20，无 grilling，直接按惯例实现）：

1. **三栏左右拖拽调节宽度，记住上次大小**：左右两根分隔条可拖；宽度持久化到 prefs.json，下次启动恢复。
2. **修复界面 icon 显示**：app 从未注册 AssetSource，所有 `Icon`/`svg().path("icons/…")` 加载失败 → 图标全空。
3. **中栏笔记行行高统一**：标题最多 2 行自动换行 + 省略；meta 单行截断；所有行固定同一高度。
4. **笔记行右键菜单**：删除笔记（trash 软删，同 web UI 语义）+ 移动笔记到笔记本（列表区切换为选择器）。

**Blocked by:** （无 — 建立在 08/09/17 之上）

**Status:** resolved

- [x] 三栏可拖拽 + 持久化 — render body 换 `gpui_kit::component::h_resizable("main-panes")`，三个 Phase 分支均用 `resizable_panel()` 包（left `.size(prefs 或 224).size_range(180..400)`，mid `.size(prefs 或 336).size_range(240..600)`，末 panel 吃剩余）；`on_resize` 回调把 `state.sizes()` [0]/[1] 写入 `prefs.pane_widths` 落盘（拖拽 mouse-up 才触发一次，panel.rs:485-498）；render_nav/render_list 去掉内部固定宽改 `w_full().h_full()`，宽度归 panel 管。实测（真实 release + fake server）：合成拖拽左分隔条 180→240pt 后 `prefs.json` 记 `{"left":240,"mid":240}`（mid 被 range 收到 240 下限），**杀进程重启后帧边界 240/480pt 精确恢复**（/tmp/l18_drag3.png、/tmp/l18_restore.png）。
- [x] icon 修复 — 根因：`gpui_kit::application()` 不注册 AssetSource，`Application::with_platform` 用空默认源 → `svg().path("icons/…")` 全部加载失败。`main.rs` `.with_assets(gpui_kit::assets::Assets)`（default icon bundle，build.rs 由 default-icons.txt 生成，覆盖 component 层 IconName 全部路径）。实测帧对比（/tmp/l17_nav.png 修复前 vs /tmp/l18_shell.png 修复后）：左栏导航区亮色字形像素 8075→14831（+84%），底部三件套区 627→2696（4.3×）— 图标实际渲染。
- [x] 行高统一 — `note_list_row` 重写：`v_flex().h(px(NOTE_ROW_H=74)).px_2().py_2().gap_1().justify_between()`；标题行 `.flex_1().min_w_0().line_clamp(2)`（14px/19px 行高），星标 `flex_none()` 不抢标题宽；meta 行 `.truncate()` 单行。实测帧：选中行 accent 带 447–520 = 74px 整（/tmp/l18_click.png），常规行 pitch 全部 148 物理像素（=74pt）无变异（/tmp/l18_shell.png 扫描 10 行带）。
- [x] 右键菜单（删除 + 移动） — `note_list_row` 挂 `.context_menu(...)` → 自由函数 `note_row_menu(menu, shell, id, lang)` 两个 `PopupMenuItem`：`MoveNoteTo`（置 `NavEdit::MoveNote { id }`）+ `DeleteNote`（`this.delete_note(id, cx)`）。`delete_note` = `store.trash_note`（IsTrash=1+LocalIsDelete=1+IsDirty=1，同步推回服务器，leanote-core lib.rs:660）+ 选中则清空 + `invalidate_lists`；`move_note_to` = `store.set_notebook`（NotebookId UPDATE + IsDirty=1）+ `nav_edit=None` + 失效刷新。MoveNote picker 在 render_list：`nav_edit` 命中时中栏整列换为「标题 + notebook_nav_nodes() 全节点列表（按 depth 缩进）+ 取消」，点目标调 `move_note_to`。实测（窗口解锁后合成鼠标）：右键行出菜单 → 点第一项 → 中栏换 picker（header 带下 11 行文本带 pitch 124px 规则排列，无笔记行）→ 点目标行 → 中栏回笔记列表带选中行（/tmp/l18_picker.png、/tmp/l18_moved.png）— 移动链路端到端走通。i18n 两 key（`MoveNoteTo`/`DeleteNote`）中英齐全，`ALL_KEYS` 穷举测试覆盖。
- [x] `cargo check -p leanote-app` 0 error 0 warning；`cargo test -p leanote-app --bin leanote-app` 43 绿（prefs round_trip 含 pane_widths 新旧兼容）；workspace `cargo test` 18 套件全绿；`cargo build --release -p leanote-app` 成功。
- [x] 假服务器真实跑 + 截帧验证 — `LEANOTE_FAKE_SERVER=1 LEANOTE_FAKE_AUTOSUBMIT=1` release 跑通（126 notebooks/1047 notes 真实语料），三栏背景 (33/42/10) 材质正确，全部证据帧在 /tmp/l18_*.png。

**Notes:**
- 恢复机制走 `.size(saved)` 首帧 flex_basis（会话内由 use_keyed_state 的 ResizableState 接管），不用启动后 `resize_panel` 编程式恢复 — 简单且无 settling frame。
- 删除未加确认弹窗（与 nav 的 delete_notebook_nav 惯例一致）；若需要可参照 logout 的 Alert 模式。
- 移动目标列全部 notebook（不排除当前所在）：目标是「笔记本」非层级父，无环风险。
