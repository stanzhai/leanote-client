# 32: ⌘T 全屏预览阅读边距优化

**What to build:** ⌘T 全屏预览只显示一篇笔记,原文 `.p(32)` 四边均等且无宽度上限,全屏窗口上文字铺满整行,阅读不友好。要左右大间距,窗口越宽留白越多。

**Root cause / decision:**

- 原布局:`v_flex().size_full().p(px(32.))` — 固定 32px,2560 宽全屏时正文一行 2496px。
- 两参数相乘的方案(padding × 上限):
  - 内容列 `max_w(1120)` + `mx_auto`:窗口超出 1120 时两侧自动留白((窗口宽-1120)/2),这是"越宽留白越多"的实现;
  - 列内左右 padding 48px:即使窗口 < 1120(普通分屏)也有舒适边距。
- 顶 24(只有关闭按钮)/ 底 48(滚动末端留白)分离,不再四边均等。

**Changes** (`app_shell.rs` ⌘T 分支):

```rust
v_flex().id("full-preview")
    .track_focus(&self.preview_focus)   // ESC dispatch 依赖,勿删(见下)
    .debug_selector(|| "full-preview".into())
    .size_full().pt(24).pb(48).px(48)
    .max_w(px(1120.)).mx_auto()
```

**踩坑记录:**

- `.id("...")` 不注册 debug selector,`debug_bounds("full-preview")` 返回 None;必须显式 `.debug_selector()`(对照 nav-pane 的做法,app_shell.rs:2985)。
- 加 `.debug_selector()` 时误删了 `.track_focus(&self.preview_focus)` → `full_preview_toggles` ESC 断言失败。preview_focus 是 ESC(capture_key_down 冒泡路径)的 focus 节点,丢了它 key 事件根本到不了 root handler。教训:改 preview 元素链时 track_focus 必须保留。

**Verification:**

- 新测试 `full_preview_keeps_reading_margins`:1920 宽窗口 ⌘T 后 `debug_bounds("full-preview")` 断言列宽 ≤1120 且左右留白 ≥48 — 锁住"不铺满"。
- 顺带修了 `full_preview_toggles` 里遗留的 `"cmd-t"` 绑定(应 `secondary-t`,macOS 测试环境 cmd-t 解析不出 platform 修饰)。
- `cargo test -p leanote-app` 67 全绿(browse.rs 的 `t` 警告预存在,非本轮)。
- 冒烟:fake server 启动,12 笔记同步,无 panic。

**Blocked by:** (无)

**Status:** resolved (2026-10-27)

## Notes

- 1120 内容宽(1120-96=1024px)仍偏宽,注释里"880px column"与实现不一致。若用户觉得还不够收,下一档:max_w 收到 920 左右(内容 ~824px,66-75 字符行宽舒适区)。
- `mx_auto` 在 resizable_panel(非 flex 居中父级)下实测生效(新测试断言左右留白对称通过),无需 h_flex 包裹。
