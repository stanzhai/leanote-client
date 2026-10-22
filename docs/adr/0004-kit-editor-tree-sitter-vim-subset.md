# 4. Markdown 编辑器用 kit Editor + tree-sitter + 自建 Vim 子集；默认进入编辑态

日期: 2026-10-20
状态: 已接受（UI 优化轮 grilling Q4/Q10/Q11/Q12/Q13 用户裁决"都按推荐的来处理"）

## 背景

spec 期（ticket 10）的编辑器是 kit `Textarea` 裸文本域。UI 优化轮用户要求：语法着色、配色风格、Vim 模式、默认即编辑态（无需点「编辑」）。侦察证实 gpui-kit 0.7.3 自带完整代码编辑器（`EditorState`/`Editor`：rope、撤销、多光标、`InputHighlighter` 高亮接缝、tree-sitter 适配器，声明支撑约 50K 行），但 kit 无任何 modal/vim 支持，且本仓库未开 tree-sitter feature。

## 决策

1. **不自研编辑器、不嵌 neovim/helix**：正文编辑面用 kit `Editor`（`EditorState`）+ `Editor` 组件，替换 `Textarea`。标题行仍是独立 kit `Input`，两者统一走「内容有变才置脏」的保存路径。
2. **语法着色走 tree-sitter**：启用 gpui-kit 的 tree-sitter feature：`tree-sitter-markdown`、`tree-sitter-markdown-inline`、`tree-sitter-rust`、`tree-sitter-javascript`、`tree-sitter-typescript`、`tree-sitter-tsx`、`tree-sitter-html`、`tree-sitter-css`、`tree-sitter-bash`、`tree-sitter-python`、`tree-sitter-go`、`tree-sitter-yaml`、`tree-sitter-toml`（见 `rust/crates/leanote-app/Cargo.toml`）；语言注册进 kit 的 LanguageRegistry；编辑器配色（capture→颜色映射）由本仓库提供 2–3 套，默认跟随系统明暗，仅作用于编辑器，不推翻 ADR-0003 的全局双材质。
3. **Vim 是键位层，不是引擎 fork**：自有 mode 状态（normal/insert/visual）+ pending operator，动作全部映射到 `EditorState` 公开 API（`set_selected_range`/`replace`/`set_cursor_position` 等，引擎的移动/删除原语已是 GPUI action）。明确边界：常用子集（motion、operator、yank/paste、`:` 命令 `:w`/`:q`），不承诺完整 vi 兼容（无寄存器 `"` 语法、无宏、无插件）。
4. **选中 Markdown 笔记即进入编辑态**（默认源码模式，保留预览开关）；自动保存 = 内容变化时防抖置脏，⌘S 保留。
5. **快捷键仲裁**：⌘K/⌘⇧P（及 Win/Linux 对应修饰键组合）触发面板；裸键永远归编辑器/输入框焦点，全局键不拦截无修饰键。

## 考虑过的方案

- 自研逐行渲染编辑器：引擎、撤销、光标几何全要重写，被 kit Editor 的存在否决。
- 嵌 helix/neovim 子进程：vim 一等公民但与 GPUI 视觉割裂，违背「严格 Wake 风格」。
- 只开 `tree-sitter-markdown` 单语言 feature：编译轻，但内嵌代码块不着色；用户裁决取整包。

## 后果

- 编译时间与二进制体积上升（tree-sitter grammars），一次性代价。
- Vim 子集是长期承诺边界：不兼容 vi 的行为是刻意的，勿"修复"成完整 vim。
- 2026-10-22 追记：vim 层按 rgpui `editor/vim` 架构重写为「纯文法状态机（`vim.rs` `handle_key`）+ 直接执行（`exec` 直改 `EditorState`）」，不再绕 kit action 往返（唯一例外是 Undo——引擎 `undo()` 非 public，走 `window.dispatch_action(Undo)`，GPUI defer 保证无重入）。边界与子集不变。
- `TextareaState` 类型别名与 `EditorState` 同引擎，旧编辑器代码迁移面小。
