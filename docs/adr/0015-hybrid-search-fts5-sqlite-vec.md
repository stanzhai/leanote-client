# 混合检索：FTS5 + sqlite-vec，仅桌面向量

统一搜索核心从"全库载入内存做子串匹配"升级为混合检索：FTS5 全文索引（bundled SQLite 已编译开启，零新依赖；中英文 run 统一用 bigram+单字预处理而非内建 tokenizer——trigram 要求查询 ≥3 字、unicode61 按空白切词对中文失效，而旧搜索的子串语义要求单字母查询也能命中）+ sqlite-vec 向量索引，两路经 RRF（k=60）融合——只用排名不做分数归一化，BM25 与余弦相似度量纲不可比。向量路径仅桌面：sqlite-vec 是需随构建编译/加载的 C 扩展，移动端（ADR-0013）为此复杂化构建链不值得；移动端靠核心的单路退化获得 FTS5，检索 API 与返回结构两端一致。FTS 表由 Rust 侧 `update_fts` 在三个写入口维护而非 SQL trigger，因为 bigram 预处理无法在纯 SQL 完成；检索入口额外做一次 O(n) 对账自愈外部直写 SQLite（JS CLI 双模、夹具都这么干）造成的漂移。Embedding 走用户配置的 OpenAI 兼容服务（设置「搜索」分区），未配置/不可达自动退化为纯全文；模型或维度变更使向量全失效，meta 记录后提示手动重建索引（存量回填不自动——用户没同意不替他花钱调 API）。

## Status

accepted

## Considered Options

- 加权归一化分数融合：两套分数分布不同，需持续调权，弃。
- 移动端也编 sqlite-vec：iOS/Android 构建链复杂化，v1 明确不做；未来想上，接口无需变动。
- SQL trigger 维护 FTS：bigram 必须在应用侧做，trigger 写不了，弃。
- 自动全量回填 / 静默混用变更后的旧向量：前者替用户花钱，后者返回错误结果，均弃。
