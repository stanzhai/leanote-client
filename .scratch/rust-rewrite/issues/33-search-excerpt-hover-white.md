# 33: ⌘K 搜索结果摘要 hover 变白

**What to build:** ⌘K 快捷面板搜索结果行,hover/选中时标题已随强调色变白,但摘要行仍是灰色(muted),在强调色背景上对比不足。摘要也要变白。

**Root cause:** 上一轮(issue 31)修标题时,摘要保留了显式 `.text_color(muted)`——显式色覆盖行级继承色,hover 时行的 accent_foreground 到不了摘要文本。

**Change** (`search_overlay.rs` 结果行):删摘要的 `.text_color(muted)`,让它和标题一样纯继承(常态 foreground、hover/选中 accent_foreground=白)。层级区分只剩字号(标题 13 / 摘要 12)。`render_dialog` 外层的 `muted` 绑定保留(empty 提示与 footer 仍在用)。

**Verification:** `cargo test -p leanote-app` 67 绿;fake server 冒烟无 panic。视觉确认(强调色上摘要呈白色)留给用户。

**Blocked by:** (无)

**Status:** resolved (2026-10-27)
