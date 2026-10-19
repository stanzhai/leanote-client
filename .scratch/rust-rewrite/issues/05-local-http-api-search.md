# 05: 本地 HTTP `/api/*` + 统一搜索核心

**What to build:** 外部脚本用 curl/HTTP 打 `127.0.0.1:8912`：`GET /api/ping` 返回与现版一致；`POST /api/note/addNote`、`/api/note/updateNote` 行为与现版逐字段一致（PascalCase 载荷、无指定笔记本自动建「AI札」、IsMarkdown、ID 前缀匹配、标签 upsert、CORS `*`、5MB 上限、仅绑 127.0.0.1）。同一票内落统一搜索核心（大小写不敏感子串、Title+Content+Tags、权重 Title 3/Tags 2/Content 1、±30/+50 摘录窗口），供 HTTP/MCP/⌘K 共用。现有性能冒烟脚本通过。

**Blocked by:** 01

**Status:** resolved

- [ ] `/api/ping` 响应与现版对拍一致
- [ ] `/api/note/addNote` / `updateNote` 的成功与错误响应字段与现版对拍一致
- [ ] 「AI札」自动创建、标签自动 upsert、IsMarkdown 写入
- [ ] 搜索核心：加权排序、摘录窗口、大小写/子串语义有单元测试（含中文输入）
- [ ] CORS `*`、5MB body 上限、仅绑 127.0.0.1 生效
- [ ] tests/ 里的 API 性能冒烟脚本对 Rust 版跑通

## Comments

**Merged:** `88512762` on `integration/rust-rewrite` (fast-forward from `6e0f02d0`; no conflicts — both-side crate additions were additive, Cargo.toml union intact).

**Summary:** `leanote-http` crate — 127.0.0.1-only bind, CORS `*`, 5MB body cap, `/api/ping` + `addNote` + `updateNote` byte-parity via 10 fixtures captured from real `src/local_server.js` (`matches_js_server_bytes` green). Search core in `leanote-core`: case-insensitive substring over Title+Content+Tags, weights 3/2/1, ±30/+50 excerpts in UTF-16 units, CJK tested. `tests/test_performance_smoke.js` unmodified → Pass 9 Fail 0.

**Evidence:** `cargo test --manifest-path rust/Cargo.toml` → 43 passed, 0 failed (cli 9, search 10, store 9, fake-server smoke 4, http api 10, js_parity 1).
