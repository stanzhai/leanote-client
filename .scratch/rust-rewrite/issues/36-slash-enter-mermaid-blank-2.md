# 36. 斜杠面板回车执行 + Mermaid 二次空白

- 状态：done
- 标签：bug, editor, mermaid
- 报告：2026-10-29 用户实测两缺陷

## 现象

1. 斜杠命令面板：方向键切焦点已生效，但回车不执行选中命令；面板仍偏松。
2. Mermaid 图渲染后图下方仍有大面积空白（上一轮 ink 裁剪已合入）。

## 根因

### 1. 回车/Tab 是动作，不走按键事件层

kit 输入组件把 `enter`/`tab` 绑定为动作（`Enter`/`IndentInline`，state.rs:156-187），
经 `on_action` 派发（state.rs:4654）。面板的执行逻辑写在 `capture_key_down` 里 ——
对 enter/tab 永不执行（死代码）。方向键上一轮同类问题已用 `capture_action::<Move*>` 修复，
Enter/Tab 是遗漏。

### 2. 裁剪光栅化用空字体库，bbox 漏掉全部文字

`ink_viewbox` 用 `usvg::Options::default()` —— 其 fontdb 为空
（options.rs:112 `fontdb: Arc::new(Database::new())`），所有 `<text>` 被丢弃，
ink 包围盒不含文字。gpui 展示侧光栅化 lazy 加载系统字体 → 两侧不一致：

- 文字主导的带（gantt 底部日期轴，纯文字）被裁掉；
- 形状主导的图族 bbox 偏小 → 裁剪不足、空白残留（journey/timeline/quadrant 实测差 47-65px）。

probe 实证（resvg 0.46，同 merman 0.9.2）：empty-db vs sys-db ink px 比 gantt 38908/36774……
sys-db 裁剪后 gantt/journey/timeline/quadrant/sankey 空白带从 37/90/48/6/123px 全部归到 2px margin。

## 修复

- `app_shell.rs`：`capture_action::<Enter>` / `<IndentInline>` 顶部加面板分支（面板开→执行选中命令+关闭+stop_propagation），与 Move* 同模式；删除 `capture_key_down` 里永不触发的 is_exec 分支。
- `mermaid.rs` `ink_viewbox`：构建 `usvg::Options` + `fontdb::Database::new()` + `load_system_fonts()`，与 gpui 展示侧一致。
- `slash_panel.rs`：CELL 36→32、GAP 4→2、PAD 6→4（prepaint 公式引用常量自动跟随）。

## 验证

- `cargo test -p leanote-app mermaid` 12 通过；新增 `ink_viewbox_counts_text_ink`（文字必须进 bbox）与 `render_svg_crop_keeps_gantt_axis_labels`（裁剪后 viewBox 底边须覆盖日期文字行）。
- `cargo test --workspace` 212 通过 0 失败；`cargo build --release` 成功。
- 交互面（合成输入不可达，/Applications 旧副本顶替进程）需手动验证：`/` 呼出 32px 紧凑格、方向键+回车执行、gantt 图下方无空带且日期文字完整。

## 缓存

`render_svg` 结果是进程内 static HashMap（键 (fxhash(src), dark)）——升级后新进程自然重算，无需清理。
