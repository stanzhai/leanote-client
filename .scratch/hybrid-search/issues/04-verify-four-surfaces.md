# 04 四端验证

cargo test leanote-core 全绿；workspace build（cli/http/mobile/app）；确认移动端退化为纯 FTS 且返回结构一致；MCP `search_notes` 旧字段字节兼容。

Status: resolved
Blocked by: 01, 02, 03

## Comments


## Answer

core 74 / CLI 55（含 JS 输出 parity）/ app 102 / http+mobile 全绿；workspace build 0 error。移动端 `search_hybrid(None)` = 纯 FTS，返回结构与桌面一致；MCP `search_notes` 旧字段未动（新增 `matched_via`）。