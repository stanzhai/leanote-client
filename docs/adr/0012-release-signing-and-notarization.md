# ADR-0012: 发布产物签名与公证（CI 密钥门控）

- 状态：Accepted（2026-10-30）
- 关联：`.github/workflows/release.yml`；`rust/scripts/package-macos.sh`；`rust/scripts/package-windows.ps1`；ADR-0008 跨平台发布

## 背景

GitHub Releases 上的产物未做分发签名：

- macOS `.app` 只有 ad-hoc 签名（`codesign --sign -`）。经浏览器下载后带 `com.apple.quarantine`，Gatekeeper 报「"Leanote"已损坏，无法打开。您应该将它移到废纸篓」——macOS 15 Sequoia 起「右键打开」绕过也被移除，普通用户无法安装。
- Windows 安装器/便携 exe 完全未签名 → SmartScreen「Windows 已保护你的电脑」。

「已从互联网下载，确定打开吗」这个可接受的首次运行提示，只有 Developer ID 签名 + 公证（notarization）+ staple 的应用才会出现。两个平台的提示都无法靠打包参数消除——签名是唯一路径，且证书必须购买。

## 决策

在两个打包脚本中加入签名，**以 CI secrets 是否存在为门控**；本地无密钥构建行为与从前完全一致（mac ad-hoc / win 未签名）：

1. **macOS**（`package-macos.sh`）：`MACOS_CERT_P12_BASE64` + `MACOS_CERT_PASSWORD` + `APPLE_ID` + `APPLE_APP_SPECIFIC_PASSWORD` + `APPLE_TEAM_ID` 齐备时——
   - Developer ID Application 证书导入临时 keychain；
   - `Contents/MacOS` 下逐个二进制 + 整个 `.app` 以 hardened runtime（`--options runtime`）+ 时间戳签名；纯 Rust gpui 应用无 JIT，不需要 entitlements；
   - `notarytool store-credentials` → 提交 zip 公证 → `stapler staple` `.app`；
   - DMG 生成后单独签名 → 公证 → staple（DMG 需要自己的 ticket），完成后删除临时 keychain。
   - 公证要求 `CFBundleVersion`/`CFBundleShortVersionString` 为纯数字，tag 前缀 `v` 剥离为 `PLIST_VERSION` 后写入 plist（文件名保留 `v`）。
   - 证书已配但 Apple 凭据缺失 → 直接失败，不静默回退未签名发版。
2. **Windows**（`package-windows.ps1`）：`WINDOWS_CERT_PFX_BASE64` + `WINDOWS_CERT_PASSWORD` 存在时——SDK 自带 signtool 对两个 exe（打 zip 前签名，便携包内也是签名件）与 NSIS setup 做 SHA-256 + RFC3161 时间戳（DigiCert）签名。
3. `release.yml` 把上述 secrets 透传给构建 job；secret 未配置时为空串，脚本走无签名分支。

## 后果

- 消除提示需要一次性采购：Apple Developer Program（$99/年，mac 必需）；Windows 代码签名证书（OV/EV，或 Azure Trusted Signing $9.99/月）。SmartScreen 信誉即使有证书也随下载量逐步积累（EV 即时）。
- Azure Trusted Signing 走自家客户端而非 signtool PFX 路径，若选它需替换 `package-windows.ps1` 中约 10 行签名调用。
- 未签名构建的用户侧绕过（`xattr -dr com.apple.quarantine` / 「更多信息 → 仍要运行」）保持为本地/应急路径，不在发布产物上承诺。
