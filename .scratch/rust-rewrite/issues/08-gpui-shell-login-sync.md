# 08: GPUI 主窗骨架 — 内嵌登录 + 全量同步进度 + Wake 主题 + 中英 + 单实例

**What to build:** 用户启动 Rust app：未登录时主窗口右栏即登录表单（无独立登录小窗）；登录成功自动触发全量同步并显示进度；完成后呈现空三栏壳（左导航/中列表/右阅读面，固定宽度、无顶部通栏、macOS 44px 透明标题栏与左栏同材质）。Wake 主题 token（四层材质、8 级字号、4px 间距、分级圆角、强调蓝）、中英双语跟随系统、窗口尺寸位置持久、单实例锁（二次启动聚焦既有窗口）。对真实服务器或假服务器都可演示。

**Blocked by:** 03

**Status:** resolved

- [ ] 主窗内嵌登录成功后自动全量同步并显示进度，完成进入三栏壳
- [ ] 布局骨架：224px 左栏 / 336px 中栏 / 右栏剩余宽 720px 居中阅读列，无顶部通栏
- [ ] 明暗双材质跟随系统；Wake 色彩 token 就位
- [ ] 界面语言中/英跟随系统切换
- [ ] 窗口尺寸/位置持久并在显示器失效时回默认
- [ ] 二次启动聚焦既有窗口（单实例锁），托盘不在此票

## Comments

Merged into `integration/rust-rewrite` as merge commit `1cc6badb` (branch `ticket/08`, tip `9bb94d20`, work `6df3ea35`).

**Summary:** Added `rust/crates/leanote-app` GPUI crate. Login form rendered in the main window's right pane (no separate login window) with fakeserver autosubmit demo; successful login auto-triggers full sync with progress UI, then the empty 3-pane shell (224px left nav / 336px middle list / centered 720px reading column, no top bar, macOS 44px titlebar sharing left-pane material). ADR-0003 layout verified by pixel measurement: 224px/336px panes + 44px titlebar same material. Wake theme tokens (4 material layers, 8 type sizes, 4px spacing, tiered radii, accent blue) with dark/light following system; zh/en i18n following system; window size/position persisted with off-screen clamp to defaults; single-instance lock focusing the existing window on second launch; 8 unit tests. Smoke-run bugs fixed: render-time entity read panic, and bounds-recenter discarding saved position.

**Evidence:** `cargo test --manifest-path rust/Cargo.toml` — 69 tests passing (≥69 expected). Layout measurements: left pane 224px, middle 336px, titlebar 44px with same material as left pane. Single-instance behavior observed on second launch (existing window focused). `cargo build --release -p leanote-app` green.
