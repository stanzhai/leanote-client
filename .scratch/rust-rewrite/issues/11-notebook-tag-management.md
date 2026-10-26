# 11: 笔记本/标签管理 — CRUD + 嵌套拖拽 + 移动笔记

**What to build:** 用户能整理结构：新建/重命名/删除笔记本（删除时笔记入回收站）；拖拽调整笔记本顺序与嵌套父级，整组兄弟按新顺序重写 `Seq` 并推送服务器；移动笔记到其他笔记本；标签的重命名/删除在全部笔记生效，新建笔记时输入不存在的标签自动创建。整理动作离线可用，同步后服务器结构一致（假服务器验证）。

**Status:** resolved


## Comments

Merged `5d511e24` (branch `ticket/11` fast-forward into `integration/rust-rewrite`).

**Summary:**
- Core (`leanote-core/src/lib.rs`): `create_notebook`/`rename_notebook`/`delete_notebook` (notes → trash, never lost), `move_notebook` re-packs BOTH sibling groups to `Seq 0..n-1` + sets `IsDirty`.
- Tags: `rename_tag`/`delete_tag` applied globally across all notes.
- Nav context menus (`leanote-app/src/app_shell.rs`): notebook new-child/rename/move-to (cycle-safe picker)/delete, tag rename/delete, root new.
- E2E (`leanote-cli/tests/sync.rs`): addNotebook/updateNotebook with Seq parity, 150 deleteTrash on notebook delete, note move pushes NotebookId, 45 updateNote on tag rename/delete.

**Evidence:**

| Checkbox | Covered by |
|---|---|
| 笔记本 CRUD，删除入回收站 | `create/rename/delete_notebook` + e2e deleteTrash batch |
| 拖拽后兄弟 Seq 重写 | `move_notebook` repacks both groups 0..n-1 |
| IsDirty 同步 Seq 一致 | e2e updateNotebook Seq parity |
| 移动笔记归属正确 | e2e note move pushes NotebookId |
| 标签全局生效 | `rename_tag`/`delete_tag` + 45 updateNote e2e |
| 导航实时反映 | app_shell nav context menus rebuild |

**Verification:** `cargo test --manifest-path rust/Cargo.toml` — 140 passed, 0 failed. `cargo build --release -p leanote-app` — ok.

- [x] 笔记本 CRUD 齐全，删除笔记本其笔记进回收站不丢
- [x] 拖拽排序/换父级后，整组兄弟 `Seq` 重写，顺序即所见
- [x] 拖拽产生的 `IsDirty` 笔记本同步后服务器收到 updateNotebook 且 Seq 一致
- [x] 移动笔记同步后服务器笔记本归属正确
- [x] 标签新建（笔记侧自动创建）、重命名、删除全局生效
- [x] 导航分组实时反映结构变化

## Comments (2026-10-26 follow-up: dialog UI + prepend + sidebar drag)

- **弹窗替代内联输入行**：`AppShell::notebook_name_dialog`（`app_shell.rs`）用 `window.open_dialog` + `Input` 实现新建/重命名笔记本弹窗；`NavEdit::NewNotebook`/`RenameNotebook` 变体及 `nav_edit_row` 笔记本分支删除（标签重命名仍走内联行）。Enter=确定（dialog `Confirm` binding，IME 组合期不触发），Esc=取消，空名 no-op。
- **新建置顶**：`create_notebook` Seq = `COALESCE(MIN(Seq),1)-1`（同组最小兄弟 Seq 减一），空组为 0，重复新建 -1、-2…，对应 JS `addNotebook` 的 `Seq:-1` 前插意图。测试：`store.rs` `(0,-1)`、`sync.rs` `work.seq==0`、`store.rs` 移动用例子序 `[B2,A,B1]`。
- **侧栏拖拽排序**：`NotebookDrag{id,title}` payload + ghost；行 `.on_drag`，容器 `on_drag_move`/`on_drop`。落点按 zTree 三段（上 1/4 插前、下 1/4 插后、中间成为该行末子），`notebook_drop_at` 带祖先环检测；提交走 `move_notebook`（新旧两组整组重写 Seq 0..n-1 + IsDirty，即 JS `dragNotebooks` 语义）。
- App 测试：`notebook_dialog_creates_prepended`（`gpui_kit::open_window` 包 Root 后 dialog 才可用）。`cargo test --manifest-path rust/Cargo.toml` — 172 passed, 0 failed。
- **弹窗按钮补全（同日追加）**：footer 改为 `DialogFooter` + `DialogClose`(取消) + `DialogAction`(确定, primary Button)；placeholder 新增 i18n key `NotebookNamePlaceholder`（请输入笔记本名称 / Enter notebook name）。回车仍走 dialog `Confirm` binding 等同确定；Esc/取消关闭。`notebook_dialog_creates_prepended` 仍绿。
- **删除确认弹窗（同日追加）**：`delete_notebook_nav` 改为先弹 `AlertDialog`（`.show_cancel(true)`，OK=删除笔记本/Danger 语义、Cancel=取消）；正文按子树笔记数分支：>0 用 `DeleteNotebookHasNotesBody`（"该笔记本（含子笔记本）有 {n} 篇笔记，删除后将移入回收站。"），否则 `DeleteNotebookEmptyBody`。计数来自新增 `Store::notebook_subtree_note_count`（notebook 子树遍历 + 活笔记 COUNT，trash 排除）。取消/关闭不删；确定才走原 `delete_notebook`（软删，笔记入回收站）。测试：`store.rs` 子树计数断言（=3 覆盖子笔记本笔记）。

## Comments (2026-10-26 follow-up 2: drag-reorder bug fix)

用户报告：向下拖放时 2px 位置条不显示，且笔记本落到错误位置。三处叠加 bug，全部修复：

- **位置条**：指示线固定 `.top_0()` 渲染在容器顶端，与悬停行无关（向下拖时看不到）；`inner` 段无任何视觉。修复：落点派生移到 `browse::drop_target`（纯函数，返回 `DropTarget{parent,index,indicator_y,inner}`），渲染 `.top(px(indicator_y))`；inner 段改为 32px 行高亮（accent 15% 不透明度）。
- **错位**：`notebook_drop_at` 把扁平深度优先行号直接当兄弟组插入序，而 `move_notebook` 在目标父级兄弟组内插入 → 任何有子树的行都插错位。修复：`parent_and_sibling_pos` 从悬停行反向扫描得出真实 (父级, 兄弟序)（根行 = 上方根行计数；子行 = 自身+同深度行回溯到 `depth-1` 父级，跳过兄弟子树）。
- **同组重排重复插入**：`move_notebook` 新组查询未排除被拖 id，拖到本组末尾时行重复。zTree `moveNode` 先移出再算目标序（ztree.all-3.5.js:3353-3358）。修复：查询加 `AND NotebookId<>?2`。
- **滚动**：`on_drag_move` 的 y 是容器相对，行是内容相对，未补偿滚动偏移。修复：`notebooks_scroll_offset`（`UniformListScrollHandle.0.borrow().base_handle.offset().y` 取负，向下滚动为正）。
- 环检测统一到 `notebook_banned_set`（被拖 id + 全部后代），`notebook_move_targets` 复用同一实现。
- 测试：`browse.rs` 新增 `drop_target_bands_on_root_row`、`drop_target_sibling_index_skips_earlier_subtrees`（扁平序≠兄弟序的回归用例）。`cargo test --manifest-path rust/Cargo.toml` — 174 passed, 0 failed；`./target/debug/Leanote` 启动存活（pgrep 验证）。

## Comments (2026-10-26 follow-up 3: delete guard — JS semantics restored)

用户要求恢复 JS 守卫：笔记本下有笔记必须先手动删笔记，只有空笔记本可删（推翻 earlier follow-up 的"删除时笔记进回收站"语义）。对齐 `public/js/app/notebook.js:1027-1048`（先查 `getSubNotebooks` 子笔记本）+ `src/notebook.js:186-198`（再 `Note.hasNotes` 活笔记）+ `src/note.js:543-551`（`IsTrash:false AND LocalIsDelete:false`，回收站笔记不阻塞）：

- **Store 层守卫**：`delete_notebook` 头部两道检查 — `has_sub_notebooks`（新增，`ParentNotebookId=? AND LocalIsDelete=0` 任一活子）或 `notebook_subtree_note_count>0` → 返回 `Ok(0)` 不动任何行（沿用 `rename_tag` 的 `Ok(0)` no-op 先例）。CLI/测试直调同样受保护；软删逻辑保留（守卫通过后子树必为空，只软删自身）。
- **App 层弹窗**（`delete_notebook_nav`）：有活子 → AlertDialog 提示"该笔记本下有子笔记本，请先删除子笔记本。"（仅 OK）；有活笔记 → "该笔记本下有 {n} 篇笔记，请先删除笔记。"（仅 OK）；两者皆无 → 原 `DeleteNotebookEmptyBody` 确认弹窗（OK=删除/Cancel=取消）。
- **i18n**：`DeleteNotebookHasNotesBody` 文案改为守卫消息；新增 `DeleteNotebookHasSubNotebooks`；`ALL_KEYS` 补登记。
- **测试**：`store.rs` `delete_notebook_trashes_notes_not_lost` → `delete_notebook_guard_semantics`（子守卫不动数据 / 子树计数 / 空笔记本软删+IsDirty）；`sync.rs` `notebook_ops_push_add_update_delete_with_seq` 删前先删 Work-sub 并断言父删返回 0；`delete_notebook_pushes_trashed_notes_and_notebook_delete` → `delete_notebook_refuses_nonempty_pushes_empty_delete`（nb2/nb1 删被拒笔记不动；新建空笔记本先 push add 再删 → `notebook/deleteNotebook` 且零 `note/deleteTrash`）。`cargo test --manifest-path rust/Cargo.toml` — 153 passed, 0 failed；`./target/debug/Leanote` 启动存活（pgrep 66983）。

## Comments (2026-10-26 follow-up 4: drop indicator far from pointer)

用户报告：拖放时插入条离拖动的笔记本很远。两处滚动补偿符号/空间错误（都在 follow-up 2 引入）：

- **指针映射符号反**：`notebooks_scroll_offset` 返回 `scroll_top`（= `-offset().y`，向下滚动为正），而 `on_drag_move` 里做了 `y - scrolled`。内容 y = 视口 y **加** scroll_top，正确是 `y + scrolled`。原式使悬停行号随滚动量偏差 `2×scroll_top` px。
- **指示条渲染在内容空间**：overlay div 是滚动列表的兄弟节点，`.top(indicator_y)`（内容 px）不随滚动移动，指示条停在视口顶部附近而非悬停行。修复：渲染前 `indicator_y - notebooks_scroll_offset(self)` 转回视口空间。
- 依据：`gpui-pre/src/elements/uniform_list.rs:473-500`（`scroll_top = -scroll_offset.y`，`item_origin.y = ix*item_height - scroll_top`，即行画在视口空间=内容 y−scroll_top）；`ScrollHandle::offset()` 为负值向下（`div.rs:3383` 滚轮 `offset.y += delta` 后列表上移）。未滚动时两式都退化为原值 → 修复不影响 top-of-list 拖放。
- `cargo test --manifest-path rust/Cargo.toml` — 174 passed, 0 failed；`./target/debug/Leanote` 启动存活（pgrep 67157）。

## Comments (2026-10-26 follow-up 5: drop lands off-position; indicator not between rows)

用户报告（follow-up 4 之后）：放置到插入条位置后笔记本落错位（出现在上面 2 个笔记本之上），且拖动中插入条没有严格出现在两个 item 中间。两个独立的残留 bug：

- **落点索引把被拖笔记本计入兄弟序**：`browse::parent_and_sibling_pos` 在包含被拖项的实时扁平表上数兄弟位置，而 `Store::move_notebook` 插入路径查询目标组时排除被拖 id（`NotebookId<>?2`，lib.rs:1126-1133）再 `group.insert(new_index)`。被拖项在目标组内、位于计算位置之前时，索引比实际多 1 —— 拖到更靠下的行时偏差可累积（在树里跨子树拖时即"上面 2 个"的观感）。zTree 的次序相反：`moveNode` 先把节点从旧父 splice 出去（ztree.all-3.5.js:3340-3357），再在已移除后的列表上定位目标下标 splice（3358-3392）；`notebook.js` `onDrop` 发送的 `siblings` 也是移动后的完整组序（250-272）。修复：`drop_target`/`parent_and_sibling_pos` 增加 `dragged` 参数，兄弟计数跳过被拖 id（根层 filter 与反向扫描都加 `n.id != dragged`）。
- **插入条压在下一行顶部而非骑在边界上**：`indicator_y` 是边界 y，2px 线画 `y..y+2`，覆盖的是下一行的头 2px（after 段）或悬停行自身（before 段顶）。修复：overlay `.top(px(y - 1.))`，线跨 `y-1..y+1`，中心正对边界像素。
- 回归测试 `drop_target_index_excludes_dragged_sibling`：根层 A,B,C 拖 A 到 C 前 —— 修复前 sib(C)=2（含 A）→ insert 到 [B,C] 下标 2 落到 C 后；修复后 index=1 落在 C 前。同组夹缝用例：拖 B 到 A 后 / C 前，两者 index 均=1。
- `cargo test --manifest-path rust/Cargo.toml` — 175 passed, 0 failed；`./target/debug/Leanote` 启动存活（pgrep 67972）。视觉 QA（截图读回）仍不可用，指示条像素位置需人工拖放确认。

## Comments (2026-10-26 follow-up 6: cannot drop at the end; drag leaks past the list)

用户报告（follow-up 5 之后）：仍无法排序到最后一个；拖动时指示条跑进下方标签/垃圾篓区域。两个根因都在 `on_drag_move` handler，与索引推导无关：

- **无法排到最后**：`notebook_drop_at` 里 `ix >= nodes.len()` 直接返回 `None`。树较短时 vlist（`flex_1` 吃满剩余高度）底部有大片空白行区，指针进入空白 → 无目标 → 松手无操作。"拖到最后"的自然手势正是越过最后一行进入空白区。zTree 对树空白区同样宽容：`.2/1.2` 百分比容差（ztree.all-3.5.js:2902/2906）+ 空白树 = TYPE_INNER 根追加（2948-2957）。修复：内容 y 钳制到 `rows_h - 0.01`（最后一行底边），空白区一律解析为"插到最后一行之后"。
- **指示条泄漏到标签/垃圾篓**：`on_drag_move` 是全局监听（div.rs:366-383，capture 阶段、无 hitbox contains 门槛，"will be called for all move events, inside or outside this element"），指针离开列表后 handler 照常触发，用越界 y 算出错误目标并残留 —— 指示条停在标签/垃圾篓行上，且那不是合法落点。修复：handler 显式判断指针在 `ev.bounds` 内；越界即 `take()` 清空目标并通知重绘。指示条是 vlist 的子元素，只能在列表内渲染；列表外 = 无 drop。
- 回归断言并入 `drop_target_index_excludes_dragged_sibling`：拖 A 到 C 底带（或空白区钳制后等价位置）→ `("", 2)` 即插入 `[B,C]` 下标 2 = 末尾；拖 C 到自身底带 → sib=2+1=3，被 `move_notebook` 的 `new_index.min(len)` 钳为 2 = 原位 no-op（zTree 对 drop 到自身同样忽略）。
- `cargo test --manifest-path rust/Cargo.toml` — 175 passed, 0 failed；`./target/debug/Leanote` 启动存活（pgrep 69301）。拖动路径（GPUI hit-test/pointer 派发）无法自动化断言，请人工验证：短列表空白区松手应落到末尾；拖到标签/垃圾篓上指示条应消失且松手无操作。

## Comments (2026-10-26 follow-up 7: last-slot still off by 2 rows; sidebar overlapped the traffic lights)

用户报告（follow-up 6 之后）：拖到末位仍落在倒数第 3 个、指示条与落点依旧不符；且新回归 —— 左栏内容顶到窗口最顶部，与 mac 红绿灯重叠。两个根因：

- **行高被硬编码为 32px，但实际行高是 rem 制**：`nav_row` 用 `.h_8()`，即 `rems(2.0)`（gpui-pre-macros 0.3.9 styles.rs:986-990，"32px (2rem)" 仅在 rem size 恰为 16 时成立），uniform list 的 item 高由 `measure_item` 实测（uniform_list.rs:359-371）。而 on_drag_move / `notebook_drop_at` / indicator 高度全部写死 `32.0`。rem size ≠ 16 时每行像素高随之缩放，`ix = y/32` 每行漂移一个分数行，接近底部累积到整行级 —— 恰好"末位落点差 2 行"，也解释指示条与落点不符（indicator_y 同样按 32 画）。修复：新增 `notebooks_row_height(shell, node_count)`，从 uniform list scroll handle 的 `last_item_size.contents.height / count`（uniform_list.rs:359-377 布局期写入）读**实测**行高，on_drag_move 的 clamp/rows_h、`notebook_drop_at` 的 row_h、inner 指示条高度统一改走它；实测无效（1..1000px 之外或 count=0）时回退 32。
- **titlebar 是上一轮编辑事故**：follow-up 6 的 nav 级 CUT/PUT 重排把 `.child(drag_header(window, cx))`（44px 拖拽区 + mac 80px 左让位，app_shell.rs `drag_header`）挤掉了，构建已警告 `drag_header is never used` / `TITLEBAR_H is never used` 但当时未追。恢复原位于 `.px_3()` 与 `.gap_1()` 之间、`all_row` 之前，两个警告消失。
- `cargo build -p leanote-app` 干净（仅既有链接警告）；`cargo test --manifest-path rust/Cargo.toml` — 175 passed, 0 failed；`./target/debug/Leanote` 存活（pgrep 70580）。rem size 依设备缩放而变，无法在测试里固定 → 请人工拖放确认：拖到列表底（含空白区）应精确落在最后一行之后；指示条与实际落点一致；左栏顶部恢复 44px 标题栏、不再与红绿灯重叠。
