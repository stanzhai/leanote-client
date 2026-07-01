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
