# Issue 40: vim 模式 Shift+i / Shift+a 行首、行尾插入

状态:已完成(2026-11-02)

## 需求

markdown 编辑器 vim normal 模式补齐两个插入入口:

- `I`(Shift+i) → 行首第一个非空白字符处进入 insert(整行空白回落行首,与 `^` 同语义)
- `A`(Shift+a) → 行尾(最后一个字符之后)进入 insert

此前 Shift+i/Shift+a 落入 unknown 键被吞(无动作)。

## 实现

`rust/crates/leanote-app/src/vim.rs`:

- 新增 `InsertAt` 枚举(`Caret`/`Append`/`OpenBelow`/`OpenAbove`/`FirstNonBlank`/`LineEnd`),`VimAction::EnterInsert` 从三 bool 结构体改为单字段 `EnterInsert(InsertAt)` —— 六个插入键各一行绑定,`i`/`a`/`o`/`O` 行为不变
- `normal_key` 新增 `"I"`/`"A"` 键;visual 模式刻意不进(子集无 block-insert),照旧吞掉
- `exec` `EnterInsert`:`FirstNonBlank` → `first_non_blank` 新 helper(从 `^` 的内联计算抽出共用,空白行回落行首),`LineEnd` → `line_range().1`(行尾边界,`\n` 之前/文末)
- 键路无需改动:`vim_key_down` 既有 shift+小写→大写映射(G/V/D 同路径)已覆盖 I/A

## 测试

- `vim.rs` `insert_entry_and_exit`:六个插入键表驱动断言(含新增 I/A)
- `app_shell.rs` `vim_exec_line_ops_dollar_visual`(端到端扩展):缩进行 `I` → 光标 2..2 + block 光标消失 → 真实输入链路输入 X 落在非空白前;escape 后 `A` → 6..6 → 输入 `!` 落在行尾

104 tests passed。

## 验证

- `cargo build -p leanote-app` 无错误/新警告
- `cargo test -p leanote-app` 104 全绿
