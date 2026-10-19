# 01: core crate — 领域类型 + SQLite 存储 + CLI 离线最小面

**What to build:** 用户在 App 未运行时能用 `leanote add -t 标题 -c 内容 -g 标签`（含 stdin）把一条 Markdown 笔记写进本地存储（笔记本不存在则建「AI札」，写入置 `IsNew/IsDirty` 脏标记），随后 `leanote list` / `leanote show <id前缀>` 能把它读回来。这是所有上层（同步、HTTP、GUI）的地基：领域类型（Note/Notebook/Tag/Account、脏标记、嵌套 `ParentNotebookId`+`Seq`、`IsTrash`/`LocalIsDelete`、`Star` 本地字段）与 SQLite 存储一次定型，字段名沿用 GLOSSARY 语义。

**Blocked by:** None (can start immediately)

**Status:** resolved

- [ ] `leanote add -t -c -g`（含 stdin 管道）写入成功，「AI札」不存在时自动创建
- [ ] 写入的笔记带 `IsNew/IsDirty` 脏标记与 `IsMarkdown:true`
- [ ] `leanote list` / `list -n -g -v -N` / `show <唯一前缀>` 读取结果与 Electron 版 CLI 同参数输出对拍一致
- [ ] ID 前缀唯一匹配定位、歧义报错、无匹配报错语义与现版一致
- [ ] 存储不读写 Electron 版 NeDB 数据目录（新库新路径）
- [ ] 崩溃/中断后重跑不损坏库（事务性写入）

## Comments

- **Resolved by** merge `4c21ec44` on `integration/rust-rewrite` (fast-forward; implementer commit `f1f78a84`).

  - leanote-core SQLite store: WAL + `FULL` sync, transactional writes, 24-hex `Id` strings, dirty flags (`IsNew`/`IsDirty`), auto-creates 「AI札」 notebook on add.
  - leanote-cli `add`/`list`/`show` byte-parity with the JS CLI: `rust/crates/leanote-cli/compare-with-js.sh` — 28 cases MATCH on stdout/stderr/exit code, including stdin piping, notebook/tag/verbose/limit flags, and a check that the Rust store never modified Electron NeDB files.
  - Store path `$HOME/Library/Application Support/leanote-rust/` (`LEANOTE_RUST_DATA` override for tests); Electron NeDB untouched.
  - Crash safety: SIGKILL mid-write replayed cleanly (transactional writes verified).

  **Deviation from checkbox wording:** the ticket said "歧义报错" (ambiguity error) for ID prefix matching, but the actual Electron JS CLI resolves ambiguity by **first-newest-match** — implementer matched the real JS behavior (spec rule: byte parity wins).

- **Verification:** `cargo test --manifest-path rust/Cargo.toml` — 22 passed, 0 failed (leanote-core 9 unit + store tests, leanote-cli 9, fakeserver smoke 4). `bash rust/crates/leanote-cli/compare-with-js.sh` from worktree `../leanote-rust-01` — all 28 byte-comparisons passed.
