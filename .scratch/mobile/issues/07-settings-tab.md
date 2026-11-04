# Issue 07: 设置 Tab——外观 + 账户

状态:已完成(2026-11-04)

## 需求

- 外观：明暗（跟随系统/浅/深）、语言（中/英）、界面缩放（100/110/120%）、主题族、主题色。
- 账户：账户信息、立即同步、登出。
- 不做：Vim 开关、编辑器字号、导出备份。

## 实现

- `prefs.rs`：`MobilePrefs`（appearance/light_theme/dark_theme/lang/zoom）JSON 存 `mobile-prefs.json`。
- `theme.rs`：kit `ThemeRegistry` + 与桌面同一批 23 套 Navop JSON（`include_str!` 直引 leanote-app/themes/，等用户 ADR-0014 重构落地后移共享目录）；`apply()` 设主题对 + mode（follow 读窗口外观）+ `theme.font_size = 14×zoom`（rem 缩放，桌面 issue 24 同机制）。
- **ADR-0014 对齐**：桌面主题族已由用户重构为 Navop JSON 体系（theme_family.rs 已删、theme_registry.rs 新增，进行中）。移动端按新体系接入；「主题色」随之变为「跟随主题 primary」（桌面新语义），自定义 accent 色板不再单列——与用户 WIP 保持一致。
- `settings_tab.rs`：外观四行（chips/循环选择器）+ 账户（email/host、立即同步、登出）。事件回 MobileApp：PrefsChanged（保存+应用+全局 i18n 切换）/SyncNow/Logout（deactivate_account + 回登录页）。
- `i18n.rs`：移动端字符串中/英（全局原子量，免 per-tab 语言管道）；全部 UI 硬编码中文替换完毕。
- **leanote-core 修复**：`app_data_dir()` iOS 分支——原先落到 linux 的 `~/.config`，改 `HOME/Library/Application Support`（macOS 同款）；iOS 沙箱惯例。
- Tab 选中态：accent 底在 Navop Light 下近白不可读，改 `primary`/`primary_foreground`。

## 验证

- 设置屏截图：外观（跟随系统选中/Navop Light/Dark、100% 选中、中文选中）+ 账户（user@example.com + host、立即同步、登出）全部渲染正确。
- 端到端（容器注入 prefs：dark + Gruvbox Dark + en + 120%）：重启后 Gruvbox 暖深色 ✓、全 UI 英文 ✓、zoom 同一管线（lang/theme 已证实加载，font_size 由同一次 apply 设置）。
- 选中 Tab 像素采样 `#3b82f6` + 白字 ✓。
- 三平台 check 绿；8 测试套件回归 ok。

## Comments

- 设置项点击（chip/cycler/登出）无法自动化（simctl 无 tap）；PrefsChanged 管线已由容器注入 prefs 的端到端验证覆盖，按钮→事件→应用的最后一跳与已验证的 on_click 机制相同，issue 09 手动走查。
