# 03: 服务器客户端 + 登录 + 全量同步

**What to build:** 用户执行 dev 子命令 `leanote sync`（或后续 GUI 登录）后，客户端用 Email/密码登录 leanote 服务器（Token 落库），从 USN −1 按 notebooks→notes→tags→content 顺序分块拉齐全量数据进本地存储，落 `LastSyncUsn`。对假服务器验收：拉齐后库内笔记/笔记本/标签数与假服务器数据集一致；中断后重跑幂等不重不漏。线协议逐字段兼容（token query 参数、PascalCase 载荷）。

**Blocked by:** 01, 02

**Status:** resolved

- [ ] 登录成功获取并持久化 Token；错误凭证给出与现版同等错误
- [ ] 全量同步后笔记本/笔记/标签/笔记内容数量与假服务器数据集一致
- [ ] `LastSyncUsn` 落库；重跑全量同步幂等（无重复行、无内容丢失）
- [ ] 同步中断（进程杀）后重跑恢复到一致状态
- [ ] 同一账号在 Electron 版与 Rust 版间互换使用，服务器侧无感知差异（协议层验证）

## Comments

- Merged to `integration/rust-rewrite` as `ae13f88d` (ticket tip already contained integration `a017d0cd`; fast-forward, no conflicts).
- Summary: Api over ureq (token as query parameter, PascalCase payloads); login form posts email/pwd, token persisted. Full sync from USN −1 in order notebooks→notes→tags→note contents, fetched in 200-item chunks; `LastSyncUsn` persisted last so a crash converges on rerun. Server-deleted semantics: notebooks/notes dropped, tags flagged `LocalIsDelete` per JS behavior. CLI: `leanote sync --host --email --password`.
- Verification: `cargo test --manifest-path rust/Cargo.toml` — 48 passed, 0 failed (incl. `rust/crates/leanote-cli/tests/sync.rs` vs fakeserver: login ok/bad credentials, 450-note multi-chunk parity with server dataset, idempotent rerun, SIGKILL crash recovery).
