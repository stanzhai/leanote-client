# 29: getNoteContent 报错带笔记定位 — noteId + 标题

**What to build:** 同步失败报错在 `api/note/getNoteContent`，但错误信息里没有请求参数，用户不知道是哪个笔记出错。要求该接口报错带上 noteId，最好直接定位到笔记（标题）。

**Root cause:** 错误链有两条，issue 28 只修了第一条：

1. 非 2xx → `check_status` → 已带 endpoint+params+body（issue 28）。
2. **200 + `{Ok:false, Msg:…}`（如 notExists/NOTLOGIN）→ `ok_or_api` → `SyncError::Api(Msg 裸串)`，完全没有 endpoint/params 上下文**——最可能的缺参数路径。其次，内容回填循环（full_sync/incr_sync）上抛错误时不带笔记标识。

**Changes:**

1. `ok_or_api` 加上下文：签名改 `ok_or_api(v, method, path, params)`，200+Ok:false 时返回 `{METHOD} api/{path}; params: {k=v&…}: {Msg}`。4 个调用点迁移（get_sync_state、sync_chunk、get_note_content、get_attach 内部）。抽 `kv()` helper 与 `check_status` 共用，避免两份拼接实现。
2. 笔记定位：`note_server_ids_needing_content()` 改名 `notes_needing_content()`，SQL `SELECT ServerNoteId, Title` 返回 `Vec<(String, String)>`；full_sync/incr_sync 两处内容回填循环的 `?` 改 `map_err`，Api 错误追加 `; note 「{title}」`。仅 Err 分支分配，成功路径零开销。
3. `:1192`/`:1282` 冲突路径的 `Err(_)` 分支不动：语义是"取不到内容当冲突处理"，上抛会改变同步行为。

**Verification:** 新增 2 测试：`content_fetch_error_names_note`（fake `contents()` 移除 note0100 构造 200+notExists + 本地 Content 置空 + set_sync_usn(1002) 触发 pull；断言错误串含 `api/note/getNoteContent`、`noteId=note0100`、`notExists`、标题 `Note 100`，token 不泄露）、`content_fetch_500_names_note`（inject_status 500 同断言，覆盖两条错误路径）。`cargo test -p leanote-core -p leanote-cli -p leanote-app` 全绿（sync 29、app 64 等 10 套）。冒烟输出：`GET api/note/getNoteContent; params: noteId=note0001: notExists; note 「Note 1」`。真实 leanote.com 复现待用户验证。

**Blocked by:** （无）

**Status:** resolved (2026-10-27)

## Notes (2026-10-27)

- 坑：fake `dataset()` 返回的 contents 与 srv 内部是两份，直接 `set_contents(dataset().2)` 会把登录后 push 产生的状态重置——测试里用 `srv.contents()` 取当前快照再改。另 incr_sync 的内容回填在 pull 分支（`server_usn > last`）内，测试须 `set_sync_usn` 抬高服务端 usn 才会走该分支。
- login 的 `Ok:false` 不走 `ok_or_api`（login 自己判 Ok），`WrongPassword` 裸串保持不变，`login_persists_token_and_wrong_password_errors` 断言不破。
- 标题用 `「」` 包裹，与 params 的 `k=v` 视觉分隔；中文全角括号在状态栏等宽渲染无问题。
