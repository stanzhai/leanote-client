# 01 FTS5 + bigram 全文索引核心

Schema v3：standalone FTS5 表 `notes_fts(note_id UNINDEXED, title, body, tags)`；bigram 预处理（CJK 双字、拉丁小写词）；查询侧同预处理 + 短语匹配。`update_fts` 收口 `add_note`/`update_note`/`apply_sync_notes`；迁移回填。`Store::search` 改走 FTS（过滤 IsTrash/IsDeleted），excerpt 维持 JS parity。测试：bigram、迁移、写入口、查询。

Status: resolved

## Comments

## Answer

Done: schema v3 + `notes_fts` + `update_fts` 三入口收口（含 sync 内容阶段 `update_fts_body`）；bigram 统一中英文 run（单字母/单字查询命中，JS 子串 parity）；检索入口 `reconcile_fts` 自愈外部直写漂移。CLI 字节 parity 测试全绿。
