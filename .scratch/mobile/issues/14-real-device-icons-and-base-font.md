# Issue 14: 真机图标全部消失 + 默认界面字体过小

状态:已完成(2026-11-07;真机复验待用户)

## 需求

真机报告：

1. iOS 真机上图标（Tab 栏、汉堡、同步 ⟳、色板勾选等）全部不显示；模拟器正常。
2. 默认界面字体太小，调大。

## 根因

### 1. 图标消失：rust-embed 在 debug 构建从文件系统读资产

`gpui-kit-assets` 的图标目录用 `rust-embed`（`RustEmbed` derive）打包。rust-embed 的行为分裂：
`debug_assertions` 构建下**不内嵌**，运行时按编译期记录的绝对路径读文件——即编译机上的
`~/.cargo/registry/src/…/gpui-kit-assets-0.7.3/assets/icons/*.svg`。

- 模拟器：进程跑在 macOS 上，registry 路径可读 → 图标正常（掩盖问题；issue 02/09 走查全是模拟器）。
- 真机：该路径不存在 → `AllAssets::get()` 全返回 `None` → `Icon` 静默不渲染（`AssetSource::load`
  报错被 img 元素吞掉）。

project.yml 真机调试指向 `target/aarch64-apple-ios/debug/libleanote_mobile.a`（debug profile，
debug_assertions 开）→ 必现。Android debug 真机同病（设备上同样没有 registry 路径）。

### 2. 字体小：移动端复用桌面 14px rem 基准

`theme.rs` 的 `theme.font_size = 14px × zoom` 抄自桌面。移动端全部文字按 rem 表达
（行标题 rems(0.95)=13px、次级 rems(0.75)=10.5px、Tab 标签 rems(0.65)=9.1px），在手机上
整体偏小一档以上。阅读正文（TextView）同样吃窗口 rem，一并偏小。

## 实现

- `Cargo.toml`（leanote-mobile）：新增 `rust-embed = { version = "8.7.2", features = ["debug-embed"] }`。
  不引用其代码——借 cargo feature 统一，把依赖图里同一个 rust-embed 实例（gpui-kit-assets 的）
  开 `debug-embed`，debug 构建也内嵌资产。一行修复，无 vendoring。
- `theme.rs`：rem 基准 14 → `16 × zoom`。全 UI 文字等比 +14%：行标题 15.2px / 次级 12px /
  Tab 标签 10.4px / 阅读正文 16px——对齐 iOS 字号阶梯（subheadline 15 / caption 12 / tab ~10）。
  界面间距全是 `px()` 不随基准变，无布局连锁；zoom 档位（100/110/120%）语义不变仍在基准之上乘算。
- 回归测试 `mobile_rem_base_is_16_not_desktop_14`：断言基准 16 及 zoom 乘算，防悄悄退回桌面值。

## 验证

- `cargo tree -i rust-embed -e features`：依赖图中唯一 rust-embed v8.13.0 已带 `debug-embed`
  （leanote-mobile → feature 统一）。
- `cargo build -p leanote-mobile --target aarch64-apple-ios` 后
  `grep -ac '<svg' …/debug/libleanote_mobile.a` = 1944、资产路径键在内——SVG 内容字节确已进
  debug 二进制（文件系统读取模式下只会有路径字符串，零内容字节）。
- `cargo test -p leanote-mobile`：6/6 绿（含新回归）；`cargo check --target aarch64-apple-ios` 绿
  （CI 同款门禁）。
- 待用户真机复验：图标显示 + 字号观感。
