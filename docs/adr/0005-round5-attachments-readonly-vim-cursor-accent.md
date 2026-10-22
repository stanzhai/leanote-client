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
