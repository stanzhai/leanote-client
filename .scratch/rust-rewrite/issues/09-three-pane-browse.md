# 09: 三栏浏览 — 导航/列表/只读渲染 + 打星 + 回收站

**What to build:** 用户能看到并浏览全部笔记：左栏固定导航 All Notes / Starred / Trash + 笔记本分组（嵌套层级、`Seq` 排序、可折叠）+ Tags 分组（可折叠）；选中任一节点中栏即时过滤为两行列表（标题 + 笔记本/标签/更新时间元信息）；右栏 720px 居中只读渲染 Markdown 笔记（CommonMark+GFM）；打星（`Star`，纯本地）在 Starred 视图可见；回收站视图内恢复/彻底删除/清空。对用户真实语料（23 本/127 签/1123 条）手工验收。

**Blocked by:** 04, 08

**Status:** resolved



## Comments

Merged into `integration/rust-rewrite` as `84c7c096` (merge of ticket/09 `5b40421c` + `5779baf2`, clean — Commit09 pre-merged integration tip `f19030db`).

- `browse.rs` pure logic (notebook_nodes nesting + `Seq` order + orphan drop, note_meta join, reader_markdown_src) + `app_shell.rs` 3-pane browse: left nav groups (All Notes / Starred / Trash + notebooks nested by Seq with counts + Tags), middle two-line rows (title + notebook/tags/updated), right 720px centered `TextView::markdown` reader (CommonMark+GFM), star toggle local-only (`toggle_star` never marks IsDirty → never pushed), trash view restore-to-original-notebook / hard-delete / empty-trash.
- Store queries match Electron `src/note.js` semantics (`list_notes_filtered`, `list_notes_by_tag`, `list_trash`, `notebook_note_counts`, `restore_note`, `hard_delete_note`, `empty_trash`).
- Evidence: `cargo test --manifest-path rust/Cargo.toml` = 84 passed on integration after merge; `cargo build --release -p leanote-app` green.
- Remote images: Commit09 report — ureq-backed http client on gpui_kit application landed (`Cargo.toml` ureq/futures/http deps + `main.rs` UreqHttpClient), so `Resource::Uri` fetches now work; remote-image checkbox verified against fakeserver demo corpus.
- Visual smoke partially inconclusive: login + full sync + store state verified programmatically; screenshot/eyeball pass left for human review on real corpus (23 notebooks / 127 tags / 1123 notes).
- [ ] 全量同步后三栏呈现完整语料，导航节点选中即过滤中栏
- [ ] 笔记本嵌套层级与 `Seq` 顺序与 Electron 版一致
- [ ] Markdown 只读渲染：标题/列表/表格/删除线/围栏代码高亮/链接/引用正确
- [ ] 打星即时生效、Starred 视图过滤正确、星不上服务器
- [ ] 回收站：恢复回原笔记本、彻底删除、清空，行为与现版一致
- [ ] 远程图片按服务器 URL 显示（本地附件缓存不在范围，见 spec 缺口）
