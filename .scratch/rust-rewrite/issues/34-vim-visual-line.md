# Issue 34: vim 模式 Shift+V 行选择(visual linewise)

状态:已完成(2026-10-27)

## 需求

markdown 编辑器 vim 模式支持 `V`(Shift+V)行选择:

- normal 下按 `V` → visual linewise,选中光标所在整行
- `V` 后 `j`/`k` 按行扩展多行;所有 motion(w/b/$/G/…)只取目标行号,列移动不改选区
- `d`/`x` 行级删除(镜像 `dd` 边界:吃尾换行不留空行、最后一行吃前导换行、单行 buffer 清空)
- `y` 行级复制 → `p` 按行粘贴在光标行下方(复用 `register_linewise` 标志与 `put_register` 现有路径)
- visual 内 `V` linewise↔charwise 互切(`v` 降级、`V` 升级),`escape` 退出

## 实现

`rust/crates/leanote-app/src/vim.rs`:

- `VimState.visual_linewise: bool` — `leave_visual`/`set_mode`(非 Visual)清零
- `VimAction::EnterVisualLine` — 进入/升级为 linewise;exec 时从 charwise 升级保留 `visual_cursor` 作为新 caret(v+j+V 选两行),否则用 `selected_range().start`
- `normal_key`:`"v"` 清 linewise,`"V"` 置位并返回 `EnterVisualLine`
- `visual_key`:`"escape"` 单列;`"V"` linewise 下退出、charwise 下升级;`"v"` linewise 下降级、charwise 下退出
- `exec` `Move`:linewise 下 `extend_linewise_row`(按 target_row snap,忽略列)
- `exec` `DeleteSelection`:linewise 分支 snap 行边界删除,`register_linewise = true`,rest 到 del_start
- `exec` `YankSelection`:linewise 分支重新推导完整行 span 存 register(**无尾换行** — `put_register` 粘贴时自己加 `\n` 前缀,与 `yy` 约定一致),rest 到**最后一行**行首(否则 `p` 粘贴位置错行)
- helper:`linewise_range(text, anchor_row, target_row)` / `extend_linewise(state, caret, cx)` / `extend_linewise_row(vim, state, target_row, cx)`

## 测试

- `vim.rs` `visual_line_toggle`:V 进入、V/V 互切、v 降级、escape 退出后标志复位
- `app_shell.rs` `vim_visual_line_selects_edits`(端到端):V 选整行 0..4 → j 扩到 0..8 → y p 行级粘贴 → jj V j d 整行删除无残留

69 tests passed(baseline 67 + 2)。

## 决策

- 复用 `register_linewise`:V 的 yank 直接置位,粘贴走现有 `put_register` 行级路径,不加新粘贴代码
- 删除边界镜像 `LineOp(Op::Delete)`(dd)的换行处理,行为一致
- v/V 在 visual 内互切而非退出 — vim 真实行为;escape 才退出

## 验证

- `cargo build -p leanote-app` 无错误/新警告
- `cargo test -p leanote-app` 69 全绿
- 冒烟(LEANOTE_FAKE_SERVER + LEANOTE_DATA=/tmp/leanote-smoke,单实例锁 59590):0 panic / 0 error
