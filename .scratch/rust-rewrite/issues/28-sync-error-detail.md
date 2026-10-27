# 28: 同步报错细化 — 接口 / 参数 / 服务端响应体

**What to build:** 用户登录 leanote.com 后同步遇 `unconnected: http status: 500`，只有状态码无法定位。错误信息必须包含：调用的接口、传的参数、服务端返回的异常体。

**Root cause:** ureq 3.3.0 默认 `http_status_as_error(true)`，4xx/5xx 在 run.rs:125-130 转成 `Error::StatusCode(u16)` 并**丢弃整个 Response（含 body）**——拿 500 详情的唯一路径是关掉该 flag 让非 2xx 走正常响应。

**Changes:**

1. `sync.rs default_agent()` 加 `.http_status_as_error(false)`。
2. 新增 `Api::check_status(resp, method, path, params, form)`：2xx 放行；非 2xx 读 body（trim + 截 500 字符）拼 `{METHOD} api/{path}: HTTP {status}; params: {k=v&…}; form: {k=v&…}; body: {…}` 返回 `SyncError::Api`（Display 无前缀，直接进 UI 状态栏）。`get`/`post_form`/`get_image`/`get_attach`/`post_note_multipart` 五个底层调用全部先过 check_status——12 个上层调用点一次覆盖。
3. 敏感信息：token 只在 URL query（check_status 不打印 URL，params/form 是业务字段白名单）；login 的 pwd 在 form 里但 check_status 只在**非 2xx**（登录失败）时打印——与 Electron 客户端把错误回显给用户同级别，且错误串只进本地日志/状态栏。见 Notes 取舍。
4. fake-server：`Inner.fail_status: Option<(suffix, u16, body)>` + `inject_status(suffix, status, body)`，axum middleware `from_fn_with_state` 匹配 path 后缀命中即短路返回注入状态码，一次性消费。

**Verification:** 新增 2 测试（500 on getSyncState、400 on addNote 带表单字段断言）；`cargo test -p leanote-core -p leanote-cli -p leanote-fakeserver -p leanote-app` 全绿（sync 27、app 64）；CLI 端到端冒烟：`leanote-cli sync --host <fake>` 输出 `Error: GET api/user/getSyncState: HTTP 500; params: macAddr=02:00:00:00:00:00&v=macos_aarch64; body: Internal Server Error: mongo: no reachable servers`，exit 1；token 不出现在错误串（测试断言）。真实 leanote.com 500 复现待用户验证。

**Blocked by:** （无）

**Status:** resolved (2026-10-27)

## Notes (2026-10-27)

- login 失败（如 500 on auth/login）时 form 含明文 pwd 会进错误串。当前接受：错误串只落本地 app.log + 状态栏，Electron 客户端同样把失败回显给用户；若要收紧，在 check_status 里对 key=="pwd" 打码即可（一行）。
- 3xx：ureq 自动跟随重定向，check_status 理论上见不到；若服务器返回不跟随的 3xx 会按非 2xx 报错（status 数字原样）。
- 错误信息里 params 用原始值不二次 urlencode（可读性优先，非 URL 构造）。
- UI 冒烟：图形会话断（screencapture 全黑），状态栏渲染路径 app_shell.rs:535 `format!("{}: {}", SyncFailed, e)` 未动，`e` 即新错误串——CLI 冒烟已覆盖同一 `SyncError::to_string()` 链路。
