# 14. 主题体系移植 Navop（ThemeRegistry + JSON 主题文件）

日期: 2026-11-04
状态: 已接受（用户裁决：参考 feigeCode/navop 的主题设计重构 Rust 客户端主题实现，把 Navop 的主题风格搬过来）

## 背景

ADR-0003/0010 的主题实现是代码内硬编码的 6 个「主题家族」表（`theme_family.rs`：每家族一组浅/深 hex + 手写语法 JSON），与 kit（longbridge gpui-component，本仓库称 gpui-kit）自带的 `ThemeRegistry` 体系脱节——kit 组件用注册表默认色，应用面板用家族表色，语法高亮再单独拼 JSON。用户要求把 Navop 的主题体系（软件内置多主题风格）搬过来：Navop 内置 23 个主题文件（39 个浅/深主题），经 gpui-component 的 `ThemeRegistry::load_themes_from_str` 注册，`ThemeConfig::apply_config` 统一着色，每主题自带 `highlight` 语法节。

## 决策

1. **主题文件原样入库**：Navop 仓库 `themes/*.json`（23 个文件）拷贝到 `rust/crates/leanote-app/themes/`，`include_str!` 进 `theme_registry.rs`（navop.json 排首，"Navop Light"/"Navop Dark" 为默认）。
2. **kit 注册表成为唯一着色源**：启动/换主题时把全部主题文件 `load_themes_from_str` 进 `ThemeRegistry`，按用户 prefs 选中 `theme.light_theme`/`dark_theme` 两个 config，模式切换由 kit `Theme::edit/apply_config` 加载——表面色、tab/table/status 等一切 kit 色彩、以及**每主题自带的语法高亮**（`config.highlight` → `theme.highlight_theme`）都由主题文件驱动；`apply_wake_theme` 瘦身为纯应用覆盖层（玻璃透明 background token、字体/缩放、圆角、零阴影）。
3. **四层材质改为注册表镜像**：`theme::material(layer, dark)` 不再读家族表，改读 `theme_registry` 解析出的同一 JSON（同样的回退规则：`sidebar ← background.blend(border@0.15)`、`list/popover ← background`）——应用自绘面板与 kit 组件同色由「同源」保证，不再靠覆盖写入。
4. **强调色跟随主题 primary（Navop 模型）**：默认（prefs `accent=""`）用当前主题的 `primary.background`/`primary.foreground`；用户点选色板（Issue 19 的 8 色）才覆盖（含 navop 的 hover `lighten(0.1)`/active `darken(0.1)` 配方与 `selection = accent@0.25`）；设置页强调色行最前为**彩虹圆圈**（六色扇形拼圆）= 跟随主题自带强调色（默认选中，白勾），其后 8 个纯色圆圈为手动覆盖。
5. **单一主题选项（2026-11-04 修订）**：prefs 保留单字段 `ui_theme` 存**主题名**——成对主题（"X Light"/"X Dark"）存族干 "X"，选择器也只列一行；外观模式自动匹配变体（浅色 ↔ "X Light"，深色 ↔ "X Dark"），单模主题（Matrix、Catppuccin Latte…）原名列出、仅服务其所属模式（另一侧回落 Navop）。解析顺序：精确名 → 兄弟变体 → Navop 默认。设置页只有一个「主题」下拉。迁移：老家族 id `solarized` → "Solarized"，`moss`/`warm`/`nord`/`ink` → 默认；中间格式的 `light_theme` 去掉 " Light" 后缀收编。
6. **删除**：`theme_family.rs`（家族表、`family_highlight`、手写语法 JSON、`set_family`）与 `apply_wake_theme` 里的表面色/滚动条/高亮覆盖全部移除——主题文件说了算。

7. **不动**：玻璃强度 pref（作用于 nav 面板 Base 色 alpha）、选区半透明约束（Issue 24 回归测试改为覆盖「跟随主题」与「自定义色板」两分支）、字体/缩放/圆角/间距（ADR-0003 体系）。
8. **默认主题深色整体提亮与侧栏分层（2026-11-05 修订）**：Navop Dark 原值整体过深且侧栏 `#1e1e1e` 距正文 `#171717` 仅 7/255，深色下整个窗口糊成一块平面。用户裁决（三轮）：① 分层参考 Navop 前家族表的「侧栏最深、亮度向内容爬升」；② 整体过深，锚点定为侧栏 `#262626`、正文/列表/阅读区 `#333333`；③ 编辑器面同为 `#262626`——深色收敛为两级灰阶（`#262626` 沉浸面：侧栏/list/table/编辑器，`#333333` 窗口面），层间步长统一 13。其余深色值按新锚点重排：popover/secondary `#3a3a3a`、hover `#474747`、active `#575757`、全局 border `#4a4a4a`、滚动条拇指 `#545454`；饱和色（primary/danger/success/warning/info）不动。④ **语法着色换主流方案**：Navop 原语法表只定义了 10 个通用 token（keyword/string/…），而 markdown 的 tree-sitter capture 词汇是 `title`/`emphasis(.strong)`/`link_uri`/`link_text`/`text.code.span`/`punctuation.*`——一个都对不上，编辑器里 markdown 几乎无着色。深色换 **One Dark**、浅色换 **GitHub Light** 标准配色，并把 kit 语法 schema 原生支持的 markdown token 全部补上（标题加粗着色、强调斜体/粗体、行内代码、链接、列表标记、围栏分隔符）。这是对「主题文件原样入库」的有意偏离，仅动 navop.json（深浅两侧的 colors+highlight），其余 22 个文件保持上游原样。

## 后果

- 正：主题数量 6 家族 → 39 个 Navop 风格主题（含 Catppuccin/Gruvbox/Tokyo Night/Solarized/Flexoki 等）；新增主题=放一个 JSON 文件 + 一行 include_str；语法高亮随主题自动切换；应用面板与 kit 组件色彩永远一致。
- 负：浅色主题仅 10 个（Navop 主题集偏深色）；`theme::material` 依赖注册表静态快照（启动时选定，进程内原子更新）；旧家族（豆沙绿/暖色/Nord/墨）不再提供（映射到默认）。
- 测试：家族表完备性测试改为注册表完备性测试（39 主题、默认项、回退、fg/bg 对比、primary 不透明）；apply_wake_theme 测试改为新签名 + 双分支选区断言。

## 依据

用户裁决（2026-11-04 第二轮澄清：不是加一个 Navop 主题，而是移植其主题设计）；Navop 事实：`themes/*.json`、`crates/core/src/themes.rs`（ThemeRegistry 加载链、ACCENT_LIGHTNESS_THRESHOLD、custom accent 覆盖模型）；kit 事实：`gpui-component/src/theme/registry.rs`（load_themes_from_str / ThemeSet）、`schema.rs`（ThemeConfig.highlight + surface 回退规则）、`theme/mod.rs`（Theme::edit 模式切换重载 config）。
