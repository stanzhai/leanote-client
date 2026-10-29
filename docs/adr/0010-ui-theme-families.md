# ADR-0010: UI 主题族——单一偏好同时重染材质与编辑器高亮

- 状态：Accepted（2026-10-28，主题功能轮裁决）
- 关联：修订 ADR-0003 第 3 条（视觉语言从单套 Wake 配色扩展为多族）；GLOSSARY「视觉语言」「设置」

## 背景

此前「编辑器配色」（`editor_palette` pref）只换源码高亮的 2–3 套颜色，UI 四层材质是另一套硬编码——两个体系各自维护，切配色时编辑器与外壳脱节，且 warm/solarized 的 editor 值是手抄副本，改一处漏一处。

## 决策

1. **族表单源**：新 `theme_family.rs` 定义六族 `ThemeFamily`（默认/豆沙绿 moss/暖棕 warm/Solarized/Nord/墨黑 ink），每族明暗两套 `FamilyColors`（base/list/background/popover 四层 + fg/muted_fg/border + editor 的 active_line）。材质 token、前景色、编辑器高亮主题（`highlight_json` 由族表生成 HighlightTheme JSON）全部从这一张表派生——不再有第二份手抄值。
2. **一个偏好值**：`Prefs.editor_palette` → `Prefs.ui_theme`（serde `alias = "editor_palette"` 使老文件 warm/solarized 原值迁入，`family_index` 直接命中同名族）。设置面板「外观」区 Combobox 下拉即切即生效；明暗仍随系统（族表明暗各一套）。
3. **默认族零解析**：`family_highlight` 对默认族直接返回 kit 内建 `HighlightTheme::default_dark()/default_light()`（与默认族同值），只有非默认族走 JSON 解析 + `Mutex<HashMap>` 缓存。
4. **accent 不入族表**：选中/动作色仍由用户主题色（ADR-0005 的 ACCENTS）决定，族表不提供第二 accent 源——一个窗口只有一个 accent 权威。
5. **不变量**：每族每明暗 base/list 带 alpha（半透明，叠在窗口底色上）、四层亮度严格递增（深度可读）、fg ≠ bg——由 `family_table_completeness` 测试锁定（纯表断言，不动全局 CURRENT_FAMILY，测试可并行）。

## 后果

- 修订 ADR-0003 第 3 条：视觉语言从「单套 Wake 配色」变为「Wake 骨架 + 六族皮肤」；无窗格边框、字号栅格等结构 token 不随族变。
- 实现注记：8 位 hex（带 alpha）必须走 `gpui::rgba()`——`rgb()` 会丢最高字节把 alpha 当噪音（曾是 bug，由上述测试捕获）。
- 运行时切换成本：一次 JSON 解析 + 缓存命中，`set_family` 后由 `apply_wake_theme` 重放全部 token 读取。
