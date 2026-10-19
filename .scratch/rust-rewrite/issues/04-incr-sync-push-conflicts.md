# 04: 增量同步 + 脏推送 + ID 映射 + 冲突置非脏语义

**What to build:** 用户离线/本地写入的内容（脏笔记）在同步时逐一推送服务器（本地新建的条目建立 `ServerNotebookId/ServerNoteId` 映射、清脏），同时拉取服务器增量；服务器在同步中途推进 USN 时自动再轮直至收敛；服务器端删除与本地修改冲突时按现版语义置非脏不删（不静默丢数据）。内容冲突记录进冲突列表，供后续冲突条 UI（票 14）呈现。对假服务器全场景验收。

**Blocked by:** 03

**Status:** resolved

- [ ] 离线新建/修改的笔记同步后服务端可见、本地清脏、建立服务端 ID 映射
- [ ] 离线删除（回收站→彻底删）同步后服务器 `deleteTrash` 收到对应请求
- [ ] 服务器 USN 中途推进 → 自动再来一轮直至收敛（回环保护）
- [ ] 服务器返回 `Msg=='conflict'` 的删除 → 置非脏不删，数据保留
- [ ] 内容不一致冲突 → 记入冲突列表（含双方版本信息），不静默覆盖
- [ ] 增量拉取只取 USN delta，重复同步无重复写入

## Comments

- **Merged:** `ticket/04` (tip `06aa1d2f`) fast-forwarded into `integration/rust-rewrite` — branch already contained integration tip `1fee8c4b` (via Finish04's `06aa1d2f` merge), so no conflicts arose; work commit `2895a049` (`04: incr sync + dirty push + conflicts`), closed by the commit adding this comment.
- **Summary:** `Api::incr_sync` runs a ≤5-round loop with `check_need_incr_sync_again` semantics (`last+1==usn` → advance, `last>usn` → ignore, else re-round); re-rounds pull but never double-push. Dirty push walks add/update/delete series, writes `ServerNotebookId`/`ServerNoteId` mappings and clears dirty flags; offline hard deletes reach the server as `deleteTrash`. Delete conflict (`Msg=='conflict'`) → row set not-dirty and kept, never silently dropped. Content conflict → both versions stored as `sync_conflicts` rows (new SCHEMA table) for ticket 14's UI, no overwrite. Incremental pull is USN-delta only; reruns write no duplicates.
- **Checkbox → test** (`rust/crates/leanote-cli/tests/sync.rs`):

  | Checkbox | Test |
  |---|---|
  | 离线新建/修改推送、清脏、ID 映射 | `incr_push_offline_new_and_edit` |
  | 离线彻底删除 → 服务器 `deleteTrash` | `incr_push_hard_delete_sends_delete_trash` |
  | USN 中途推进 → 自动再轮至收敛（回环保护） | `incr_usn_advance_re_rounds_to_convergence` |
  | 删除返回 conflict → 置非脏不删 | `incr_delete_conflict_sets_not_dirty_keeps_row` |
  | 内容冲突 → 记入冲突列表不覆盖 | `incr_content_conflict_recorded_not_overwritten` |
  | 增量只取 USN delta，重复同步无重复写入 | `incr_pull_delta_only_no_duplicates_on_rerun` |

- **Evidence (post-merge, integration):** `cargo test --manifest-path rust/Cargo.toml` → 75 passed, 0 failed across the workspace (core 19 = store 9 + search 10; cli 20 = cli 9 + sync 11; fake-server 4; http 16 = api 10 + js_parity 1 + mcp 5; app 11; rest 0). `cargo build --release -p leanote-app` → `Finished release profile [optimized]`.
