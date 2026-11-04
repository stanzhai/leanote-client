# 主题体系移植 Navop

Status: resolved

## 目标

把 Navop（feigeCode/navop）的主题体系搬进 Rust 客户端：ThemeRegistry + JSON 主题文件 + 浅/深双主题名 + 每主题自带语法高亮，替代代码内硬编码的 6 家族表。决策记录见 `docs/adr/0014-navop-theme-system.md`。

## 落地

- `rust/crates/leanote-app/themes/`：Navop 的 23 个主题文件原样入库（39 主题）。
- `src/theme_registry.rs`（新）：解析入库主题（镜像 kit 回退：sidebar ← bg.blend(border@0.15)，list/popover ← bg），向 kit `ThemeRegistry` 注册，持有当前浅/深选择与 `MODE_DARK`。
- `src/theme_family.rs` 删除；`theme.rs` 的 `material()/foreground()/border()/glass_base()` 转由注册表驱动；强调色默认跟随主题 primary（设置页彩虹圆圈），8 个纯色圆圈为手动覆盖。
- `main.rs` 启动链：注册 → 选 config → 按外观 pref `Theme::change` → `apply_wake_theme`（瘦身为玻璃 token/字体/缩放/圆角）。
- `app_shell.rs`：设置页单个「主题」下拉（成对主题合并为族干行）；`set_ui_theme(name)` 解析双模 config + 重放覆盖层。
- `prefs.rs`：单字段 `ui_theme` 存主题名；老家族 id 与中间双字段格式在 load 迁移。

## 验证

`cargo test -p leanote-app --bins`：94 通过（注册表完备性、prefs 迁移、选区双分支、玻璃 reconcile、accent 跟随）。dev 二进制启动冒烟通过；视觉确认需本机打开应用（终端无屏幕录制权限）。
