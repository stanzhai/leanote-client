# Issue 18: 移动端应用图标 + 包名前缀 site.stanzhai.leanote

状态:已完成(2026-11-07;真机装包复验待用户)

## 需求

1. 移动端应用添加 icon——此前的「图标」是 gpui-mobile 模板的几何占位图（嵌套方框
   +斜线），不是品牌 logo。
2. 移动端包名前缀 `com.leanote` → `site.stanzhai.leanote`。

## 实现

### 图标（源 = 仓库 `logo/icon-black-white.png`，桌面打包同源，黑底白蚁标）

- iOS：`AppIcon-1024.png` 重新生成（1024×1024、灰度无 alpha，满足 actool 单尺寸
  图标要求）；**顺带修了一个真 bug**——project.yml 把 `Assets.xcassets` 标成
  `type: folder`（文件夹引用、原样拷贝进 .app），actool 从未编译过它，主屏图标
  实际一直不生效（旧 .app 无 Assets.car、Info.plist 无 CFBundleIcons）。改为正常
  资源源（actool 编译）。
- Android：五档 mipmap（48/72/96/144/192）`ic_launcher.png` +
  `ic_launcher_round.png` 全部由同源生成；splash（API 31+ 的
  `windowSplashScreenAnimatedIcon` 与 API <31 的 layer-list）都引用
  `@mipmap/ic_launcher`，换图即全覆盖。

### 包名

- iOS：`project.yml` `bundleIdPrefix: site.stanzhai.leanote` +
  `PRODUCT_BUNDLE_IDENTIFIER: site.stanzhai.leanote.mobile`，xcodegen 重生成工程。
- Android：`build.gradle.kts` `namespace`/`applicationId` →
  `site.stanzhai.leanote.mobile`（manifest 无 package 属性、Java 类在
  `dev.gpui.mobile` 包下，无连带）。
- `rust/README.md` 两处 simctl 命令同步。

## 验证

- iOS `xcodebuild -sdk iphonesimulator` BUILD SUCCEEDED；产物检查：
  `CFBundleIdentifier = site.stanzhai.leanote.mobile`、`Assets.car` 存在、
  `CFBundleIcons`（AppIcon）注册、actool 渲染出的 `AppIcon60x60@2x.png`
  视觉确认为蚂蚁标。
- Android `./build.sh` BUILD SUCCESSFUL；`aapt2 dump badging`：
  `package: name='site.stanzhai.leanote.mobile'`，APK 内 mipmap-xxxhdpi
  192px 图标确认为蚂蚁标。
- 换包名后签名/注册注意：TestFlight 需在开发者后台注册新 bundle id
  （issue 09 的记录基于旧 id）；已装的旧包不会自动覆盖，需卸载重装。
