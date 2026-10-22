# 19: UI 第五轮 — 左栏导航重构 / 中栏搜索+懒加载 / 右栏编辑器布局 / 设置主题色

**What to build:** 用户 2026-10-21 清单 27 项（grilling Round 1 Q1–Q14 全按推荐裁决）：

## 左栏

1. 顶部删 Leanote 名称 → 搜索按钮（开现有 ⌘K 面板）。
2. 导航图标加大（≈文字大小 14px），图标与文字留间距（gap 8px）。
3. 笔记数量右对齐到行最右 — 与 4 同根因（行缺 `w_full`）。
4. 笔记本选中整行背景（含缩进；`nav_row` 根加 `w_full()`，缩进从 `ml` 改为 padding 或内嵌 spacer）。
5. 标签改为按 notes.Tags 列聚合去重（以笔记为准；tags 表仅同步数据不驱动左栏）。
6. 底部设置按钮 icon 不显示 — 修复（AssetIcon Cog 渲染路径）。
7. 拖左分隔条只挤压右栏；中栏分隔条照旧（两根保留），中/右栏弹性吸收。
8. 「全部笔记」→「最近」：最近编辑 20 条（UpdatedTime 倒序）；「全部笔记」入口永久移除。
9. 废纸篓移到标签分组下方（低频）。
10. 持久化左栏状态：选中节点 + 笔记本/标签分组折叠态 + 最后选中笔记 id；滚动位置不记。启动恢复。
11. 左栏磨砂半透明：窗口 `WindowBackgroundAppearance::Blurred` + 左栏材质带 alpha；效果差则回退纯色并移除开关。

## 中栏

12. 笔记行 `w_full()` 撑满列宽，随栏宽自适应（uniform_list 行不 stretch 的根因修复）。
13. 标题超长截断（已有 line_clamp(2)，随 12 生效到列宽）。
14. 行内三段：标题 / 笔记本+更新时间 / 摘要一行截断（新增摘要：正文纯文本前 ~120 字符）。
15. 懒加载：列表状态只存轻量 meta（不含正文 String），正文选中时才取 — 消灭每次 render 全量 clone 正文。
16. 列表顶部工具栏加上边距（列容器 pt ≈ 44–48px）。
17. 中栏搜索框：header 下 Input，即输即搜（150ms 防抖、IME 安全、复用 `Store::search`），清空恢复视图，回车跳第一条。

## 右栏

18. 布局重构：第一行 = 标题（左） + 工具按钮（右，仅 icon）：预览、附件、编辑历史、全屏。
19. 删 `--NORMAL--` 状态行。
20. Vim 默认开启（`vim_default: true` 默认），无 vim 开关按钮。
21. 历史版本居中大弹窗（列表+预览+恢复），不再内联展开。
22. 内容区域撑满面板（删 READER_W 720px；左右 padding 48px，无最大宽）。
23. Vim 修复：ESC 正确回 normal（insert 分支 esc→`VimAction::SetMode(Normal)`）；normal=block 光标 / insert=竖线 / visual=block+选区；visual 模式真选区（motions 扩展 selection，d/x/y 按选区操作）。
24. 标题下工具栏：markdown 格式按钮（B/I/H1-3/行内码/代码块/链接/列表/引用/表格/分隔线）+ 右侧帮助按钮（语法速查弹窗）。对选中文字包裹语法，无选中插占位。

## 附件（Q6=B）

25. 本轮：查看笔记附件列表 + 下载已有附件；上传（含粘贴图片）拆独立 issue 20。core 需补 file 端点（`file/getAttach` 等，参考 Electron `src/file.js` 21KB）。

## 设置

26. 浅色主题修复：自绘面板 `dark()` 改读 Theme.mode（非 window.appearance），或调 `App::set_window_appearance` 统一 — 全 APP 一致明暗。
27. 主题色：设置加 accent 色板 6 色（蓝 `#3b82f6`/紫/绿/橙/红/粉），点选即存即生效（prefs.accent）。

**Blocked by:** （无）

**Status:** resolved
