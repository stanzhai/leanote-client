# 26: UI 第九轮 — 12 项 UI/UX 改进

**What to build:** 用户 2026-10-26 清单 12 项（grilling 9 问全按推荐裁决）：

1. **Add-note 按钮上下文化**：仅 Notebook 导航显示；Trash 显示清空按钮；Recent/Starred/Tag 无按钮。⌘K QuickNewNote 保留（回退第一个 notebook）。
2. **状态条移位**：从编辑器底部移到标题下方一行。左 = Tag icon + 标签胶囊 + 输入框（flex_1）；右 = Clock icon + 更新时间（flex_none）。下边框分隔；编辑器底边距 5px；仅编辑模式显示（vim 模式段与行:列删除）。
3. **新建笔记聚焦标题**：标题为空 → 聚焦标题输入框（placeholder "请输入笔记名称"）；Tab 从标题跳编辑器。
4. **toggle_star 不 bump UpdatedTime**：SQL `UPDATE notes SET Star=1-Star`，仅 Star。
5. 其他 bump 路径（trash/restore/move/tag-rename）维持现状。
6. **accent_foreground 纯白**：两种模式（kit 默认 neutral-900/50 在中饱和强调色上不可读）。
7. **⌘K 行两行化**：第一行标题截断居左 + `notebook · date` 居右 flex_none；第二行摘要省略号截断；标签不显示；去掉 `.label()` 双渲染。
8. **主题切换修复**：拆两次 `Theme::update`（先 mode config，后 apply_wake_theme）——kit `Theme::edit` 闭包后重放 config 会覆盖闭包内颜色编辑。
9. **Trash 清空**：工具栏清空按钮 + AlertDialog 确认（`EmptyTrashConfirmBody`）；删除阅读区重复按钮。
10. **空态**：FileText icon 居中 + 单行提示（列表空 → "还没有笔记…"；非空 → "选择一条笔记"）；删除双重渲染。
11. **侧栏宽度**：`LEFT_W 180→150`，`LEFT_RANGE (180,400)→(150,400)`。
12. **Recent 徽章**：`min(20, live)`，共享 `RECENT_LIMIT=20` 常量。

**Blocked by:** （无）

**Status:** resolved (2026-10-26)

## Adjudications (2026-10-26)

- Q1 add-note 按钮范围：仅 Notebook 视图（⌘K 保留）
- Q2 状态条内容：tags 左 + 时间右，vim/行列删除
- Q3 标题聚焦：空标题聚焦标题框，Tab 跳编辑器
- Q4/Q5 toggle_star：不 bump UpdatedTime；其他路径不动
- Q6 accent_foreground：两模式纯白
- Q7 ⌘K 行：两行，meta = notebook（无 tags）
- Q8 主题切换：堆叠 update（接受一帧 kit 默认色）
- Q9 清空确认：AlertDialog（列表头部按钮为唯一入口）
- Q10 空态文案：区分"无笔记"与"未选择"
- Q11 侧栏：150/400
- Q12 Recent 徽章：min(20, live) 共享常量

## Comments

## Evidence (2026-10-26)

全部实现；`cargo build -p leanote-app` 零警告（除预存 linker 提示）；`cargo test -p leanote-core -p leanote-app -p leanote-cli` 全绿（63+11+5+10+24+9+8+25）。

1. **toggle_star**（leanote-core/src/lib.rs:444-461）：SQL 只改 Star；测试 `toggle_star_flips_star_without_setting_isdirty` 加 `updated_time` 不变断言。
2. **Add-note 上下文**（app_shell.rs `render_list` ~4031-4092）：`match &self.nav` — Notebook → 新建按钮；Trash → 清空按钮（`confirm_empty_trash`）；Recent/Starred/Tag → 无。
3. **清空确认**（app_shell.rs ~1412-1430）：`window.open_alert_dialog` + `.show_cancel(true).on_ok(empty_trash)`；i18n `EmptyTrashConfirmBody`。
4. **状态条**（app_shell.rs ~4619-4688）：`title_row → status_bar → md_toolbar → body` 顺序；`.h_7().gap_2().px_2().border_b_1()`；col `.pb(px(5.))`。`ModeNormal`/`TagsLabel` 死键删除。
5. **标题聚焦/Tab**（app_shell.rs ~741-783 + ~4732-4747）：`.placeholder(TitlePlaceholder)`；`focus_title = note.title.is_empty()`；capture_key_down 拦截 Tab（标题有焦点 → content.focus）。
6. **accent_foreground**（main.rs ~439-444）：`theme::accent_foreground()` 纯白，两模式。
7. **主题堆叠**（app_shell.rs `set_appearance` 951-979）：两次 `Theme::update`（apply_config → apply_wake_theme）。
8. **⌘K 行**（search_overlay.rs）：`ResultRow.meta → notebook`（纯 notebook 名）；两行渲染 `gpui_kit::div().truncate()`；测试改 `r.notebook` 断言。
9. **空态**（app_shell.rs ~4308-4334）：`flex_1().items_center().justify_center()` v_flex + `AssetIcon::FileText` + `visible_notes().is_empty()` 分流文案。
10. **侧栏**（app_shell.rs 38-42）：`LEFT_W=150`、`LEFT_RANGE=(150,400)`。
11. **Recent 徽章**（app_shell.rs ~2705-2713 + 590）：`RECENT_LIMIT=20` 共享（`visible_notes` 与 `all_count` 两处）。

### GUI smoke (2026-10-26，真机 + 像素测量)

- 应用启动（LEANOTE_FAKE_SERVER=1 种子 12 笔记）正常渲染三栏。
- **侧栏宽度像素实测**（外接 1x 显示器，`np.median` 列剖面）：分栏跳变在 x=151 与 x=396 → 侧栏 150px、列表 240px，与 `LEFT_W`/`MID_W` 常量一致（#11 ✓）。
- 点击笔记行：列表选中态与编辑面同时更新（diff 33673/6380 px）。
- 编辑面标题与正文之间存在独立窄带（状态条带 + 下边框阶梯 27→14，#2 结构 ✓）。
- 交互项（Tab 焦点流、⌘K 两行、AlertDialog、空态文案切换）为运行时行为，GPUI 无 AX 脚本接口，无法自动断言——留给用户过目。

文档：GLOSSARY（收藏不 bump UpdatedTime、Recent 徽章语义、编辑器状态栏词条重写）。
