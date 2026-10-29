# Issue 38 — 斜杠面板二轮：宽度收紧 / 删标签条 / 插入位置修正

- 状态：done
- 日期：2026-10-29
- 模块：`rust/crates/leanote-app/src/slash_panel.rs`、`app_shell.rs`
- 版本：3.1.1

## 现象（用户反馈）

1. 面板太宽，超出 markdown 编辑区；内部 icon 格子稀疏。宽度应随 icon 格子自适应。
2. 已有 tooltip，面板底部标签条冗余，要求去掉。
3. 点击菜单插入的内容不在当前光标处 —— 实际多删了字符，向前多吃了一行，插入位置偏移。

## 根因

1. `prepaint` 宽度公式按 `COLS=4` 整行计算，即使过滤后只剩 1–3 项，容器仍按 4 列宽渲染（`layout_as_root` 给的是 `Definite(viewport.width)`，grid 沿 flex 拉伸占满容器）。
2. 标签条是 Issue 37 需求 4 的产物，用于键盘切换时的即时提示；tooltip 落地后职责重叠。
3. `execute()` 开头按 `n = 1 + query.chars().count()` 做 `caret.saturating_sub(n)` 删除"触发范围"。但 `/` 触发键与过滤字符在 `capture_key_down` 里一律 `stop_propagation` 吃掉，**从未写入编辑器 buffer**（Issue 36 Esc 关闭回填逻辑即为此存在）。该删除删掉的是用户本来已有的正文 —— 用户报告"向前多删一行"即此。

## 修复

1. `panel_size(n)` 新增共享尺寸函数：宽度随 `min(n, COLS)` 实际列数收紧；`panel_div` 容器 `.w(px(w))` 显式定宽，`prepaint` 定位公式同源，防漂移。
2. 删除 `LABEL_H` 常量、`panel_div` 中 `sel_label`/`label_bar` 构造；高度公式去掉 `LABEL_H`。
3. `execute()` 删除整个"删除触发范围"代码块，签名去掉无用的 `panel: &SlashPanel` 参数；三个调用点（Enter `capture_action`、Tab `IndentInline`、`render()` 内格子 `on_click`）同步改为 `selected` 直接传值。
4. 新增回归测试 `app_shell::tests::slash_panel_inserts_at_caret`：真实窗口 + `simulate_keystrokes` 全管线 —— `/` 呼出面板（buffer 不变）→ `head` 过滤 → Enter 执行 → 断言 `"line one\nline # two"`（光标 14 处插入 `# `，前方零删除）+ 光标 16 + 面板关闭。修复前该测试失败（旧代码删 5 字符得 `"line one\n# two"`）。

## 验证

- `cargo test -p leanote-app slash`：4/4（原 3 + 新回归 1）。
- `cargo test --workspace`：19 个测试二进制全绿。
- `cargo build -p leanote-app` / `cargo clippy -p leanote-app`：无 error、无 unused 告警。
- 视觉项（宽度观感、tooltip）留手动：`/` 呼出 → 过滤至 ≤3 项 → 面板应收窄到实际列宽、无底部标签条。
