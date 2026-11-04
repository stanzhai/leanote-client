# Issue 09: iOS v1 验收 + TestFlight + CI

状态:已完成(2026-11-04；TestFlight 上传为记录在案的人工步骤)

## 需求

- 按 spec 验收清单全链路真机/模拟器走查。
- TestFlight 打包；CI 加 `cargo check --target aarch64-apple-ios`。

## 实现/验证

### CI

`.github/workflows/mobile-check.yml`：push/PR 触发（rust/ 路径），iOS job（macos runner）+ Android job（ubuntu + NDK 28.2 装配 CC/CXX/AR env）。首次 push 后生效。

### 验收清单走查（各 issue 证据汇总）

| 项 | 证据 |
|---|---|
| 登录（含自托管地址） | issue 02：登录页截图（host/email/密码）；fake server 真实 `Api::login` |
| 全量同步进度 | issue 03：「同步完成：3 笔记本 · 4 篇笔记（内容 2）」截图 |
| 最近列表（RECENT_LIMIT=20） | issue 04：倒序列表截图 |
| 笔记本/标签浏览 | issue 04：嵌套缩进 + 标签计数截图 |
| 搜索（统一核心） | issue 06：「渲染」两命中、权重排序截图 |
| markdown 阅读（高亮/Mermaid/图片） | issue 05：Mermaid 中文节点、data-URL 红点、GFM 表格截图 |
| HTML 富文本只读 | issue 05：n10/n11 截图（ADR-0001 管线） |
| 明暗/主题/主题色/语言/缩放 | issue 07：Gruvbox Dark + English + zoom 端到端截图 |
| 下拉刷新/启动增量 | issue 08：USN 17→20 启动增量拉新笔记截图 |
| 登出 | issue 07 代码路径（deactivate + 回登录页）；点击未自动化 |
| Release 构建 | `-configuration Release` BUILD SUCCEEDED（iphonesimulator） |
| CI check | workflow 就绪 |

### 未自动化项（手动走查清单，真机/模拟器点击）

Tab 切换、行点击推入/返回、触摸滚动手感、IME 拼音组合输入、设置 chips 点击、登出按钮、真下拉手势（现为「最近 ⟳」头行点击）。

### TestFlight（人工步骤，wizard 类）

签名基础已预接线：project.yml `DEVELOPMENT_TEAM: 253N8647LQ`（现有 Developer ID 同一账号），模拟器构建保持免签。需要你：

1. Xcode → Settings → Accounts 登录该 Apple ID；
2. 首次真机构建自动创建 Apple Development 证书 + profile（`xcodebuild -sdk iphoneos -configuration Release archive`）；
3. Xcode Organizer → Distribute App → TestFlight（或 `xcrun altool` 上传）。

bundle id `com.leanote.mobile` 需在开发者后台注册（自动签名会代建）。
