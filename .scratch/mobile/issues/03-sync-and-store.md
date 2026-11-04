# Issue 03: 同步接入——设备端 SQLite + 全量同步进度

状态:已完成(2026-11-04)

## 需求

- 复用 `leanote-core`：设备数据目录（iOS app container）、全量同步 + 进度 UI、`pull_missing_images`。
- 只拉不推：push 路径不接（无编辑面即无脏数据）。

## 实现

- `MobileApp` 增加 `SyncPhase`（Progress/Done）与 `store: Option<Arc<Mutex<Store>>>`（后台同步线程与 UI 共享；rusqlite Connection 是 Send）。
- `start_full_sync`：登录成功 → 后台 `full_sync`（桌面 `run_login_sync` 同款 ChannelProgress 模式，channel 关闭即完成）；进度屏显示 note 行 + `笔记内容 d/t`；完成屏 = 摘要行（X 笔记本 · Y 篇笔记（内容 Z），对齐桌面同步摘要卡的最小版）+「开始使用」按钮进主壳。
- 已登录冷启动：`active_account` 直进主壳（离线可读），启动增量刷新在 issue 08。
- `pull_missing_images`：随 `full_sync` 内置复用，无单独代码（语料 0 图片）。
- `TabShell.note_count`：笔记 Tab 占位显示库内笔记数——issue 03 验收的可见证据，issue 04 换真列表后删。
- fake seed 补 `set_contents`（n1/n4 两条），覆盖内容拉取阶段。

## 验证

- 模拟器（`LEANOTE_FAKE_SERVER=1`）：登录 → 进度 → 完成屏截图确认「同步完成：3 笔记本 · 4 篇笔记（内容 2）」（与 seed 一致）。
- 断网冷启动（无 env、无 server）：直进主壳，笔记 Tab 显示「4 篇笔记」（本地 SQLite 读出）+ 选中 Tab 蓝。正文（内容 2）随 full_sync 落库 `Content` 字段。
- 三平台 check 绿；桌面 `cargo test -p leanote-app -p leanote-core` 6 套件 ok。

## Comments

- 「开始使用」按钮的点击无法自动化（simctl 无 tap），进入主壳的路径由断网冷启动（同一条 `sync=None → shell` 渲染分支）验证。
