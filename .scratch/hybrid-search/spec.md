# 混合检索 (Hybrid search)

Spec per ADR-0015. 在统一搜索核心（leanote-core `search_notes`）上实现混合检索：FTS5 全文 + sqlite-vec 向量，RRF (k=60) 融合，任一路缺席自动退化。四端（⌘K / CLI / MCP / 移动端）同一核心，无兼容分支。Embedding 服务为设置「搜索」分区的 OpenAI 兼容配置，仅桌面；移动端 v1 纯 FTS。

## 决策摘要（grilling 共识，Q1–Q15）

1. 嵌入文本 = Title + "\n" + 纯文本正文（HTML 剥标签；Markdown 容忍语法标记）；标签只进 FTS 不进向量。
2. 后台增量索引：同步拉取后 + 防抖保存后入队，批量 16 条；失败重试，不阻塞用户操作。
3. 存量回填手动触发（设置「搜索」→ 重建索引，带进度）；首次配置完成弹提示引导。
4. 未配置 / 不可达 → 自动退化纯 FTS，不报错；设置面板显示索引状态（n/m、模型名）。
5. 模型/维度变更：search_meta 记录；变更即索引过期，横幅提示重建，期间退 FTS。
6. 融合 = RRF k=60。
7. 中文分词 = 索引与查询双侧 bigram 预处理 + 短语匹配（1~2 字查询可命中）。
8. 移动端 = 核心单路退化，接口/返回结构一致，无 sqlite-vec。
9. 不新增 HTTP `/api/search`（YAGNI）；MCP/CLI/⌘K/移动端经核心自动获益。
10. API Key 存 prefs.json（与现有 token 同威胁模型）。
11. FTS 写入 = Rust 侧 `update_fts` 收口三个写入口，非 trigger。
12. 隐私 = 设置面板一行说明，不弹窗。

## Issues

- 01-fts5-bigram-core
- 02-sqlite-vec-embedding-hybrid
- 03-desktop-indexer-settings
- 04-verify-four-surfaces
