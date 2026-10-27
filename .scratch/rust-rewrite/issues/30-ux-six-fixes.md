# 30: 六项 UX 修复 — 快捷键 / sync 四态 / logout 关弹窗 / 居中 / 登录预填 / ⌘K 白字

**What to build:** 六项互相独立的小需求：① ⌘S 立即同步、⌘N 新建笔记（应用范围内生效）；② 底部 sync 按钮四态 icon（新增 Pending=同步 icon）；③ logout 后 settings dialog 未自动关闭；④ Syncing/SyncDone 页面内容垂直居中；⑤ 登出后再启动登录页预填 host/email 并聚焦密码框；⑥ ⌘K 搜索结果选中行文字在 accent 背景上变白。

**Root cause / design:**

1. **③ logout 弹窗**：settings 与 logout-confirm 是 kit `active_dialogs` 栈的两层；alert 的 `on_ok` 返回 true 只由 kit 关一层，settings 留在底下。`logout()` 拿不到 window 所以无法自己关。用 `close_all_dialogs`（一次清栈，避免与 kit 自关层的顺序耦合）。
2. **② pending 态**：本地有 IsDirty=1 行（notes/notebooks/tags）即"待同步"。状态来源单一化：`invalidate_lists()` 末尾重查（它本来就是所有变更后的统一失效点），`sync_now` 完成回调已走它，无需第二处挂点。优先级 **Err > Pending > Ok > None**。
3. **⑥ 白字**：kit `item_row` 选中时设 `bg(accent).text_color(accent_foreground)`，但 `Label` 自带 `.text_color(theme.foreground)` 覆盖继承色，且 child 闭包（`Fn(&mut Window, &mut App)`）拿不到选中态——唯一可行方案是去掉显式 text_color 让行级色继承，层级感全靠字号（14/12/11px）。
4. **①⑤**：详见 Changes。

**Changes:**

1. `leanote-core/src/sync.rs`：新增 `last_account()`（`ORDER BY rowid DESC LIMIT 1`，不看 IsActive，返回 `Option<(host, email)>`）与 `has_dirty()`（三表 COUNT，短路返回）。
2. `app_shell.rs`：
   - `logout(&mut self, window, cx)` 签名加 window，尾部 `window.close_all_dialogs(cx)`；调用点（logout-confirm `on_ok`）传 window。
   - render_syncing / render_sync_done 两处 `.items_start()` → `.items_center()`。
   - `AppShell::new` 加参 `remembered: Option<(String, String)>`；fake 预填优先，否则预填 host/email；password 删 `placeholder("••••••••")`（保留 masked）；Login 相且预填非空 → `password.update(… s.focus(window, cx))`。
   - 字段 `pending_sync: bool`；`invalidate_lists()` 末尾 `store().has_dirty()` 重查；`sync_status_label()` 优先级改 Err > Pending > Ok > None；footer icon 四分支 match（Err→TriangleAlert、Pending→RefreshCw、Ok→CircleCheck、None→Cloud）。
   - 编辑器 capture_key_down 的 ⌘S 分支：`save_editor` 后追加 `sync_now`（保存置脏，立即推）。
   - **关键修复**：`new` 尾部 `window.focus(&shell.root_focus, cx)`——`root_focus` 此前只 track 从未 focus，空焦点时（启动即按键）dispatch path 不含 root，⌘K/⌘,/⌘S/⌘N 全部失灵。这使"应用范围内生效"在无焦点状态也成立。
   - `main.rs` bind_keys 增 `cmd-s/ctrl-s → QuickSyncNow`、`cmd-n/ctrl-n → QuickNewNote`（action 与 root on_action 已存在）；未登录时查 `last_account()` 传入 `AppShell::new`。
3. `search_overlay.rs` 搜索结果行三个 Label 删 `.text_color(muted)`，继承行级色（选中=白）。默认行（quick-actions/recents）不动，保留 kit 渲染的 kbd 提示。
4. `i18n.rs` 增 `PendingSync => ("有更新待同步", "Changes waiting to sync")`。

**Verification:** `cargo test -p leanote-app` 66 全绿（新增 `pending_sync_tracks_dirty_rows`：IsDirty 行置 pending + label 文案；`cmd_n_creates_note`：绑定同 main.rs，dispatch ⌘N 断言笔记 +1——该测试暴露并验证了 root_focus 修复）。冒烟：`LEANOTE_FAKE_SERVER=1 LEANOTE_FAKE_AUTOSUBMIT=1` 启动 15s，accounts 落库 IsActive=1、notes IsDirty=0（全推干净）、无 panic。GUI 目测项（居中/白字/icon 四态）待用户确认。

**Blocked by:** （无）

**Status:** resolved (2026-10-27)

## Notes (2026-10-27)

- dispatch 链：`simulate_keystrokes` → `dispatch_key_event` 取 **focus 节点的 dispatch path** → keymap 匹配 → action 沿 path 冒泡。root 的 `on_action` 挂在 `track_focus(&root_focus)` 的 v_flex 上；focus 不在树内时 action 静默丢弃。排查时在 handler 里 `#[cfg(test)] eprintln!` 是最快定位法（binding 没匹配 vs path 断了）。
- ⌘S 在编辑器内走 capture_key_down（save+sync），不落到全局 action；编辑器外走 QuickSyncNow。两条路径语义一致（保存置脏→推）。
- logout-confirm 的 `on_ok` 先 `logout()`（内部 close_all_dialogs 清栈）再返回 true：kit 随后 `defer_close_dialog` 在空栈上 pop 是 no-op，无顺序问题。
- `close_dialog`（弹一层）在 alert 层在栈顶时会关错层；`close_all_dialogs` 是唯一稳的选择。
