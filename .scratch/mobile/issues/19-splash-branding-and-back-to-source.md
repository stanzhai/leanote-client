# Issue 19: 启动屏改为 logo + Leanote 文案；阅读屏返回回到来源列表

状态:已完成(2026-11-07;Android 真机/模拟器观感待用户复验——词标是 stroke 矢量)

## 需求

1. splash 屏显示 logo + "Leanote" 文案（此前 iOS 是 "GPUI iOS" 文本，Android 只有居中图标）。
2. 笔记详情的返回要回到**来源**：搜索列表进 → 回搜索 Tab；笔记本进 → 回笔记本列表（原本行为正确）。

## 方案

### 1. 启动屏

- **iOS**（`LaunchScreen.storyboard`）："GPUI iOS" 标签 → `SplashLogo` 图片（120pt，居中略偏上）+
  下方 "Leanote" bold 28pt 标签。图源 `logo/icon-black-white.png` 以亮度作 alpha 派生两版
  （浅色模式黑蚁标 / 深色模式白蚁标），imageset 带 luminosity 外观变体，背景仍是
  systemBackgroundColor（深浅自适应）。
- **Android**（`drawable/splash_logo.xml`）：layer-list = 56dp 蚁标图标 + 下方
  `splash_wordmark.xml`（"Leanote" stroke 路径矢量，白色 round cap；不引字体文件），
  整体控制在 ~108dp 图标容器内，`values/` 与 `values-v31/` 两套 `GpuiSplashTheme` 的
  `windowSplashScreenAnimatedIcon` 均改指它。删除死资源 `splash_background.xml`、
  `splash_icon.xml`（模板遗留，无引用）。

### 2. 返回来源

`NotesTab` 增 `reader_from_search` 标志：`open_reader`（搜索结果入口）置真；笔记 Tab 内部
两处 push Reader（最近列表 / 笔记本列表行）置假。`pop()` 弹出 Reader 且标志为真时发
`NotesEvent::ReturnToSearch`，MobileApp 订阅处把 TabShell 切回 `TabId::Search`。
返回按钮与左缘右滑（issue 17）都走 `pop()`，同一处生效。
## 追修（同日）：splash 图标居左

用户复验发现 iOS splash 图标居左。根因：手写 storyboard 时丢失了 imageView 的
centerX 约束（首次编辑把原 label 的 centerY 约束行误当注释位覆盖），只剩 centerY
约束 → imageView 落在 x=0。另外首次 xcodebuild 产物 Assets.car 里没有 SplashLogo
（xcodegen 工程是老文件清单，新 imageset 没进编译）——需 `xcodegen generate` 后重编。
验证：模拟器上用 20 行 UIKit harness 加载 app bundle 内的 LaunchScreen.storyboardc
并 dump 视图树——修复前 imageView frame=(0,120)，修复后 (100,120)（320 宽，正中），
label 全宽居中、SplashLogo 512px 正常解析。真机/模拟器冷启动观感待用户复验。
## 已知边界

- `cargo test -p leanote-mobile` 11/11 绿（新增 `reader_pop_returns_to_origin`：搜索进入
  pop → ReturnToSearch 恰一次；内部进入 pop → 不发）。
- iOS：`ibtool --compile` storyboard 绿、`actool` 编译 Assets.xcassets 绿（含 SplashLogo）。
- Android：`aapt2 compile + link`（带 core-splashscreen AAR res）绿，无未解析引用。

## 已知边界

- Android 12+ 系统 splash 对 icon 容器有缩放/圆形遮罩语义，词标观感需真机确认；不合适就
  回退 `@mipmap/ic_launcher`（一行样式改回）。
