# 13: ⌘K 全局搜索 overlay

**What to build:** 用户在任何界面状态按 ⌘K 弹出全局搜索悬浮层（宽 680px，顶部无边框输入 + 结果列表 + 快捷键脚注），即输即搜，结果复用票 05 统一搜索核心（加权排序 + 命中摘录），↑↓ 选择、↩ 打开笔记（右栏直达）、esc 关闭。支持 Windows/Linux 对应快捷键。

**Blocked by:** 09

**Status:** resolved

Merged into `integration/rust-rewrite` as `c90ff4d5` (real merge — ticket/13 tip `0b51f79d` pre-merged integration `37c9e405`; 14's `a3c50f89`/`3add57bd` were not in it). One conflict, `i18n.rs` `ALL_KEYS`: resolved additively — union of 14's conflict keys (`ConflictTitle/ConflictLocal/ConflictServer/UseLocal/UseServer/KeepBoth`) and 13's search keys (`SearchPlaceholder/SearchNoResults/SearchEmptyHint/SearchFootnote`). `app_shell.rs` and `main.rs` auto-merged cleanly (13's search fields/methods and 14's conflict bar coexist).

**Summary**

- `rust/crates/leanote-app/src/search_overlay.rs` (new): Command palette hosted in a gpui_kit Dialog, 680px wide, filterable(false) — searchable input on top, result rows, shortcut footnote. Rows are CLI-parity (same weighted rank + hit excerpt as `leanote-cli search` / MCP `search_notes`) via `format_rows` from the ticket-05 unified search core: title, excerpt, meta line, date. 4 unit tests in-module.
- `app_shell.rs`: `search`/`search_rows` fields; `pub fn toggle_search` bound on the root view as `ToggleSearch` action → `cmd-k` (macOS) / `ctrl-k` (Windows/Linux) fires from any phase (dialog layer is window-level, not pane-level); `run_overlay_search` runs the committed query through `search_store` (unified core) and stashes rows; `confirm_overlay` (↩) opens the note via `select_note` so the reader jumps to it while the middle list keeps its filter/nav state untouched (returnable); `dark()`/`lang()` made `pub(crate)` for the overlay's theming/i18n.
- IME safety: `on_query` only fires on `InputEvent::Change` (committed text) — preedit composition emits no event, so 中文输入法确认前不触发搜索. `should_search(query, false)` gates whitespace/empty queries so blank input never hits the store.
- Empty-input vs no-results are distinct states: empty input → `SearchEmptyHint` (footnote hint, no error); query with no hits → `SearchNoResults` row + `SearchFootnote` shortcut legend. i18n `ALL_KEYS` gains the 4 search keys (zh + en) — exhaustive key test passes.
- `main.rs`: action registration + keybinding wiring on root.
- Tests: 18/18 in leanote-app (includes the 4 overlay unit tests).

**Checkbox evidence**

| Checkbox | Evidence |
| --- | --- |
| ⌘K（及 Win/Linux 等价键）任何状态弹出/关闭搜索层 | `main.rs` binds `ToggleSearch` to `cmd-k`/`ctrl-k` on the root view; `app_shell.rs` `toggle_search` → `search.toggle` — `app_shell` unit tests assert toggle state transitions; dialog is window-level so phase doesn't matter |
| 即输即搜结果与 CLI `search` / MCP `search_notes` 同序同摘录 | `run_overlay_search` → `search_store` (same unified core, same `format_rows`) — `search_overlay.rs` unit tests assert row order/excerpt shape match the core's output for the same query |
| 中文输入法确认前不触发搜索 | `on_query` fires only on `InputEvent::Change` (committed text); preedit emits no Change — `search_overlay.rs` IME unit test; `should_search` gate tested |
| ↩ 打开笔记右栏直达，列表状态保留可返回 | `confirm_overlay` → `select_note` (drives the reader via `selected`), overlay closes, list filter/nav state untouched — unit test covers confirm path; `select_note` preserves list state by design (ticket 09) |
| 无结果/空输入的展示态正确 | distinct branches: empty input → `SearchEmptyHint`; no hits → `SearchNoResults` — unit tests for both states; i18n `every_key_has_both_langs` covers the 4 new keys |

**Visual smoke:** capture exists but is unverified — no human reviewed the rendered dialog. Evidence is unit + structural only (component construction, keybinding wiring, i18n keys) per the table above.

**Verification**

- `cargo test --manifest-path rust/Cargo.toml` on integration after merge `c90ff4d5`: **112 passed, 0 failed** (≥108 expected post-union; exhaustive i18n `every_key_has_both_langs` re-run green with the unioned `ALL_KEYS`).
- `cargo build --release -p leanote-app`: green — `rust/target/release/leanote-app` produced.
