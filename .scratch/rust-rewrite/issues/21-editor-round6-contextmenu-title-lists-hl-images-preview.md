# 21: UI 第六轮 — 右栏编辑器: 右键菜单/标题/列表自动延续/高亮/图片拖放/全窗口预览

**What to build:** 用户 2026-10-22 清单 10 项（grilling Round 1 Q1–Q10 + Round 2 Q5 全按推荐裁决）：

1. 删 Done 按钮；星标、导出（HTML/MD/PDF）移入笔记列表行右键菜单（`note_row_menu` 扩展）。接受无显式"退出编辑"（选中另一笔记即退出，ADR-0004）。
2. 标题输入框字号放大至 20px（固定值，不做成设置项）。
3. 标题行距窗口顶部 10px：编辑态删独立 `reader_drag_strip`，标题行两侧留白兼作窗口拖拽区；阅读态保持现状。
4. 列表自动延续：回车延续 `1. `/`- `/`* `/`+ `；任务项 `- [ ]`/`- [x]` 延续为 `- [ ]`；引用 `>` 延续。空列表项/空引用回车取消。
5. markdown 自动补全包：成对符号自动闭合（`` ` ``/`**`/`~~`/`[`/`(` 输入时补另一半，光标居中；有选区直接包裹）、围栏代码块（```` ``` ```` 自动补收尾围栏并定位到 info string）、Tab/Shift-Tab 列表项增减缩进。vim 激活时禁用成对闭合与围栏补全（缩进保留）。`$`/`$$` 配对不做（随 LaTeX 推迟）。
6. 删底部 vim 状态行（`mode_label`/`cmdline` 标签）；`:w`/`:q` 仍可用，无替代显示。vim 本体不动。
7. 语法高亮：启用 kit 全部 ~35 个 tree-sitter 语言 feature（markdown `injections.scm` 使编辑器内嵌代码块同享按需着色；`from_name` 别名 sh/cs/scss 自动生效）。
8. mermaid/LaTeX **推迟**，单开 issue 22（无原生 GPUI 渲染器；候选方案 A/B/C 已在烤问中记录）。
9. 图片拖放：拖入编辑器 → 复制到用户 images 目录 → 插入 `leanote://file/getImage?fileId=<24hex>`；工具栏加插入图片按钮（`prompt_for_paths` 选文件）。同步：images 表 + `update_note` multipart FileDatas 推送 + 服务器 Files 回写 ServerFileId + fake-server 端点 + 预览经 `image_source` 解析 `leanote://`。不做剪贴板粘贴。
10. 全窗口预览（新）：⌘T 应用内全窗口渲染预览，右上角关闭 icon，ESC/再按 ⌘T 退出，编辑态/阅读态均可用。删现有 OS fullscreen 按钮。原"全屏 (Focus mode)"（⇧⌘F）更名"专注模式"，行为不变。GLOSSARY 相应拆分。

**Blocked by:** （无）

**Status:** resolved

## Comments

2026-10-23 完成全部 10 项：

1. `note_row_menu`（app_shell.rs）加 星标/导出 HTML/MD/PDF；Done 按钮与 `editing_done` 路径删除。
2. 标题输入框 20px 固定。
3. 编辑态删 `reader_drag_strip`，标题行两侧留白作拖拽区，距顶 10px；阅读态 28px 不变。
4. 列表/任务/引用回车延续（`markdown_input.rs` `continue_list_on_enter`），空项回车取消。
5. `markdown_input.rs`：成对闭合 + 围栏补全 + Tab/Shift-Tab 缩进；vim 激活时禁用闭合/围栏（缩进保留）。
6. vim 状态行（`mode_label`/`cmdline`）删除，`:w`/`:q` 保留。
7. kit `tree-sitter-languages` 聚合 feature（`language_config.rs` 注入）。8. mermaid/LaTeX → issue 22。
9. 图片链路闭环：core `images` 表 + `image_ids_in_content`（id 字符集含 `_`）；sync `update_note`/`add_note` multipart 上传 + `apply_files_mapping` 回写 ServerId + 新 `Api::get_image` + `pull_missing_images`（full_sync/incr_sync 尾部拉取缺失字节）；fake-server multipart 解析 + `/api/file/getImage` + `set_images`/`contents`/`notes` accessor；app `attach_images`（文件选择/拖放）+ `browse::resolve_note_images`（data URL 渲染，reader/preview/full preview 三处）；demo seed n12。
10. ⌘T 全窗口预览 + ESC/✕ 退出；OS fullscreen 按钮删除；⇧⌘F 更名专注模式。

验证：`cargo test -p leanote-core -p leanote-cli -p leanote-app -p leanote-fakeserver` 全绿（新增 `incr_push_uploads_dirty_images_with_note`、`full_sync_pulls_referenced_image_bytes`、`resolve_note_images_swaps_known_ids_only`）；真机 smoke（LEANOTE_FAKE_SERVER=1）：GUI 进程稳定运行、fake-server 登录、`file/getImage?fileId=srvimg_demo` 返回 image/png 8 字节、n12 内容含 getImage 链接。
