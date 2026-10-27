# ADR-0009: notebook push 携带映射后的 parentNotebookId 且父先子后

- 状态：Accepted（2026-10-27，拖放排序破坏服务端层级修复轮裁决）
- 关联：GLOSSARY「笔记本」「同步」；JS 权威参考 `src/api.js:417-436`、`src/notebook.js:412-428`

## 背景

Rust 版 `push_dirty_notebooks` 曾硬编码 `parentNotebookId=""`，且 `dirty_notebooks()` 的 SELECT 不含 `ParentNotebookId` 列，push 顺序按 rowid——本地拖放把笔记挂到子笔记本后，push 把所有脏笔记本平铺成根级，服务端层级与排序被弄乱。

## 决策

1. **push 载荷带映射后的父服务器 id**：`DirtyNotebook` 增加 `parent_notebook_id` 字段；push 时本地父 id 经 `Store::notebook_server_id()` 映射为服务器 id（空字符串或映射不到回退 `""`，即根级）——对齐 JS `getServerNotebookIdByNotebookId`。`addNotebook` 与 `updateNotebook` 均传。
2. **父先子后**：按脏集合内的父链深度 stable sort 后逐个 push，对齐 JS `getDirtyNotebooks` 的 DFS 前序——父笔记本先在服务端存在，子笔记本引用的父 id 才有效。
3. **本地行不动**：本地 `NotebookId`/`ParentNotebookId` 仍是本地 id，仅 push 载荷做 id 翻译。

## 后果

- 服务端层级与本地拖放意图一致；跨端重拉后树形不丢。
- 依赖服务器 addNotebook 返回的 `NotebookId` 及时写入本地映射（`ServerNotebookId` 列）；同一批 push 中子引用父时父必已 push 完毕（深度排序保证）。
- 已被回归测试锁定：`notebook_hierarchy_pushes_parent_server_id_parent_first`（leanote-cli tests/sync.rs）。
