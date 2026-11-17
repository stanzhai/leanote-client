# 03 桌面后台索引器 + 设置「搜索」分区

Prefs 加 `search: SearchPrefs { base_url, api_key, model, dim }`。设置面板新分区「搜索」：三字段 + 测试连接（POST 单条 embedding，回报维度）+ 索引状态（n/m、模型）+ 重建索引按钮（进度）+ 一行隐私说明。索引器：同步完成后与防抖保存后增量入队，后台批量嵌入；模型/维度变更横幅提示重建。

Status: resolved
Blocked by: 02

## Comments


## Answer

Done: `SearchPrefs.embedding`（prefs.json serde 默认，旧文件无迁移）；设置第四分区「搜索」（BaseURL/Key/模型/维度 + 测试连接 + n/m 状态 + stale 警示 + 重建带进度 + 隐私行 + 空 FTS 提示，i18n 中英）；`kick_embedding_indexer` 挂同步完成 ×3 与防抖保存后，`index_busy` 防重入，失败仅日志；⌘K 走 `search_with(cfg)`。CLI/MCP 无配置管线，走 FTS-only（核心 `Store::search`）。