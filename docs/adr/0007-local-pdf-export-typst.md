# ADR-0007: PDF 导出本地化（typst）

- 状态：Accepted（2026-10-23，issue 23 裁决 Q1）
- 关联：ADR-0006（笔记图片），issue 15（导出 HTML/MD/PDF/备份），issue 23

## 背景

PDF 导出原走服务器 `note/exportPdf`（与 Electron 版同路径）。实测服务端端点返回 `sysError`（issue 23 复现），且依赖服务器意味着离线不可用、导出格式不可控。

## 决策

导出 PDF 改为**完全本地**实现：`leanote-app/src/pdf.rs` 用 typst 0.15.1 栈（`typst` / `typst-pdf` / `typst-kit`）在进程内编译 A4 文档并写盘。

- 序列化：复用现有 `markdown` crate 的 GFM AST → typst 标记（纯函数 `markdown_to_typst`）；内容模式特殊字符全部转义。
- 字体：`typst-kit` FontSearcher 先扫系统字体（CJK 覆盖），嵌入式字体兜底拉丁文；进程范围 `LazyLock` 缓存一次扫描。
- 服务器路径 clean cutover：`Api::export_pdf`、fake-server 的 exportPdf 路由/处理器/夹具全部删除，无回退分支。
- 已知取舍：图片不嵌入（裁决）； fenced code 含 ``` 的列提前闭合、`start≠1` 有序列表重新编号（罕见，见 pdf.rs 注释）。

## 后果

- 导出离线可用、不再依赖服务器端点行为；首字节含 `%PDF` 头的输出由单测保证。
- 二进制体积 +10–20 MB（typst + 字体）；首次导出付一次性字体扫描（macOS <1s）。
- 失败路径直接把 typst 诊断 message 展示在导出面板，无静默回退。
