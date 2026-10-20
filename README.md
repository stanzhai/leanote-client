# Leanote Desktop App

[![Gitter](https://badges.gitter.im/Join%20Chat.svg)](https://gitter.im/leanote/desktop-app?utm_source=badge&utm_medium=badge&utm_campaign=pr-badge)

Use Electron(atom-shell) to create leanote desktop app.

![preview.png](preview.png "")

## Download
Please see http://app.leanote.com

## How to develop it

Download this project, and run

```shell
nvm use v18

# 1. install dependencies
$> cd PATH-TO-LEANOTE-DESKTOP-APP
$> ELECTRON_MIRROR=https://npmmirror.com/mirrors/electron/ npm install

# 2. build Less styles (gulp 已弃用，使用新的 npm 脚本)
# Option A — 一次性构建
$> npm install
$> npm run less:build

# Option B — 开发中监听并自动构建
$> npm install
$> npm run less:watch

# 3. run with electron
$> npm run dev
```

## Rust 重写版（GPUI）

本仓库含一套完整的 Rust 重写（`rust/`）：SQLite 存储、leanote 同步引擎、本地 HTTP/MCP（127.0.0.1:8912）、三栏 GPUI 桌面端与命令行工具，与 Electron 版线协议、CLI 字节输出、MCP 行为对拍一致。详见 `rust/README.md`。

### 开发与运行

```sh
cd rust
cargo build --release            # 产出 target/release/leanote-app（GUI）与 leanote（CLI）
cargo test                       # 全工作区测试（145 个）
./target/release/leanote-app     # 直接运行桌面端

# 冒烟（假服务器 + 自动登录，无需真实账号）：
LEANOTE_RUST_DATA=/tmp/smoke LEANOTE_FAKE_SERVER=1 LEANOTE_FAKE_AUTOSUBMIT=1 ./target/release/leanote-app
```

数据目录：`~/Library/Application Support/leanote-rust/`（可用 `LEANOTE_RUST_DATA` 覆盖），与 Electron 版的 NeDB 数据完全隔离。日志：`<数据目录>/logs/app.log`；`LEANOTE_MCP_DEBUG=1` 时追加 `~/leanote_mcp.log`（与 JS 版一致）。

### 打包（macOS）

```sh
sh rust/scripts/package-macos.sh        # 默认输出 ../dist（相对 rust/）
```

产出 `dist/Leanote.app`（GUI + CLI 双二进制、`icon.icns`、ad-hoc 签名）、`dist/Leanote.dmg`（hdiutil），并把 `~/bin/leanote` 指向包内 CLI。Linux/Windows 暂无打包脚本，见 `rust/README.md` 平台说明。

验收脚本：`sh rust/scripts/acceptance-macos.sh`（CLI/HTTP/MCP/日志/打包产物，15 项）。

### CLI 与 MCP 对拍

```sh
LEANOTE_PORT=8913 bash rust/crates/leanote-cli/compare-with-js.sh . rust/target/debug/leanote   # JS/Rust CLI 逐字节对比（56/56）
LEANOTE_RUST_MCP=1 node tests/test_mcp_server.js                    # 未修改的 JS 测试跑 Rust MCP
LEANOTE_RUST_MCP=1 node tests/test_mcp_sse.js
LEANOTE_RUST_MCP=1 node tests/test_mcp_stateless.js
```

## Package

```shell
nvm use v18
npm run build
```

## Command Line Interface

Leanote 桌面客户端启动后会自动安装 `leanote` 命令行工具，无需系统安装 Node.js。

```bash
# 添加笔记
leanote -t "标题" -c "# Markdown 内容" -g "标签1,标签2"
leanote add -t "标题" -c "内容" -n "AI札" -g "test"

# 通过管道传入内容
echo "# Hello" | leanote -t "管道测试"

# 列出最近笔记
leanote list                  # 最近的 20 条
leanote list -n "AI札"        # 按笔记本过滤
leanote list -g "work"        # 按标签过滤
leanote list -v -N 5          # 显示内容预览，限定 5 条

# 查看笔记完整内容（支持 ID 前缀匹配）
leanote show a1b2c3d4

# 搜索笔记（匹配标题、内容、标签，按相关度排序）
leanote search "关键字"
leanote search "meeting" -N 10

# 编辑笔记
leanote edit a1b2c3d4 -t "新标题"
leanote edit a1b2c3d4 -g "urgent,important"   # 替换标签
leanote edit a1b2c3d4 -n "其他笔记本"          # 移动笔记
echo "# 新内容" | leanote edit a1b2c3d4 -c -  # 管道更新内容
```

## MCP Server

Leanote 桌面客户端启动后会在本地开启 **MCP Server**（Model Context Protocol，Streamable HTTP），让 AI 智能体（Claude Desktop、Cursor 等）通过标准协议管理笔记。

- 端点：`http://127.0.0.1:8912/mcp`（仅绑定本机回环 127.0.0.1，需 app 在运行）
- 与 CLI 文件 I/O 模式一致：直接读写本地 NeDB 文件，写入的笔记带同步标记（`IsDirty`），app 下次加载/同步时生效

### 工具

| 工具 | 说明 |
|---|---|
| `list_notebooks` | 列出所有笔记本及笔记数量 |
| `list_tags` | 列出所有标签 |
| `add_note` | 新建 Markdown 笔记（`title` / `content` / `tags` / `notebook`，默认笔记本 "AI札"，不存在则自动创建） |
| `list_notes` | 列出最近笔记，可按 `notebook` / `tag` 过滤 |
| `get_note` | 按 ID（或前缀）查看笔记完整内容 |
| `search_notes` | 按关键字搜索（标题 / 正文 / 标签），按相关度排序 |
| `update_note` | 更新笔记（`title` / `content` / `tags` / `notebook`） |
| `delete_note` | 移到回收站 |

### 智能体配置

在 Claude Desktop / Cursor 等的 MCP 配置中加入（Streamable HTTP）：

```json
{
  "mcpServers": {
    "leanote": { "url": "http://127.0.0.1:8912/mcp" }
  }
}
```

### 调试

```bash
npx @modelcontextprotocol/inspector
# Transport 选 Streamable HTTP，URL 填 http://127.0.0.1:8912/mcp
```

仓库内附冒烟测试：`node tests/test_mcp_server.js`（需本机已有 Leanote 数据）。

## Docs

Please see https://github.com/leanote/desktop-app/wiki

## LICENSE

[LICENSE](https://github.com/leanote/desktop-app/blob/master/LICENSE)

```
LEANOTE - NOT JUST A NOTEPAD!

Copyright by the contributors.

This program is free software; you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation; either version 2 of the License, or
(at your option) any later version.

This program is distributed in the hope that it will be useful,
but WITHOUT ANY WARRANTY; without even the implied warranty of
MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
GNU General Public License for more details.

Leanote destop app is licensed under the GPL v2.
```
