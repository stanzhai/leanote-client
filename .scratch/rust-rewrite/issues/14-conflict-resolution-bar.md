# 14: 冲突条 — 本地 vs 服务器人工裁决

**What to build:** 同步产生内容冲突时（票 04 的冲突列表），用户在右栏顶部看到冲突条：本地版本与服务器版本的摘要并列，三个动作——用本地（推送本地）、用服务器（拉取覆盖本地）、两边保留（服务器版存回原笔记、本地版另存新笔记）。不弹独立窗口，裁决后冲突条消失、冲突列表清项。假服务器制造冲突场景端到端验收。

**Blocked by:** 04, 09

**Status:** resolved

- [x] 假服务器注入冲突后，冲突条出现在对应笔记的右栏顶部
- [x] 用本地：本地内容推送成功、冲突清除
- [x] 用服务器：本地被服务器版覆盖、本地脏标记清除、冲突清除
- [x] 两边保留：原笔记取服务器版、本地版成为新笔记（置 IsNew 待推送）
- [x] 同笔记多轮冲突不累积重复冲突条
- [x] 无冲突时冲突条不出现，同步完成状态可见（沿用票 08 的同步状态区）

## Comments

Resolved by ticket/14 `f1193190`, merged `a3c50f89`.

- **Core** (`leanote-core/src/sync.rs`): `list_conflicts()` typed rows (both versions' title/content/updated), `resolve_conflict_use_local` (conflict row deleted, note stays dirty → next sync force-pushes), `resolve_conflict_use_server` (server JSON overwrites Title/Content/Desc/Abstract, IsDirty/LocalIsNew/LocalIsDelete=0, Usn from server), `resolve_conflict_keep_both(server_note_id, suffix)` (original takes server version clean; local content becomes new note IsNew+IsDirty, ServerNoteId NULL → addNote; suffixed title; returns new Id). Rows DELETEd on resolve, keyed on ServerNoteId → no accumulation. No schema change.
- **App** (`leanote-app/src/conflict_bar.rs`, additive hook in `app_shell.rs::render_reader`): bordered card at TOP of reader (also with no selection — falls back to oldest conflict), accent title 检测到同步冲突, local/server columns (title + 80-char excerpt + date), three buttons 用本地(primary)/用服务器(outline)/两边保留(ghost) → `AppShell::resolve_conflict`. zh/en via 6 new i18n keys.
- **Tests**: core `tests/conflicts.rs` (5), cli `tests/sync.rs` conflict e2e via fakeserver `inject_response({"Ok":false,"Msg":"conflict"})` (4: no-accumulate, use-local pushes updateNote with local edit, use-server overwrites + zero updateNote after, keep-both pushes addNote “Note 42（本地副本）”）， conflict_bar unit (6 incl. no-conflict-no-bar, CJK-safe excerpt).
- **Verification**: worktree 78 tests green; merged integration `cargo test --manifest-path rust/Cargo.toml` → **108 passed, 0 failed**. Smoke screenshot `conflict-bar.png` (bar verified rendering at reader top via pixel-scan of accent title at expected coords; app driven with pre-seeded conflict db, own PID only, user's 8912 untouched).
