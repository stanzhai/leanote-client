# 02 sqlite-vec + Embedding 客户端 + RRF 混合编排

sqlite-vec 可选加载（失败即禁用向量路）；`vec_notes`（vec0，维度随模型 DDL 时定）、`embeddings(note_id, content_hash, updated_at)`、`search_meta`。OpenAI 兼容 `POST /embeddings` 客户端（批量 16，`EmbeddingConfig` serde）。`search_hybrid`：FTS BM25 排名 + 查询向量 KNN → RRF k=60，单路退化，embedding 失败不报错。`reindex_embedding`/`reindex_fts`。结果加 `matched_via`，旧字段不动。

Status: resolved
Blocked by: 01

## Comments


## Answer

Done: sqlite-vec 静态注册（`sqlite3_auto_extension`，无 load_extension 特性），`vec_version()` 探测失败即禁用向量路；`embeddings`/`search_meta` 表；ureq `/embeddings` 客户端批量 16；`search_hybrid` RRF k=60、失败静默退化；`reindex_embedding`/`reindex_fts`/`index_stats`/`embedding_index_stale`。live e2e（本地 embeddings 服务器）验证 KNN+RRF 与模型变更重建。