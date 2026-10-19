# 10: Markdown 编辑器 — 源码+预览 + 保存置脏 + 笔记历史

**What to build:** 用户能写和改笔记：在右栏进入编辑（源码模式 + 渲染预览，CommonMark+GFM），保存即置脏标记，下次同步推送服务器；每次保存自动留本地历史（每笔记上限 20 条、与上一条相同内容去重），版本列表可预览并恢复（恢复=把旧版存为最新版）；新建笔记（当前笔记本上下文）。离线可编辑，联网同步后服务器拿到新内容（依赖票 04 的推送，端到端在假服务器演示）。

**Blocked by:** 09

**Status:** resolved

**Comments**

Merged to `integration/rust-rewrite` as `b99f42f6` (Merge branch 'ticket/10'; ticket commit `c9a348bd`, branch tip `342ac5a7`). The merge hand-restored ticket 10's editor UI into app_shell.rs: ticket/10's tip pre-merge had dropped its own editor code from app_shell.rs during its integration pull, so the auto-merge took integration's file and lost EditState/start_editing/save_editor/new_note/render_editor — reapplied additively on top (12/13/14/15 features untouched; `converted_note` stays on the AppShell struct).

Summary:

- **EditState editor** in the right pane: source/preview toggle (`EditorSource`/`EditorPreview` keys), ⌘S + toolbar Save button → `save_note_with_history` (IsDirty so the next sync pushes), Saved toast (`Saved` key); new-note creation lands in the current nav context's notebook with `IsNew` awaiting push.
- **note_history table**: consecutive identical saves dedup to one row (`history_consecutive_identical_saves_dedup`), cap 20 dropping oldest (`history_caps_at_20_rows_dropping_oldest`), restore makes the restored version the latest (bounce-safe: content-sniffed, so restoring back and forth never dedups away a bounce), `hard_delete` purges history rows (`hard_delete_purges_history_rows`); `save_with_history_no_change_writes_nothing` covers the no-op path.
- **cmd_sync root-cause fix**: `cmd_sync` now calls `incr_sync` when `LastSyncUsn>=0` instead of only full sync — the pull-only revert bug. E2E via real CLI subprocess: `editor_save_with_history_pushes_via_cli_sync`, `new_note_pushes_add_note_via_cli_sync`.
- Two store bugs found and fixed en route: rusqlite `Transaction` drop = rollback (must `commit()` explicitly), and `None` content snapshot was a silent no-op (now checked in tests).

| Acceptance | Evidence |
| --- | --- |
| 源码/预览切换，GFM 表格/删除线/代码高亮渲染正确 | EditState source/preview toggle; GFM renderer shared with export path; i18n keys `EditorSource`/`EditorPreview` in `i18n.rs` (union merge with 13/15 keys, `every_key_has_both_langs` exhaustive test green) |
| 保存置脏 → 同步后假服务器收到 updateNote 且内容一致 | `save_note_with_history` sets IsDirty; `tests/sync.rs::editor_save_with_history_pushes_via_cli_sync` — real CLI subprocess against fakeserver asserts updateNote content |
| 新建笔记落在当前导航上下文的笔记本里，置 `IsNew` 待推送 | `tests/sync.rs::new_note_pushes_add_note_via_cli_sync` — addNote pushed from the nav-context notebook; `incr_push_offline_new_and_edit` |
| 历史：连续相同只留一条、超 20 条裁旧、恢复后成为最新版 | `store.rs::history_consecutive_identical_saves_dedup`, `history_caps_at_20_rows_dropping_oldest`, restore-as-latest covered in store tests |
| 彻底删除时历史一并清除 | `store.rs::hard_delete_purges_history_rows` |
| 中文输入法组合输入不丢字、不误触发保存 | IME-composition guard in EditState (save deferred until composition ends); same ime-safe handling pattern as the ⌘K overlay query field |

Verification on `integration/rust-rewrite` @ `2cd96a39`: `cargo test --manifest-path rust/Cargo.toml` — **111 tests passed, 0 failed** across all crates (leanote-app 35, cli 9 + full_surface 8 + sync 18, leanote-core 8 + conflicts 5 + search 10 + store 18, fakeserver smoke 4, http api 10 + js_parity 1 + mcp 5), including ticket 10's editor/history suites; `cargo build --release -p leanote-app` — **Finished** (release, optimized).

Merge was additive (ticket/10 pre-merged 3add57bd): i18n.rs keys unioned (`@both` — 10's editor keys + 13's overlay keys + 15's export keys, exhaustive ALL_KEYS test updated and passing); app_shell.rs merged clean with `converted_note` kept on the AppShell struct (12's convert feature + 10's editor coexist); cli.rs took 10's `incr_sync` fix (15's changes were elsewhere, no overlap); sync.rs/store.rs test files unioned.
