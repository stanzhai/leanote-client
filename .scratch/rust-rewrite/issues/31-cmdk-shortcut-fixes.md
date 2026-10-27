# 31: 四项 ⌘K 面板/快捷键修复 — ⌘, 失效 / hover 黑字 / 宽度过宽 / kbd 提示错平台

**What to build:** ① ⌘, 无法调出设置（应全局生效）；② 浅色下 ⌘K 搜索结果行 hover/选中时文字仍是黑色（应为白色，与快捷操作行一致）；③ ⌘K 面板太宽、行太紧凑；④ 快捷操作右侧的按键提示显示 ⌃N（Ctrl 符号），macOS 应显示 ⌘N。

**Root cause:**

1. **⌘, 失效**：binding 字符串写的 `"cmd-"`——`Keystroke::parse` 把结尾 `-` 后的空 component 吃掉，key 解析为 `""`（probe 验证：`parse("cmd-") → key=""`），永远匹配不上真实按键 `,`。Zed 惯例是 `"secondary-,"`（secondary = macOS ⌘ / 其他平台 Ctrl）。
2. **hover 黑字**：上一轮的"删显式 text_color 靠继承"方案**无效**——kit `Label` 的 `RenderOnce` 自带 `.text_color(cx.theme().foreground)`（label.rs:211），`refine_style` 只会再叠加自己的色，永远覆盖行级继承色。gpui 里真正继承的是**纯文本节点**（`div().child(str)`），文字样式走 style refinement 继承链。
3. **kbd 提示错**：每个手势注册了 cmd-x + ctrl-x 两条；keymap 反序遍历取"最后注册"的 binding（keymap.rs `bindings_for_input`），⌘K 面板的 `Kbd::binding_for_action_in` 取到 ctrl-x → macOS 显示 `⌃N`。
4. **宽度**：ADR-0003 定的 680px 在 13–14" 屏上占大半窗口；行内 `py_1` 与 kit 行自带 `py_1p5` 叠加过挤。

**Changes:**

1. `main.rs` bind_keys 全部改为 `secondary-` 单条绑定（k/t/s/n/,、shift-p/f），一条覆盖双平台。顺带修好 ④：每 action 只剩一条 binding，`Kbd::format` 在 macOS 输出 `⌘N`/`⌘S`/`⌘,`（probe 验证）、其他平台 `Ctrl-N`。注释记录 `"cmd-"` 的 parse 坑。
2. `search_overlay.rs` 自定义结果行：三个 `Label` 全部换成纯文本节点（`gpui_kit::div().child(SharedString)`）；标题/笔记本/日期继承行色（选中/hover 随行变白），摘录保留显式 muted（次要信息恒定可读）。标题 14→13px。删内层 `py_1`（kit 行已有 py_1p5，叠加导致过挤）。
3. `OVERLAY_W` 680→560，ADR-0003 决策 5 同步修订（注明日期与理由）。
4. 测试：`cmd_n_creates_note` 改用 `secondary-*` 绑定并追加断言——`simulate_keystrokes("cmd-,")` 后 `window.has_active_dialog(cx)` 为 true（锁住 ⌘, 修复）。

**Verification:** `cargo test -p leanote-app` 66 全绿。probe（临时 example，已删）验证：`Keystroke::parse("cmd-") → key=""`（旧行为）、`Kbd::format(secondary-n/s/,/k)` → `⌘N/⌘S/⌘,/⌘K`。冒烟：fake server 启动 15s，12 笔记全量同步、账户 IsActive=1、无 panic。hover 白字与面板宽度为视觉项，待用户确认。

**Blocked by:** （无）

**Status:** resolved (2026-10-27)

## Notes (2026-10-27)

- `secondary-` 是 vendored gpui-pre（keystroke.rs:143）支持的修饰符，macOS 映射 platform、其他映射 control。`Kbd::format` 对 platform 输出 `⌘`、对 control 输出 `⌃`（macOS）——所以绑定写法直接决定提示符号。
- kit `Label` 与纯文本节点的本质区别：Label 是组件（自带默认样式），文本节点是样式化 div 的 child（完全继承）。任何需要"跟随父级变色"的行内文本都不能用 Label。
- keymap 的"最后注册优先"（`enumerate().rev()` 取首个匹配）意味着**同一 action 的多条绑定，只有最后一条会出现在 Kbd 提示里**；但 dispatch 时多条都可触发。想提示正确就别注册重复手势。
