# 6. 第六轮 UI：图片随笔记 multipart 推送、全窗口预览与专注模式拆分、语法高亮全量、markdown 自动延续

日期: 2026-10-22
状态: 已接受（grilling Round 1 Q1–Q10 + Round 2 Q5，用户裁决"全按推荐"）

## 背景

用户提出编辑器 10 项优化。三项决策超出既有 ADR 覆盖：

1. **图片**：ADR-0005 把上传推迟到 issue 20，但 issue 20 转向了缺陷修复，上传至今缺位。拖放图片要求完整语义（Electron `src/file.js`）：本地 images 目录 + `leanote://` URL + 脏图片随笔记推送，服务器回写 ServerFileId。
2. **"全屏"一词二义**：GLOSSARY 的"全屏 (Focus mode)"（⇧⌘F 隐藏左右栏）与新需求（⌘T 应用内全窗口渲染预览）是正交模式；现有 `fullscreen` 按钮误触 OS 原生全屏，正是用户报告的 bug。
3. **语法高亮**：kit 已按需惰性解析（`component_code_block_highlighter` 每语言缓存），缺的只是 feature 开关——markdown `injections.scm` 使编辑器源码视图内嵌代码块同样获得注入着色。

## 决策

1. **图片完整链路（Q9）**：images 表（`LocalId`/`ServerId`/`Bytes` BLOB，v2 migrate）+ `leanote://file/getImage?fileId=<24hex>` URL + 渲染前 `browse::resolve_note_images` 把链接替换为 `data:<mime>;base64` 内联（TextView 内建 data URL 解码缓存；未命中保留原链接渲染 alt）+ 同步拉取侧 `pull_missing_images`（full_sync/incr_sync 尾部，store 缺失字节的 id → `Api::get_image` 落库）+ 推送侧 `add_note`/`update_note` multipart（Files JSON + 每图 part），服务器 `Files` 回写 ServerId + fake-server 手写 multipart 解析与 `file/getImage` 端点。URL 中保持 local id 不改写 content（同 JS）。剪贴板粘贴不做（后续加）。理由：glossary 规定不得静默破坏同步语义——只做本地会让其他设备看不到图片，比不做更糟。
2. **专注模式/全窗口预览拆分（Q10）**：⇧⌘F 更名"专注模式"（行为不变）；⌘T 新增"全窗口预览"（渲染预览铺满整窗、右上角关闭、ESC/⌘T 退出、编辑/阅读态均可用）；删 OS fullscreen 按钮。两模式正交，可叠加。
3. **tree-sitter 全量 feature（Q7）**：启用 kit 全部 ~35 语言。按需解析已由 kit 保证，成本只是二进制体积/编译时间。
4. **markdown 自动延续/补全（Q4/Q5）**：回车延续列表（有序/无序/任务/引用，空项取消）；成对符号闭合、围栏补全、Tab/Shift-Tab 列表缩进。vim 激活时禁用闭合与围栏（vim 用户不期望插入式补全），缩进保留。`$`/`$$` 不做（随 LaTeX 推迟，见 issue 22）。
5. **mermaid/LaTeX 推迟（Q8=A）**：无原生 GPUI 渲染器；deno_core+JS→SVG 管道是重量级独立决策，拆 issue 22。

## 后果

- images 表是 leanote-core 首个"服务器对象本地缓存且带脏推送"的文件表；模式与 notes 一致（ServerId 映射），无新抽象。字节存 BLOB 而非文件目录：leanote 服务器语义就是 fileId→bytes，本地目录只会多一层无意义映射。
- 成对闭合在 vim 下禁用意味着两套输入行为，由 `vim_enabled` 分支保证，不引入配置项。
- 全量语言 feature 增大二进制（估 10–20MB）；接受。
