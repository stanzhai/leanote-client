# 移动端形态（iOS 先行）spec

来源：2026-11-03 grill 会话，决策记录见 ADR-0013。GLOSSARY「移动端」「移动端导航」。

## 范围

- 栈：gpui-mobile（`gpui-pre-mobile`），新 crate `rust/crates/leanote-mobile` + `rust/mobile/{ios,android}/` 宿主工程。复用 `leanote-core` / `theme.rs` / 渲染管线；不复用 `app_shell.rs`。
- v1 纯阅读：只拉不推，无冲突面（桌面唯一写端）。
- 平台：iOS 先行；Android 跟进（issue 10）。
- 发布：TestFlight + 直装 APK 自用。

## 裁剪清单（v1 不做）

编辑（标题/正文）、Vim、斜杠面板、⌘K 面板（搜索 Tab 取代）、多选、收藏/Star（本地字段跨端不通，显式缺口）、发布博客、回收站、历史版本、导出/PDF、专注模式、全窗口预览、本地 HTTP 服务/CLI/MCP、tray、多账户切换、后台刷新。

## 验收（iOS v1）

登录（含自托管地址）→ 全量同步进度 → 最近列表（RECENT_LIMIT=20 复用）→ 笔记本/标签浏览 → 搜索（复用统一搜索核心）→ 阅读 markdown（高亮/Mermaid/图片）与 HTML 富文本只读 → 明暗/主题族/主题色/语言/缩放 → 下拉刷新 + 启动自动增量同步 → 登出。CI：`cargo check --target aarch64-apple-ios`。

## 关联决策

- 版本对齐：vendor gpui-mobile patch 到 0.3.9；fallback 降级 vendored gpui-pre 至 0.3.4 重放 IME 补丁（ADR-0005）。
- 凭据：v1 明文 SQLite（app 沙箱），上架前升 Keychain/Keystore。
- 同步：纯前台（启动 + 下拉 + 手动按钮）。
- 导航：底部 Tab（笔记/搜索/设置）+ 推入栈；「笔记」Tab 落地最近列表，笔记本/标签为其内浏览入口。
