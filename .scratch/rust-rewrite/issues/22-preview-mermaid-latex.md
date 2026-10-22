# 22: 预览渲染 mermaid 图与 LaTeX 数学公式

**What to build:** issue 21 需求 8 推迟项。GPUI 无原生 mermaid/LaTeX 渲染器，需先定方案再实现：

1. mermaid 代码块（```mermaid … ```）在 阅读态/编辑预览/⌘T 全窗口预览 渲染为图（JS 客户端经 iframe + mermaid.js）。
2. LaTeX 数学（`$…$` 行内、`$$…$$` 块级）渲染为公式（JS 客户端经 MathJax）。
3. 编辑态保持源码（无渲染）。

**候选方案**（issue 21 烤问记录）：
- A. 外部进程渲染：起本地 headless Chromium/`resvg` 管道，SVG→PNG 落盘，预览区显示位图。重依赖，离线可用。
- B. 内嵌 WebView 层（如 `wry`/`tao` 一个窗口层叠 GPUI 窗口）跑 mermaid.js + MathJax，转发渲染结果。中依赖，双窗口同步复杂。
- C. 纯 Rust 子集渲染：`pulldown-cmark` 数学 AST + 自绘/`mathjax-rs` 类库 + mermaid 手绘常用图形子集（flowchart/sequence）。无 JS 依赖，覆盖面有限。

**Done 判据:** demo seed 笔记含 mermaid 流程图 + 行内/块级公式，三处预览（阅读/编辑预览/⌘T）均正确渲染；`cargo test` 全绿；真机 smoke 截图确认。

**Blocked by:** 21

**Status:** open
