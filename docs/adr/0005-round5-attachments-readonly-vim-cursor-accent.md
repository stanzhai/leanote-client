# 5. 第五轮 UI：附件本轮只读不写、Vim 无状态行、历史入弹窗、主题色可配置

日期: 2026-10-21
状态: 已接受（grilling Round 1 Q6/Q11/Q13 用户裁决"都按推荐"）

## 背景

用户提出 27 项 UI 优化。附件功能在 Rust 版完全缺失（leanote-core 无任何 file API）；Vim 模式有 ESC 失效与 visual 假选区两个 bug，且要求删除 `--NORMAL--` 状态行；accent 写死 Wake 蓝；历史版本现为编辑列内联展开。ADR-0004 已定 Vim 是键位层非引擎。

## 决策

1. **附件（Q6=B）**：本轮只做「查看 + 下载已有附件」（core 补 file 端点），上传（含粘贴图片）拆独立 issue。理由：上传链路（选文件→上传→插入正文→置脏）是独立工作量，与查看/下载不同风险面。
2. **Vim（Q11）**：删状态行后，模式反馈**只靠光标形状**：normal=block、insert=竖线（beam）、visual=block+选区高亮。同时修 ESC（insert→normal）与 visual 真选区——光标区分是删状态行的前提，visual 假实现是既有 bug 不是新需求。
3. **历史版本（Q7）**：居中大弹窗（列表+预览+恢复），不做独立 OS 窗口——多窗口状态同步（选中笔记切换、同步刷新历史）复杂度不成比例。
4. **主题色（Q13）**：accent 从写死改为 6 色板（蓝/紫/绿/橙/红/粉）本地偏好，即存即生效。仍属 ADR-0003 双材质体系的参数化，不恢复多主题。

## 后果

- Vim 模式不可见状态行后，新用户可能不知道自己在哪个模式——这是接受的成本（真实 vim 也无状态行依赖，光标形状是惯例反馈）。
- 附件只读期间，用户在本客户端内无法新增附件（可用旧客户端/网页版），issue 20 承接上传。

## 追记（2026-10-22）

用户报告“无法进入 vim 模式、光标不对、模式不对”。诊断：键路（capture_key_down 拦截裸键）经 gpui-pre 源码核实无误——裸可打印键无 kit 绑定，`prefers_ime_for_printable_keys` 为 false，KeyDown 先走完整 dispatch（capture 阶段最先），未消费才由 AppKit 落为文本。真实缺陷在执行层：

1. **光标不对**：`exec` 所有 normal 态收尾用 `goto`（折叠为细光标），违反本 ADR 的 block-caret 约定；`start_editing` 进入编辑时也从不设初值。修复：新增 `rest()`（normal/visual → `block_caret`，insert → `goto`），覆盖 Move/Delete/Yank/LineOp/LeaveVisual/Paste 全部收尾点，`start_editing` 在 vim 开启时直接选 `0..1`。
2. **模式不对**：`vim_consume` 只 notify 编辑器实体，模式标签挂在 shell 上，永不重绘。修复：consume 后 `cx.notify()`（shell）。
3. **测试盲区**：旧测试直接调 `vim_key_down`，真实 capture 链路从未被测过。经核实 `simulate_keystrokes` → `dispatch_keystroke` → `dispatch_event` 走的就是真实分发链（需先 `window.draw()`），pipeline 测试已改走该链路并断言 block-caret 语义（`selected_range`），另加编辑器进入时的 `0..1` 断言。

## 追记二（2026-10-22）：CJK IME 吞键与 vendored patch

上一轮修复后用户复测：mode 标签显示（`--NORMAL--`）但 esc 切模式失效、normal 模式按键直接输入文本、delete 无效。追根因更正上一轮结论：

- `ElementInputHandler::prefers_ime_for_printable_keys`（gpui-pre `input.rs`）默认委托 `accepts_text_input`，而 kit 的 `InputBaseState` 未 override → 恒 true。上一轮“prefers 为 false”的断言只在非 CJK 输入源下成立。
- macOS `handle_key_event`（gpui-pre-macos `window.rs`）：输入源为 CJK IME（拼音）且 prefers=true 时，可打印键先进 `inputContext handleEvent:`，被 IME 消费即返回，GPUI dispatch（含我们的 capture_key_down）根本不执行 → 字母变组合文本/直接插入 = "编辑模式混进 normal"。无 marked text 时 esc/方向键经 `do_command_by_selector` 回到 dispatch；有候选窗时第一下 esc 先关候选窗（IME 标准行为）。
- headless 测试走 `simulate_keystrokes`（内部 dispatch），不经过 `is_ime_input_source_active` 分支，故 48 测试全绿仍真机坏——此盲区无法在测试内消除，只能真机验证。

**决策**：`rust/vendor/gpui-pre`（8.3M，copy 自 registry 0.3.9），唯一改动 `input.rs` 的 `prefers_ime_for_printable_keys` → 恒 `false`，workspace 根 `Cargo.toml` 加 `[patch.crates-io]`。安全性论证：normal 模式 printables 走 dispatch（vim 消费 ✓）；insert 模式未消费时 macOS 分支回落 `inputContext handleEvent:`，拼音组合照常 ✓；`is_composing` 时仍 IME-first ✓；app 与 kit 无裸字母 chord 绑定，全局无副作用。修复走 patch 而非 app 层 workaround，因 app 无杠杆：`set_input_handler`/`take_input_handler` 未在 `Window` 公开，`InputModeKind` sealed。

**维护负担**：gpui-pre 升级需重新 vendor 并重打此补丁（一行）；上游若让 kit editor override prefers（Zed 语义），可撤 patch。

## 追记三（2026-10-22）：`^` motion 与空行 block caret

用户报告两个缺口：无 `^`（行首第一个非空白字符）；完全空的行上 normal 光标显示为 insert 竖条。

1. **`^`**：新增 `Motion::FirstNonBlank`（全空白行回落行首）与 `DeleteOp::ToFirstNonBlank`（`d^`/`y^`/`c^`），进 normal/visual 键表、`motion_of`、`operator_motion` 与 preferred-col 刷新列表。`^` 是 shifted 符号（US 键盘与 `$` 同类），无需 app_shell 映射。
2. **空行 block caret**：`block_caret` 的 `end = next_char(offset).min(line_end)` 在空行（`line_start == line_end`）折叠成空 range → kit `layout_selections` 跳过空 range，只剩细光标。修复：空行时不 clamp，选区覆盖 `\n`（`off..off+1`）——kit `layout_range_corners` 对 selected newline 画 one-space cell（element.rs 既有语义），即空行的 block caret。`x` 无需新 guard：`delete_range_for(Char)` 的 `caret >= line_end → None` 已使空行 `x` 为 no-op（真 vim 行为），测试已断言不并线。

## 追记四（2026-10-22）：block caret 原生化（vendored gpui-base）

用户报告：normal 模式同时显示块光标与 kit 的 2px 竖条（闪烁）；行尾 `\n` 被选中高亮。根因：gpui-base 0.7.3 无 caret-shape API，`element.rs` 无条件画竖条（`show_cursor` 为 `pub(crate)`，app 层无法关闭）；追记三的选区 hack 覆盖 `\n` 造成选中高亮副作用，且选区无法参与 kit 的 BlinkCursor 闪烁循环。

决策：vendor `gpui-base` 0.7.3（`rust/vendor/gpui-base`，`[patch.crates-io]` 指向，与 gpui-pre 同模式），加最小原生支持：

1. `InputBaseState` 新增 `block_cursor: bool` + `set_block_cursor()`/`block_cursor()`。`block_cursor` 时 `layout_cursors` 把 caret 宽度从 `CURSOR_WIDTH`(2px) 扩为下一 caret 位置与当前位置的 x 差（跨行/行尾回落一个 `space_width`，`.max(CURSOR_WIDTH)` 保底），paint 时以 45% 透明度覆盖字形（vim 风格，字仍可读）。闪烁复用 kit 既有 BlinkCursor（`show_cursor` 已含 blink 可见性）。
2. app 层 `vim.rs`：normal/visual 模式 `set_block_cursor(true)` + 折叠选区（`off..off`），insert 模式 `set_block_cursor(false)`（kit 竖条即 insert 光标）。删除追记三的"选区覆盖 `\n`"方案——空行 block 由 kit 块宽逻辑天然覆盖（`next.y != cursor_pos.y` → space_width），`\n` 不再被选中。会话开启（`start_editing`）与 vim 开关（`QuickToggleVim`）同步该标志。
3. 维护负担：gpui-base 升级需重新 vendor 并重放上述 diff（state.rs 三处 + element.rs 三处，均有 `leanote:` 注释标记）；测试从"选区断言"改为"折叠选区 + `block_cursor()` 断言"。

追记三第 2 点的空行语义由本追记取代（空行不再选中 `\n`）。

## 追记五（2026-10-22）：`D` 与 `{`/`}` 段落跳转

补充两条 normal/visual 命令：

1. **`D`**：删除到行尾——直接复用既有 `DeleteOp::ToLineEnd`（`d$` 的实现），只是键表新入口。
2. **`{`/`}`**：段落跳转（段落边界 = 空白行）。新增 `Motion::ParaBackward`/`ParaForward` 与 `para_forward`/`para_backward`（扫行找空白行；`}` 无下界时落 buffer 末尾——kit 对结尾 `\n` 的 virtual 行本身是空白行，与 vim 的末段停靠一致）。均为合法 operator 目标（`d}`/`y{`/`c}` 走 `ToParaForward`/`ToParaBackward`），进 normal+visual 键表与 preferred-col 刷新列表。`{`/`}` 是普通符号键（非 shifted），无需 app_shell 大小写还原。

注意：`}` 停在空白行上，`}}` 连按会先停在中间空白行、再停到下一段落边界——从段落中间开始时 `d}` 删到下一空白行（不含该行），测试断言了 `}}`/`{`/`d}` 三条路径。
