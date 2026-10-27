# 27: UI 第十轮 — 设置面板重构 + 字体/缩放/tooltip 等 13 项

**What to build:** 用户 2026-10-26 清单 13 项（裁决完毕）：

1. **设置面板两栏三分区**：左栏 110px 分区导航（外观/编辑器/数据）+ 右栏设置项，宽 640px。编辑器配色与主题色归外观分区。
2. **语言切换即时生效**：dialog builder 每帧重跑（kit root.rs:204），build 闭包内 `shell.read(cx).lang` 动态读，无需重开弹窗。
3. **界面字体**：系统默认 / 霞鹜文楷（LXGW WenKai）。首启后台线程下载 v1.530 Regular ttf（ureq 3，直链 25.9MB，重试 1 次，magic 校验）装入 `~/Library/Fonts`，重启生效（CoreText 启动时扫描）。prefs.font 字段；theme.rs `WENKAI` + `sans_for(font_pref)`。
4. **界面缩放三档**：100/110/120%（rem 级）。旧 prefs `zoom > 1.21` clamp 到 1.2。
5. **⌘K 快捷面板删 2 项**：导出备份、Vim 切换（设置面板保留开关）。actions! 仅剩 QuickNewNote/QuickSyncNow。
6. **markdown 编辑器字号跟缩放**：容器 `text_size(rems(1.0))`。
7. **标题行缩放**：`text_size(rems(1.25)).h(px(32.))`（原 20px/36px 固定）。
8. **4 个 tooltip**：header-search / nav-settings / new-note / empty-trash（此前裸 icon 无提示）。
9. **空列表态垂直居中**：`v_flex().flex_1().items_center().justify_center()`。
10. **状态栏去下边框**：视觉不再框住编辑区。
11. **Tab 修复**：预览态不从标题抢焦点（`!ed.preview` 条件）。
12. **打包**：gen-icons.sh SRC → 根目录 logo.png 重生成全套；bundle id `site.stanzhai.leanote.client`（macOS Info.plist + Linux appdata）。
13. **登出确认框语言**：on_click 时捕获 `shell.read(cx).lang`（曾硬编码 Zh）。

**Verification:** `cargo build -p leanote-app` 零错误零死代码告警；`cargo test -p leanote-app` 63 通过；release 构建 + LEANOTE_FAKE_AUTOSUBMIT=1 冒烟（窗口起、⌘K 无焦点可呼出、设置弹窗开、AddTagPlaceholder 恢复、字体 ttf 落盘 25.9MB）。

**Blocked by:** （无）

**Status:** resolved (2026-10-26)

## Notes (2026-10-26)

- prefs::round_trip 的 legacy-json 断言需带 `editor_palette` 与 `recents`（两字段无 `#[serde(default)]`，缺了整个反序列化失败 → 落 Default 掩盖 clamp 行为）。
- 字体 segmented 第三槽位为 dummy（helper 固定 `[SharedString; 3]` 签名，active 只会 0/1）；`FontInstallFailed` key 预留（下载失败无回传通道，失败时静默用系统字体）。
- drag_header 需 lang 参数传入（渲染期 `entity.read(cx)` 触发 "cannot read while being updated" panic，5 个测试曾因此挂）。
