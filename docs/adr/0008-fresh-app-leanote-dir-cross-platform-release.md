# ADR-0008: 数据目录并入 `leanote`、全新 app 语义、跨平台发布

- 状态：Accepted（2026-10-24，打包与发布轮裁决）
- 关联：ADR-0002（新本地存储不迁移 NeDB）、GLOSSARY「存储层」

## 背景

Rust 版此前以 `leanote-rust` 为名：数据目录、bundle id、GUI 二进制均带 `-rust` 后缀，且数据目录刻意与 Electron 版的 `~/Library/Application Support/leanote/` 隔离（ADR-0002 时代的防御）。Windows 无打包、`app_data_dir()` 依赖 `HOME`（Windows 常未设置，直接 panic）、GUI 无 `windows_subsystem` 属性（Windows 下带控制台窗口）。

发布需求：名称统一为 Leanote、跨平台打包（win/mac/linux × x64/arm64）、tag 推送自动构建并附到 release。

## 决策

1. **全新 app 语义**：Rust 客户端作为 Leanote 的正统继任，产品名/bundle id/数据目录全部去掉 `-rust`。数据目录为 `~/Library/Application Support/leanote`（mac）、`%APPDATA%\leanote`（win）、`~/.config/leanote`（linux，尊重 `XDG_CONFIG_HOME`）。Electron 旧 NeDB（`nedb55/`）在同一 mac 目录共存但永不读写——文件名不相交（`leanote.db`）。**不做** `leanote-rust` → `leanote` 迁移：存储本来就是服务器缓存，重登录全量同步即恢复。
2. **GUI 二进制名 `Leanote`**；CLI 二进制随包但规避大小写不敏感文件系统（APFS/NTFS）冲突：mac `.app` 内 `leanote-cli`、win `<安装目录>\bin\leanote.exe`（NSIS 可选加 PATH）、linux `/usr/bin/leanote`。用户敲的命令名三平台不变。
3. **`LEANOTE_RUST_DATA` → `LEANOTE_DATA`**，无别名，仓库内脚本/测试一次切换。
4. **Windows**：`#![windows_subsystem = "windows"]` 消控制台；`winresource` 在构建期把 `icon.ico` + manifest 嵌入 exe（仅 Windows target，build.rs 判 `CARGO_CFG_TARGET_OS`）。
5. **打包脚本手写**（复用 `rust/scripts/` 现有模式，不引 cargo-packager/tauri-bundler）：`package-macos.sh <arch>`（.app+.dmg）、`package-linux.sh <arch>`（AppImage+deb，`dpkg-deb`/`appimagetool`）、`package-windows.ps1 -Arch`（NSIS+zip）。图标产物（icns/ico/png）由 `gen-icons.sh` 生成后**提交入库**，CI 不装图形工具链。
6. **CI**：`.github/workflows/release.yml`，推 `v*` tag 触发 6 目标矩阵（mac arm64 runner 交叉编 x64；linux/win 双架构原生 runner），10 个产物直接 publish 到 GitHub Release。版本取 tag，不回写 Cargo.toml。仅 linux-x64 格跑 `cargo test --release` 作冒烟。无签名（mac ad-hoc、win 未签名）。

## 后果

- 老的 `leanote-rust/` 数据目录成为孤儿（用户需重登录全量同步）——接受，单人使用。
- Electron 版若与新客户端同时安装，同目录下各自文件（`leanote.db` vs `nedb55/`）互不干扰，但删除"Electron 数据"时需注意别整目录删。
- Linux arm64 / Windows arm64 的实际安装运行只能由 CI 绿 + 真机验收兜底（本机无对应硬件）。
