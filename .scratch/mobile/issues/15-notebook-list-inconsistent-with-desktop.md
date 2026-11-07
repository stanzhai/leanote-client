# Issue 15: 移动端笔记本列表与桌面不一致（顺序错乱/嵌进丢失）

状态:已完成(2026-11-07;真机复验待用户)

## 需求

真机报告：移动端笔记本列表显示错乱，与 Rust 桌面客户端的列表不一致。

## 根因

移动端 `notes_tab.rs#notebooks()` 自己排序：按 `(parent_notebook_id 字符串, seq)`
排序 + 深度只有 0/1 两档。与桌面 `browse.rs#notebook_nodes`（roots 按 Seq →
子挂 ParentNotebookId 下按 Seq → 深度优先走树）语义完全不同：

1. **顺序**：字符串排序把所有子笔记本甩到**全部 roots 之后**、按父 id 字典序分组
   ——子项脱离其父，看起来随机错落；
2. **嵌套**：孙辈（depth≥2）一律压到 depth 1 缩进；
3. **孤儿**：父 id 无对应笔记本的孤儿行照样显示（桌面直接不显示）。

## 实现

- `leanote-core/src/nav.rs`（新）：把桌面 `browse.rs` 的纯树构建 `NavNode` +
  `notebook_nodes` **迁入两端共用的 leanote-core**（含两条单测原样随迁）——
  单一实现，移动端从此不可能再与桌面漂移。
- 桌面 `browse.rs`：删除迁出项，改 `use leanote_core::nav::NavNode`；`app_shell.rs`
  导入与调用点（`browse::notebook_nodes` → `notebook_nodes`）同步；drop_target /
  note_meta / 标签 NavNode 用法不动。
- 移动端 `notebooks()`：删自排逻辑，改调 `leanote_core::nav::notebook_nodes`
  （counts 传空——移动端行不显示计数，属设计而非缺陷）。
- 回归测试 `notebooks_nest_children_under_parent`（移动端，真实 SQLite Store）：
  子必须紧跟其父且 depth 1，后位 root 必须排在其父子树之后。

## 验证

- `cargo test -p leanote-core`：13+5+10 绿（含随迁的 nav 两条）。
- `cargo test -p leanote-app`：97 绿（browse 剩余 drop_target/note_meta 4 条 + 全量）。
- `cargo test -p leanote-mobile`：7 绿（含新回归；修复前该测试红——子项落在全表尾）。
- `cargo check --workspace` + `cargo check -p leanote-mobile --target aarch64-apple-ios` 绿。
