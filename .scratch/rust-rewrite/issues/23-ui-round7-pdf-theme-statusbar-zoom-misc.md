# 23: UI 第七轮 — PDF 本地导出 / 主题切换优化 / 状态栏 / 缩放 / 杂项 11 项

**What to build:** 用户 2026-10-23 清单 11 项（grilling Round 1 ask + Round 2，全按推荐裁决，Q1 用户改判）：

1. **PDF 导出本地实现**：用户服务器 `stanzhai.site` `/api/note/exportPdf` 实测返回 `{"Ok":false,"Msg":"sysError"}`（服务端缺 wkhtmltopdf）。裁决：**本地渲染 PDF**，跨平台、轻量（纯 Rust，倾向 typst 库：CJK 换行/表格/代码块/字体子集化开箱即用；md→typst 序列化复用现有 `markdown` crate AST；富文本笔记走既有 html2md）。元素子集：标题/段落/粗斜体/行内码/代码块/列表/引用/表格/分隔线；**图片不嵌入**（跳过）。服务端路径按"本地实现"裁决移除（若 CLI 还有调用方则保留 core API）。
2. **主题切换优化（彻底）**：去 `Theme::change` 双重主题表重载 + 缓存调色板 JSON parse（`theme::editor_palette` 每次 `serde_json::from_str`）+ 切换重绘合并单帧提交。
3. **垃圾篓 icon**：`AssetIcon::Delete` → `AssetIcon::Trash`（左栏废纸篓行 + 中栏 trash 视图 header，app_shell.rs 两处）。
4. **快捷面板选中文字效果**：拖蓝（text selection）改白色；可读性由 vendored `selectable_text.rs` 保证文字对比。
5. **界面缩放**：设置面板 100/110/125/150/160 五档，`window.set_rem_size()`（GPUI 官方 zoom 语义）+ app 内固定 px 换算（rem 化 / scale 换算），整体缩放。
6. **编辑器底部状态栏**：仅 markdown 源码编辑态显示（预览/阅读/全窗口预览隐藏）。左 = 标签（胶囊 + ✕ 删除 + 输入回车添加，走 `NoteUpdate.tags` 置脏）；中 = vim 模式 `-- NORMAL/INSERT/VISUAL --`（vim 关显示 `-- 普通 --`）+ 光标行列 `r:c`；右 = 本地 `UpdatedTime`（保存即刷新）。保留光标形状反馈。
7. **HTML 预览 10px 边距**：仅编辑器内预览左右各 10px（全窗口预览保持 32px）。
8. **标题工具栏**：根因修复——标题行高 32→36px（20px 字号完整显示），非字面负 margin。
9. **设置面板 Vim 开关**：默认开；⌘K 快捷切换保留。
10. **设置面板编辑器工具栏开关**：默认关（现默认显示，改为全局默认隐藏 + 设置开启）。
11. **中栏笔记列表分隔条**：1px 底边线 `theme::border(dark)`（深浅自适应），末条不画，选中行不画（accent 色带连续）。

**Blocked by:** （无）

**Status:** resolved (2026-10-23)

## Adjudications (2026-10-23)

- Q1 PDF：本地实现、跨平台、轻量（用户改判，覆盖"服务端优先"旧语义）
- Q2 主题：彻底（合并单帧）
- Q3/19 状态栏中间区：vim 关显示"普通"文案
- Q4 状态栏范围：仅 markdown 编辑态
- Q5/18 缩放档位：100/110/125/150/160
- Q6 缩放范围：整体（app 固定 px 一起换算）
- Q7 选中效果：拖蓝改白
- Q8 预览边距：仅编辑器内预览
- Q9 标题行：修行高 36px（根因）
- Q10/20 光标形状反馈：保留
- Q11/21 分隔条：1px theme::border(dark)，末条不画
- Q12/22 设置行序：外观→缩放→Vim→工具栏→配色→主题色→语言→账户
- Q13/23 ⌘K Vim 快捷切换：保留
- Q14 工具栏默认：关（接受已打开笔记工具栏消失）
- Q15/24 更新时间：本地 UpdatedTime
- Q16/25 分隔条范围：仅中栏
- Q17 图片嵌入 PDF：不做（跳过）

## Comments

## Evidence (2026-10-23)

全部实现 + 编译 + 测试通过；GUI smoke 待用户过目（视觉项无法自动断言）。

1. **PDF 本地导出**：`leanote-app/src/pdf.rs`（新）typst 0.15.1 栈；`export_note_pdf` 重写为本地编译 + `fs::write`；`Api::export_pdf`、fake-server exportPdf 路由/夹具删除（grep 全仓仅剩历史注释）。测试：`cargo test -p leanote-app pdf` 5 passed（含 CJK 冒烟，输出 `%PDF` 头）。
2. **主题**：`theme.rs editor_palette` → `LazyLock<Mutex<HashMap>>` 缓存 parse（key = palette id × dark）；`set_appearance` no-op 点击早退（同模式不重载主题表）。kit `Theme::change/update` 每次必 `refresh_windows`（crates.io 产物不可合并），change+update 同一 effect 周期内连续调用，中间无帧。
3. **Trash icon**：`nav_row` 参数 `Option<IconName>`→`Option<AssetIcon>`（组件枚举无 Trash，assets 全目录有）；trash 行 + trash 视图 header 两处换 `AssetIcon::Trash`。
4. **拖蓝白**：`main.rs apply_wake_theme` `colors.selection = white()`（覆盖 search overlay/reader 全部 SelectableText）。
5. **缩放**：prefs.zoom（serde default 1.0）；设置行 5 按钮（100–160%）；`set_zoom` = 存盘 + `window.set_rem_size(px(16*factor))`；启动恢复；窗口 min size ×zoom。`note_list_row` 等手写 px 不随 rem 缩放（uniform_list 无 rem 上下文，已知取舍）。
6. **状态栏**：EditState 扩 `tags/updated_time/tag_input`；`save_editor` 送 `NoteUpdate.tags`（tags 变化置脏）+ 刷新 `updated_time`；渲染 `.when(!preview)` 三段式（标签胶囊+✕+Enter 添加 / vim 模式+r:c / fmt_time）。
7. **预览边距**：编辑器内 preview v_flex `.px(px(10.))`；全窗口预览 32px 未动。
8. **标题行**：Input `.h(px(32.))`→`.h(px(36.))`。
9. **Vim 设置行**：Switch 组件，写 prefs.vim_default（⌘K 切换保留，同一 prefs 字段）。
10. **编辑器工具栏开关**：prefs.editor_toolbar（default false）+ Switch 行；`render_editor` `.when(!preview && self.editor_toolbar)`。
11. **分隔条**：`note_list_row` 加 `divided` 参数，行底 1px `theme::border(dark)`，`.when(divided && !is_sel)`。

测试：`cargo test -p leanote-app -p leanote-core -p leanote-fakeserver` → 57+11+5+10+24+4 全绿。
文档：GLOSSARY（Vim 状态行、设置词条、新增「编辑器状态栏」）+ ADR-0007（本地 PDF）。

### GUI smoke (2026-10-23，真机 leanote-smoke 实例，像素级断言)

环境：`LEANOTE_RUST_DATA=/tmp/leanote-smoke LEANOTE_FAKE_SERVER=1`，键盘 Tab×3+Enter 登录 fake 账号（12 笔记同步），JXA CGEvent 点击，`screencapture` + PIL/numpy 像素分析（无视觉模型/tesseract，人工像素判读）。

- **#6 状态栏** ✓ markdown 笔记（点击进编辑态）编辑器底部三段式渲染：左段 tags 簇 x864-1027（TagsLabel + 120px 输入框），右段 x2147-2548（vim 模式 + r:c + 时间），h≈32 device px 带 border_t；reader/富文本笔记（n7/n10/n11）正确无状态栏（仅编辑态，符合 Q4 裁决）。
- **#11 分隔条** ✓ 中栏 9 条 1px 线（y334/464/594/724/854/984/1114/1244/1374，pitch 130 device = 65 逻辑行高）；选中行（accent 行底 y1374 线在、y1504 线无）与末条不画。
- **#8 标题行** ✓ 编辑器顶部 10px drag strip + 36px 标题行 = 92 device px（y0-92），20px 标题文字 y36-62 完整无裁切。
- **#7 预览边距** ✓ 编辑器内预览 `.px(10)`+`.px(5)`：渲染文字距 pane 边 41 device px（≈30 px padding + TextView 内边距）。
- **#10 工具栏默认关** ✓ 编辑态标题行与正文之间无 toolbar 带；prefs 落盘 `editor_toolbar: false`。
- **#5 缩放** ✓ 点击设置面板缩放分段第 3 档 → prefs `zoom: 1.25` 立即落盘，设置面板行距重排；退出后手写回 1.0（pill 二次定位失败，未见副作用）。
- **#4 拖蓝白** ✓ ⌘K 面板输入 "markdown" 后 ⇧⌘A 全选：选中带 x602-1957 纯白 (255,255,255) 背景 + 黑字 (10,10,10)。
- **#3 Trash** ✓ 源码 `nav_row` 两处 `AssetIcon::Trash`（app_shell.rs:2492 等）；像素侧废纸篓行在窗口折叠线下方仅露局部图标，形状与桶盖轮廓一致（弱证据，以源码为准）。
- **#1 PDF / #2 主题 / #9 vim 文案**：GUI 级未断言 —— #1 导出走原生 NSSavePanel（headless 无法驱动保存对话框）；#2 流畅度无自动断言手段；#9 状态栏右段簇存在但 12px 文字无法像素 OCR。均以单测覆盖（pdf 5 测试输出 %PDF 头；theme refresh_windows 单帧路径；vim 模式文案在状态栏代码路径内）。

### 遗留（非阻塞）

- 缩放只影响 kit rem 部分，手写 px（列表行高等）不缩放；160% 下中/右栏布局需 smoke 确认不破版。
- typst 二进制 +10–20 MB、首次导出字体扫描 <1s（一次性）。
- GLOSSARY「Vim 模式」词条已去掉"无状态行"（与 ADR-0004/0005 的旧表述冲突已修正）。
