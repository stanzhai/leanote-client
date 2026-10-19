# 06: MCP 端点 — 8 工具、Streamable HTTP 与 stateless 双模式、遗留 SSE

**What to build:** AI agent / MCP 客户端对 `http://127.0.0.1:8912/mcp` 的既有配置零修改可用：Streamable HTTP 有状态会话（上限 50、30 分钟 TTL）+ stateless 直发两种模式都工作；8 个工具（list_notebooks / list_tags / add_note / list_notes / get_note / search_notes / update_note / delete_note）名称与 schema 与现版一致，delete_note 送回收站不硬删，search_notes 用票 05 的统一搜索核心。验收就是现有三份 MCP 测试脚本不改一行全绿。

**Blocked by:** 05

**Status:** resolved

## Comments

**Merged:** `b9b67f33` on `integration/rust-rewrite` (fast-forward — `ticket/06` already contained integration tip `5c971cbe`; zero conflicts, leanote-core additions additive).

**Summary:** MCP server in `leanote-http` (`mcp.rs`, 915 lines): Streamable HTTP `/mcp` stateful (50-session cap, 30-min TTL, LRU eviction) + stateless direct mode; legacy `/sse` + `/messages`. 8 tools (list_notebooks / list_tags / add_note / list_notes / get_note / search_notes / update_note / delete_note) byte-parity with JS stack incl. preserved SDK key order (`result, jsonrpc, id`); delete_note → trash (IsTrash), not hard delete. `leanote-core` additions: `trash_note`, `set_notebook`, `find_notebook_by_title`, `find_note_by_prefix` trash-exclusion parity fix. `LEANOTE_MCP_DEBUG=1` → `~/leanote_mcp.log` with 10MB rotation. Test bridge: `LEANOTE_RUST_MCP=1` in `src/local_server.js` spawns `rust/target/debug/leanote serve --port` so unmodified JS suites hit the Rust implementation.

**Evidence:** `cargo test --manifest-path rust/Cargo.toml` → 53 passed, 0 failed (mcp 5 new). 3 unmodified JS suites green via bridge: `tests/test_mcp_server.js` (stateful, ALL CHECKS PASSED), `tests/test_mcp_stateless.js` (HARPA OK), `tests/test_mcp_sse.js` (SSE TRANSPORT OK).
