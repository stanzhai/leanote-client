# 25: Markdown 预览不显示远程图片（私有图片匿名 500）

**What to build:** 用户报告 markdown 预览无法正常显示图片。实测（真实库 `leanote.db`：114 条含图笔记，全部是 `https://stanzhai.site/api/file/getImage?fileId=<24hex>` 远程链接，`leanote://` 链接 0 条，`images` 表 0 行）：

- 114 张图中 41 张匿名 GET 返回 500（其余 73 张公开图匿名 200）。
- 同一 URL 加上账号 token（`...?token=<Token>&fileId=...`）后 114/114 全部 200 image/png。
- 预览（阅读态/编辑预览/⌘T 全窗口）走 GPUI HTTP client 直接 GET 图片 URL，无登录会话/token → 私有图 500，img 元素错误分支静默空白（无 fallback 时）。

**Fix:** `crates/leanote-app/src/browse.rs` `resolve_note_images`（三处预览唯一入口）追加一步：读 `store.active_account()`，把笔记里 `<host>/api/file/getImage?fileId=` 前缀（host 来自账号，含尾斜杠归一）重写为 `<host>/api/file/getImage?token=<Token>&fileId=`。token 为 hex 无需转义；host 不匹配的外站图不动；未登录（无账号或空 token）不动。

**Done 判据:** 含私有图的笔记在三处预览显示图片；公开图不受影响（token 附加后实测仍 200）；`cargo test -p leanote-app --bin Leanote` 全绿（新增回归 `resolve_note_images_appends_token_to_remote_links`）。

**Status:** done
