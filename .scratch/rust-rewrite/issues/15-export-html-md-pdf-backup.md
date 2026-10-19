# 15: 导出 — HTML/Markdown/PDF/全库备份

**What to build:** 用户能带走数据：单笔记导出为 HTML 文件与 Markdown 文件；单笔记导出 PDF（走服务器既有 exportPdf 端点，端点失败降级导出 HTML 并明确提示）；全库导出 `.leanote` JSON 备份（笔记元数据 + 内容 + 内嵌文件 base64）。保存面板/路径选择遵循平台习惯。导入方向不在范围（spec 裁撤）。

**Blocked by:** 09

**Status:** resolved

- [x] 单笔记导出 HTML：双击浏览器打开排版正确，内嵌图片以服务器 URL 引用
- [x] 单笔记导出 Markdown：内容与库内一致，GFM 表格/代码块保留
- [x] 单笔记导出 PDF：服务器端点成功时落 PDF；失败时降级 HTML 并提示原因
- [x] 全库导出 `.leanote` JSON：字段结构与现版导出格式对拍一致，文件 base64 内嵌
- [x] 大笔记（长文/大表格）导出不卡 UI、不崩

## Evidence

| 验收项 | 证据 |
|---|---|
| HTML 导出 | `export.rs::note_to_html` — 内联 CSS + GFM 渲染，图片以服务器 URL 引用 |
| Markdown 导出 | `export.rs::note_to_markdown_file` — 内容 verbatim，GFM 表格/代码块保留 |
| PDF 导出/降级 | `Api::export_pdf` — `%PDF` 魔数检测；失败降级 HTML 并带原因；fakeserver `exportPdf` 路由 + 失败注入测试 |
| `.leanote` 备份 | `export.rs::backup_to_leanote_json` — 对齐 JS 插件结构（含 `leanote://` URL 重写 + base64/md5 文件内嵌） |
| 大笔记不卡 UI | 导出走后台线程 + 平台原生保存面板；PDF smoke 挂起根因修复：`current_thread` runtime 饿死 spawned server → 改 `multi_thread`，60s → 0.02s |

## Comments

**7a257f96** merged into `integration/rust-rewrite` (clean fast-forward from 52bf91fa).

Summary: `export.rs` 新增 `note_to_html`（内联 CSS + GFM）、`note_to_markdown_file`（verbatim）、`backup_to_leanote_json`（匹配 JS 插件导出结构，含 `leanote://` URL 重写与 base64/md5 文件内嵌）；`Api::export_pdf` 走服务器端点，`%PDF` 魔数检测，失败降级导出 HTML 并提示原因；fakeserver 增加 `exportPdf` 路由与失败注入。UI 侧导出在后台线程执行并使用原生保存面板。PDF smoke 挂起根因：`current_thread` runtime 让主线程阻塞在 ureq 时饿死了 spawned server task，改 `multi_thread` 后 60s → 0.02s。合并中 13+14+12 的导出按钮/状态、冲突条、搜索层做加性并集，i18n 键并集；`7a257f96` 修复加性解决时丢失的 `home_dir`/impl 边界残留。

Verification: `cargo test --manifest-path rust/Cargo.toml` — 124 passed, 0 failed（跨全部 crate）；`cargo build --release -p leanote-app` 成功。
