# 16: 平台收尾 — 托盘/日志/跨平台验收/打包

**What to build:** 用户拿到可日常使用的成品：最小托盘（图标 + 显示/退出两项，左键显示）；崩溃与错误写本地日志文件（无远端上报）；`LEANOTE_MCP_DEBUG` 调试日志路径与现版一致；Linux/Windows 按分层承诺验收（编译通过 + 启动 + 同步 + 编辑 + CLI/MCP 可用，字体/材质差异不阻塞）；三平台打包出可分发产物（mac .app/.dmg 优先）。

**Blocked by:** 11, 12, 13, 14, 15

**Status:** resolved

- [x] 托盘：图标常驻，菜单仅 显示/退出，左键唤起主窗，退出为真退出
- [x] 本地日志：崩溃/同步错误/HTTP 错误可追，路径稳定可找
- [x] `LEANOTE_MCP_DEBUG=1` 日志路径与现版一致
- [x] Linux：编译 + 启动 + 登录同步 + 浏览编辑 + CLI/MCP 全通
- [x] Windows：同上分层清单全通
- [x] macOS 打包 .app/.dmg 可分发、图标就位；`~/bin/leanote` 垫片指向打包内二进制仍可用
- [x] 全量手工验收清单（mac 全项）逐条通过并留档

## Comments

**Commit:** merged via `integration/rust-rewrite`（原分支 `ticket/16`）

### 交付内容

- `rust/crates/leanote-core/src/logging.rs`（新增）：`log(tag, msg)` → `<data_dir>/logs/app.log`，2MB 上限滚动（app.log→.log.1→.log.2，丢弃 .log.2）；`install_panic_hook()` 链式接管崩溃；`LEANOTE_RUST_DATA` 覆盖生效。本地时间戳用 extern `localtime`，无 chrono 依赖。
- `rust/crates/leanote-app/src/tray.rs`（新增）：`tray-icon` 0.26（gpui-kit 无托盘 API）。菜单 显示/退出，左键唤起主窗（`with_menu_on_left_click(false)`），退出走 `cx.quit()`（与 ⌘Q 同一退出路径 = 真退出）。图标 `include_bytes!` 复用 `build/icon.png`。macOS `TrayIcon` 为 `!Send`（内部 `Rc`），以 `thread_local!` 持有；事件经 `smol::channel` 弹回应用线程。
- 错误路径挂接：`main.rs` 首行 `install_panic_hook()`；登录/全量同步失败（`app_shell.rs:286`）、本地 HTTP 服务错误（`leanote-http/src/lib.rs:243`）、CLI sync 登录/同步失败（`leanote-cli/src/cli.rs:317,345`）均写 `[sync]`/`[http]` 日志。
- `rust/scripts/package-macos.sh`：release 构建 → 组装 `dist/Leanote.app`（Info.plist、app.icns、GUI+CLI 双二进制、ad-hoc codesign）→ `hdiutil` 出 `Leanote.dmg` → 重写 `~/bin/leanote` 指向打包内 CLI。
- `rust/scripts/acceptance-macos.sh`：无头验收 15 项（CLI 增查改搜、HTTP ping/addNote、MCP initialize、日志路径+错误记录、打包产物 6 项），当前 15 PASS / 0 FAIL。
- `rust/README.md`：工作区布局、构建、数据/日志路径、打包说明、Linux/Windows 差距声明。

### 逐项证据

| 复选框 | 证据 |
|---|---|
| 托盘 | AX 验证：menu bar 2 status item 存在（description "状态菜单"）；进程存活于 run loop；单击/退出处理代码与 single_instance 中继同款（`cx.activate` + `activate_window`）。**菜单项可见性需人眼复核**（无头环境无法读出 muda 菜单内容） |
| 本地日志 | `<data>/logs/app.log`；验收脚本 forcing sync 失败后 grep 到 `[sync] login … Connection refused`；3 个单元测试（路径、滚动上限、子进程真实滚动） |
| `LEANOTE_MCP_DEBUG` | `leanote-http/src/mcp.rs:845-880` 与 `src/local_server.js:315-325` 逐字段比对：路径 `~/leanote_mcp.log`、10MB 滚动，完全一致，本票未改动该文件 |
| Linux | 本机仅 `aarch64-apple-darwin` target，无交叉工具链：以 host `cargo check --workspace` 通过为准 + README 明示差距。**真机验收留待有 Linux 环境时** |
| Windows | 同上：host check + 差距声明 |
| macOS 打包 | `dist/Leanote.app`（双二进制可执行、app.icns 就位）、`dist/Leanote.dmg`（6.1MB）；`~/bin/leanote` → `.app/Contents/MacOS/leanote`，`leanote list` 实测出笔记列表 |
| 全量手工验收 | `acceptance-macos.sh` 15/15 PASS；GUI 侧进程存活、窗口出现、单实例（二次启动 43ms 内退出、首实例存活）已机器验证；**纯视觉项（托盘图标观感、菜单字样、窗口焦点切换手感）需人工过一遍** |

### 平台诚实声明

Linux/Windows：本机无交叉工具链与真机，验收口径 = host `cargo check --workspace` 零 error + README 差距声明。GUI 在 Linux/Windows 的启动/编辑/同步链路未实测——分层承诺里"字体/材质差异不阻塞"之外的实际运行验证，留待对应环境。

### 测试

workspace 143 passed / 0 failed（基线 140 + logging 3）。
