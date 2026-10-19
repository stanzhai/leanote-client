# 12: 富文本只读渲染 + 转存为 Markdown

**What to build:** 用户的 20 条存量富文本笔记（`IsMarkdown:false`）在 Rust 版可读：HTML 解析为 GPUI 文本树呈现（标题/段落/列表/表格/链接/图片），标注只读；用户显式执行「转存为 Markdown」后该笔记变为可编辑（接受转换损失），转换器对常见 TinyMCE 产物（嵌套列表、表格、行内样式）有单元测试。不实现富文本编辑（ADR-0001）。

**Blocked by:** 09

**Status:** resolved

## Comments

Merged into `integration/rust-rewrite` as `fd1e71b6` (fast-forward — ticket/12 had pre-merged integration tip `1d52c795` via `1d18a88a`).

**Summary**

- `rust/crates/leanote-core/src/html2md.rs`: htmd-based `html_to_markdown` converter with custom `<u>`/`<s>` handlers (htmd has no built-in underline/strikethrough rules) — headings, nested lists, tables, links, images, bold/italic, code blocks all covered; malformed HTML and plain text pass through without erroring (8 unit tests in-module).
- `Store::convert_to_markdown` (leanote-core/src/lib.rs): one-way convert — rewrites content via html2md, sets `IsMarkdown=1` + `IsDirty=1`; idempotent (returns `None` when already Markdown, never marks clean notes dirty), so the next push sends Markdown to the server.
- Reader (`browse.rs` `reader_markdown_src`): non-Markdown notes render via display-only conversion; store keeps original HTML until the explicit Convert action. Empty rich body falls back to the title heading.
- `app_shell.rs`: rich-text notes get a `RichTextReadonly` badge + `Convert to Markdown` button in the reader toolbar; on convert, `ConvertedToMarkdown` feedback shows, badge disappears (note is now editable Markdown). No rich-text edit entry exists anywhere.
- e2e: `rust/crates/leanote-cli/tests/sync.rs` convert-then-push test against the fake server proves updateNote push parity (converted content + IsMarkdown=true reach the server).
- Corpus: fake server gains rich notes `n10`（2018 归档：嵌套列表+表格+彩色行内样式+引用）and `n11`（旅行清单：斜体/下划线/嵌套有序列表/链接/图片）.
- Smoke: AX traversal exposed the 「转换为 Markdown」 button on rich-text selection; no edit affordance exposed.

**Checkbox evidence**

| Checkbox | Evidence |
| --- | --- |
| 全部 20 条存量富文本笔记可读、排版无乱码、无内容丢失 | `html2md` unit tests + `reader_markdown_src` tests (headings/nested lists/tables/links/images/bold/italic/code); display-only conversion never mutates the store; smoke over n10/n11 corpus notes |
| 只读态有明确视觉提示，不提供富文本编辑入口 | `RichTextReadonly` badge (i18n `富文本（只读）`/`Rich text (read only)`) rendered in toolbar for `!is_markdown`; no edit action wired for rich notes (ADR-0001) |
| 「转存为 Markdown」显式触发，转换后 IsMarkdown:true、可编辑、置脏待推送 | `Store::convert_to_markdown` sets `IsMarkdown=1`+`IsDirty=1`; `leanote-core/tests/store.rs` idempotency/flag assertions; explicit button only |
| HTML→Markdown 转换器单测覆盖：标题/嵌套列表/表格/链接/图片/粗斜体/代码块 | `html2md::tests` — 8 tests incl. TinyMCE-style nested list/table/inline-style corpus |
| 转存后同步推送，服务器侧内容与本地一致（假服务器验证） | `leanote-cli/tests/sync.rs` e2e: convert → push → fake-server `updateNote` body matches converted local content |

**Verification**

- `cargo test --manifest-path rust/Cargo.toml` on integration after fast-forward merge: **94 passed, 0 failed**.
- `cargo build --release -p leanote-app` (via `--manifest-path rust/Cargo.toml`): green, `rust/target/release/leanote-app` produced.
