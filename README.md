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
