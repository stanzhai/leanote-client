# 3. 信息架构采用 Wake 三栏，替代现有 leanote 布局与多主题体系

日期: 2026-10-19
状态: 已接受（Q4=B、Q7=B、参考 iAmCorey/Wake）

## 背景

现 Electron 版布局为 leanote web 版移植：左笔记本树 + 中列表 + 右编辑器，11 个 LESS 主题、5 语言。用户要求布局风格参考 Wake（iAmCorey/Wake，Rust+GPUI）。Round 1 曾推荐“结构沿用 leanote、Wake 只出视觉”（Q4=A），用户裁决 **Q4=B：信息架构照 Wake**。

## 决策

1. **信息架构 = Wake 骨架**：左栏固定宽导航（顶部搜索入口 + 固定导航 Recent/Starred + 可折叠分组 Notebooks/Tags + 底部 Trash 行 + 工具栏）；中栏笔记列表（行内三段：标题 / 笔记本+更新时间 / 摘要一行截断）。无顶部通栏；macOS 44px 透明标题栏与左栏同材质。
2. **视觉语言 = Wake 规范**：四层材质色（titlebar/sidebar、list、background、popover）、无窗格边框线、持久 UI 零阴影、系统字 + Menlo 等宽、8 级字号、4px 间距栅格、分级圆角（面板 12 / 选中 8 / 按钮 6 / 徽标 4）、强调色仅用于选中与动作。跟随系统深浅色，另设**可配置主题色**（6 色板，2026-10-21 第五轮裁决）。macOS 左栏材质磨砂半透明（窗口 Blurred + sidebar 层 alpha）。
3. **多主题体系不迁移**：11 个 leanote 主题被 Wake 式明/暗双材质取代（对应映射：pebbles→浅色）。i18n 只保留中文与英文两种界面语言（Q12 裁决；de/zh-hk/ja 移除）。
4. 布局实现方式参考 Wake：裸 `h_flex/v_flex` + 常量宽度，不引入 dock/panel 框架；组件库用 gpui-kit（即 longbridge `gpui-component`，Wake 锁定 rev `fd3bc2b`）。
5. ⌘K 全局搜索 overlay（Wake 同款：输入框+列表+脚注）承载全局搜索交互；中栏另有内嵌搜索框（即输即搜，作用于当前列表）。宽度 680px → **560px**（2026-10-27 修订：13–14" 屏上 680 占掉大半窗口，收窄为浮动面板观感）。
6. **第五轮修订（2026-10-21）**：「All Notes」入口永久移除，代之以「Recent」（最近编辑 20 条）——浏览全部走笔记本/标签分组；Trash 降频移至标签分组下；右栏内容区不再固定 720px 阅读列，撑满面板（左右 48px padding）；拖左分隔条只挤压右栏，中栏由自身分隔条调节。

## 后果

- 正：与参考实现同构，样式可以直接对照 Wake 源码调；删掉 11 主题的 LESS 维护面；Wake 的布局代码模式（单 workbench、材质分层）可复制。
- 负：信息架构从“笔记本树在左栏”变为“笔记本/标签是左栏的折叠分组”，老用户有迁移学习成本；多主题用户失去旧主题。
- 右栏编辑器自第五轮起为撑满布局；720px 阅读列决策已被第 6 条修订取代。

## 依据

Q4=B 用户裁决；Wake 事实（`agent://MapWakeLayout`：workbench.rs 结构、theme.rs 材质、DESIGN.md 规范）。
