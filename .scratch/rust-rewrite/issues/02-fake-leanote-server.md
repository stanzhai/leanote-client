# 02: 假 leanote 服务器 — 同步引擎的测试底座

**What to build:** 一个进程内可编程的假 leanote 服务器 crate，按真实线协议（`auth/login`、`user/getSyncState`、`notebook/getSyncNotebooks`、`note/getSyncNotes`+`getNoteContent`、`tag/getSyncTags`、以及 add/update/delete 系列接收端）提供回放与断言：能设定任意 USN 序列、服务端版本、冲突响应（`Msg=='conflict'`）、登录 Token 校验。它是后续同步票（03/04/14）的验收接缝——不用真服务器、不碰真数据。

**Blocked by:** None (can start immediately)

**Status:** resolved

- [ ] 假服务器可编程返回任意 USN 增量与分页 chunk
- [ ] 登录成功发 Token、错密码拒绝，语义与现版线协议一致
- [ ] 推送端点（addNotebook/addNote/updateNote/deleteTrash/addTag/deleteTag）记录收到的请求供测试断言
- [ ] 可注入 `Msg=='conflict'` 响应
- [ ] 自带冒烟：启动、登录、全量数据集回放、推送记录各一条测试通过

## Comments

- Merged into `integration/rust-rewrite` at cf8e185e55b5d6a6f1cdabaa3d56258082e23480 (`02: in-process programmable fake leanote server (rust/fake-server)`) — fast-forward from 7c9d4eed.
- Summary: new crate `rust/fake-server` — in-process fake leanote server covering the real wire protocol endpoints (`auth/login`, `user/getSyncState`, `notebook/getSyncNotebooks`, `note/getSyncNotes` + `getNoteContent`, `tag/getSyncTags`, add/update/delete push endpoints), with programmable USN sequences/paging, login token semantics (bad password rejected), injectable `Msg=='conflict'` responses, and push-request recording for assertions.
- Verification: `cargo test --manifest-path rust/Cargo.toml` green on integration branch (4/4 smoke tests: full dataset replay with paging, login ok + bad password, injected conflict, pushes recorded); `cargo clippy --all-targets` clean.
