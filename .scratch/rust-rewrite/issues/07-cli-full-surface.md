# 07: CLI 完整面 — search/edit/help + 双模探测 + 垫片切换

**What to build:** 命令行用户的全部既有习惯在 Rust 版成立：`search <关键词>`、`edit <id前缀>`（-t/-c/-g/-n 与 stdin）、`help`；子命令探测本地 HTTP（短超时）失败则自动落离线直写模式；首启把 `~/bin/leanote` 垫片指向 Rust 二进制（原 Electron 版垫片被替换）。所有子命令两种模式下输出与 Electron 版对拍一致。

**Blocked by:** 04, 05

**Status:** resolved

- [ ] `search` 结果与摘录与现版对拍一致（走票 05 统一搜索核心）
- [ ] `edit` 在 HTTP 与离线两种模式下行为与现版一致（离线编辑置脏，App 启动后同步推送）
- [ ] HTTP 探测超时（App 未运行）自动切直写，无感
- [ ] `help` 文本与现版一致
- [ ] 首启后 `~/bin/leanote` 指向 Rust 二进制，`which leanote` 与执行结果正确
- [ ] 子进程级测试覆盖双模式全部子命令（含 stdin、ID 前缀歧义）

## Comments

- **Merged:** `ticket/07` (tip `ce9d8e96`) fast-forwarded into `integration/rust-rewrite`; resolved by commit `e1c1248b`.
- **Summary:** search/edit/help reach byte-parity with the JS CLI — 56/56 harness cases match (stdout+stderr+exit code), incl. stdin edits and ambiguous ID-prefix cases. Dual-mode probe hits `/api/ping` with a 300ms timeout, honoring `LEANOTE_PORT` so tests never disturb the user's live app on 8912. Shim switch is one-shot (marker file), guarded against overwriting non-leanote forms, with `LEANOTE_NO_SHIM_SWITCH` opt-out. Fixed `Store::update_note` dropping `NotebookId` moves. `tests/full_surface.rs` adds 8 tests incl. a real `serve` subprocess exercising dual-mode.
- **Also fixed here:** flaky `mcp.rs` test — `spawn()` derived temp dirs from `(pid, nanos)`, and same-process concurrent tests could land on the same clock tick → two `Store::open`s on one SQLite file → `SQLITE_BUSY`. Replaced with a per-spawn atomic counter; 12/12 repeat runs green.
- **Evidence:** `cargo test --manifest-path rust/Cargo.toml` → 61 passed, 0 failed across the workspace (leanote-cli 22 = cli 9 + full_surface 8 + sync 5; core 19; http 16; fake-server 4). `bash rust/crates/leanote-cli/compare-with-js.sh` → `ALL BYTE-COMPARISONS PASSED` (last line of script output).
