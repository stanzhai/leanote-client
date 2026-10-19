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
