# 1. 富文本笔记在 Rust 版中降级为只读 + 可转 Markdown

日期: 2026-10-19
状态: 已接受（Q1=A、Q9=A）

## 背景

重写目标之一是去除 web UI 依赖（Q1=A）：不内嵌 webview、不复刻 TinyMCE、不移植 35k 行的 StackEdit 式 markdown 编辑器。但现有语料 1123 条笔记中有 20 条富文本（`IsMarkdown:false`，真实存档笔记：买房、面试、电影清单等），服务器协议是 HTML 载荷，Rust 版仍要能显示它们。

## 决策

1. 富文本笔记（`IsMarkdown:false`）在 Rust 版中**只读渲染**：把 HTML 解析为 GPUI 文本树展示（标题/列表/表格/图片），不提供可视化编辑。
2. Markdown 笔记用原生编辑器编辑（源码模式 + 渲染预览，具体形态由后续设计定）。
3. 提供“转存为 Markdown”操作：把 HTML 结构化转成 Markdown 覆盖该笔记（用户显式触发），转换后即变为可编辑。
4. CLI/MCP 写入一律 `IsMarkdown:true`（与现行为一致）。

## 后果

- 正：去掉整个 TinyMCE 依赖树；20 条存量笔记不被阻塞；升级路径（逐条手动转 Markdown）明确。
- 负：富文本笔记在转换前不可在 Rust 版编辑；HTML→MD 转换会丢非 Markdown 表达力（如复杂表格、内嵌样式），转存即接受该损失。
- 若未来富文本需求增长，需要独立立项做原生富文本组件，不属于本重写。

## 依据

语料统计：1103/1123 条为 Markdown；`IsMarkdown:false` 的 20 条标题均为存档性质（`~/Library/Application Support/leanote/nedb55/5368…/notes.db`，2026-10-19 扫描）。
