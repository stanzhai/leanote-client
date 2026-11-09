# 更新日志 / Changelog



## 3.1.16（2026-11-09）

- **修复：导出/另存为记住的目录比所选目录高一级**：3.1.2 的目录记忆存在双重取父 bug——两个调用点传入的已是所选文件的**目录**（`path.parent()`），`remember_export_dir` 内部又 `parent()` 一次，prefs 里实际存的是该目录的上一级。现按调用方语义原样存储。顺带修复 Windows 家目录：`home_dir()` 此前只读 `HOME`（Windows 不设该变量，回落 temp 目录），补 `USERPROFILE` 兜底。


## 3.1.14（2026-11-09）
- **重构：仓库目录扁平化，`rust/` 提升为项目根**——纯 Rust 仓库（Electron/JS 代码已全部移除）。`Cargo.toml`、`crates/`、`fake-server/`、`mobile/`、`packaging/`、`scripts/`、`vendor/` 移到根目录；CI 工作流、打包脚本、`.gitignore`、js_parity 测试的 fixtures 路径同步更新；根 README 以原 rust/README 为基础重写。行为无变化，`cargo test --workspace` 全绿。
- **修复：单模主题（Matrix、Kibble、Jellybeans、Harper、Fahrenheit、Catppuccin 系列等）在设置里选了不生效**：设置对话框的主题下拉订阅闭包把 `Confirm` 载荷拿去**对话框打开时捕获的旧 items 列表**里查位置再取值——而外观行就在主题行上方，对话框内切外观（如 跟随系统/浅色 → 深色）会按新外观热更新下拉列表（新增 Matrix 等深色单模主题），之后选中的载荷在新列表、闭包却查旧列表 → `position()` 落空 → **pick 静默丢弃**，主题停留在上一个。成对主题（Gruvbox 等）标签在所有过滤列表里都有所以幸免——这正是只有单模主题（全部深色单模 + 浅色 Catppuccin Latte/Aurora Light）踩坑的原因。修复：载荷本身就是选项名，直接 `set_ui_theme(name)`，删掉多余映射（移动端本就直接用值，无此 bug）。回归测试 `boot_and_switch_single_mode_theme` 钉住单模主题的启动解析与单模间切换。
- **修复：Android 模拟器 macOS 物理键盘无法输入文字（issue 10）**：两个前提 + 一个断点。前提：AVD `hw.keyboard = no` 默认关闭宿主键盘转发（已改 yes），模拟器需重启生效。断点：硬件键事件到达 gpui 后只以 KeyDown 派发，而 gpui 编辑器插入文字仅走 input handler（IME 路径），裸 KeyDown 永不插入（iOS 侧 `handle_text_input` 一直先走 handler）。vendored gpui-pre-mobile Android 键事件闭包补齐：可打印字符（非控制符）DOWN 时直接 `replace_text_in_range` 进聚焦输入框，无 handler 再回退 KeyDown；退格/方向/Home/End 等控制键保持 KeyDown 路由由编辑器键绑定处理。验证：模拟器 + macOS 键盘（AppleScript keystroke 端到端）输入 "hi" 正确落入登录邮箱框、光标跟随；软键盘 IME 路径未动。
- **修复：Android 顶栏与状态栏重叠导致无法点击（笔记本切换、详情页返回键）**：vendored gpui-pre-mobile 的 Android 窗口未把 `safe_area_insets` 接进 `PlatformWindow::insets()`（iOS 侧有 leanote 同款补丁，Android upstream 留默认 0）——app 各屏 `.pt(safe.top)` 全部拿到 0，顶栏画进系统状态栏区域被系统手势区挡住。Android 窗口补 `insets()` 实现（`safe_area_insets_logical()` 换算逻辑像素），布局管线与 iOS 同源。
- **修复：Android 模拟器/ES 3.0 设备卡 splash 无法进入应用（issue 10）**：三层原因。① 模拟器 host-GPU 翻译器仅 GLES 3.0（无 vertex storage buffer、compute 限值报 0），Vulkan 又不可用（device lost），GL 回退被 gpui-pre-wgpu 固定请求的 `downlevel_defaults()`（storage=4 / compute=65535）拒绝 → vendor `gpui-pre-wgpu`：GL 适配器缺 `VERTEX_STORAGE` downlevel flag 时改用 `downlevel_webgl2_defaults()`，`uses_webgl_instance_data()` 从 wasm-only 改为同 flag 能力判定（uint-texture 实例传输路径），桌面 GL 4.x / Vulkan / Metal 不受影响。② `app_data_dir()` 在 Android panic（无 HOME/XDG env）→ 新增 `LEANOTE_DATA_DIR` 覆盖，`android_main` 从 `AndroidApp::internal_data_path()` 注入应用私有 files 目录。③ Android 入口漏 `gpui_kit::init(cx)`（iOS 入口有）→ ThemeRegistry panic。验证：android-34 arm64 模拟器（`-gpu host`）登录页完整渲染、`init_done=true` 无 panic；host `cargo check` 绿。
- **主题列表随外观自适应（桌面 + 移动端）**：主题下拉不再全量列出 39 个主题——选**跟随系统**只列同时支持深浅两种风格的主题（成对 "X Light/X Dark"）；选**浅色**只列浅色风格主题（成对 + Catppuccin Latte 等浅色单模）；选**深色**只列深色风格主题（成对 + Matrix、Tokyo Moon 等深色单模），不支持当前外观的主题不再出现。切换外观时已选主题若不支持新外观（如 Matrix × 浅色）自动回落 Default 并即时刷新下拉（桌面 `choices_for` + `set_items` 热更新；移动端过滤同源、下拉惰性重建）。回归测试：`choices_filter_by_appearance` / `trunk_list_filters_by_appearance`。
- **移动端：启动屏 branding + 阅读屏返回来源（issue 19）**：① splash 改 logo + "Leanote" 文案——iOS 启动屏由模板 "GPUI iOS" 文本改为 `SplashLogo` 图片（`logo/icon-black-white.png` 亮度作 alpha 派生浅/深两版蚁标）+ "Leanote" 标签；Android 两代 splash API 的图标统一改 `splash_logo` layer-list（56dp 蚁标 + stroke 路径 "Leanote" 词标矢量，无字体依赖），顺手删除无引用的模板死资源 `splash_background.xml`/`splash_icon.xml`。② 阅读屏返回回到来源：搜索结果进入的阅读屏，返回按钮/左缘右滑均切回**搜索 Tab**（此前回的是笔记 Tab 根屏）——`NotesTab.reader_from_search` 标志 + `NotesEvent::ReturnToSearch` 事件，Tab 壳订阅切 Tab；笔记 Tab 内部进入的返回行为不变（回所在列表）。回归测试 `reader_pop_returns_to_origin` 钉住两种来源的返回语义。
- **移动端：应用图标 + 包名前缀 site.stanzhai.leanote（issue 18）**：两端图标此前是 gpui-mobile 模板的几何占位图（嵌套方框+斜线），现统一从桌面打包同源的 `logo/icon-black-white.png`（黑底白蚁标）生成——iOS `AppIcon-1024.png`（1024 单尺寸、无 alpha），Android 五档 mipmap（48–192px，ic_launcher + round；splash 两代 API 均引用 @mipmap/ic_launcher 自动覆盖）。**顺带修复 iOS 主屏图标从未生效的真 bug**：project.yml 把 `Assets.xcassets` 标成 `type: folder`（原样拷贝、actool 不编译，旧 .app 无 Assets.car、Info.plist 无 CFBundleIcons），改为正常资源源。包名：iOS `PRODUCT_BUNDLE_IDENTIFIER`/`bundleIdPrefix` 与 Android `namespace`/`applicationId` 统一 `site.stanzhai.leanote.mobile`（xcodegen 重生成工程；README simctl 命令同步）。验证：iOS 模拟器构建产物含 Assets.car + CFBundleIcons、actool 渲染位图确认为蚁标；Android `aapt2 dump badging` 包名正确、APK 内 192px 图标确认为蚁标。注意：新 bundle id 需在开发者后台注册（TestFlight），旧包需卸载重装。
- **移动端：屏幕左缘右滑返回（issue 17）**：推入栈的浏览屏（笔记详情/笔记本/标签/列表）此前只能点返回按钮。实现纯 GPUI 层的边缘手势识别——触摸 pan 在平台层已被手势识别器转成 ScrollWheel 派发（vendored gpui-pre-mobile），非根屏容器挂 `on_scroll_wheel` 观察之：左缘 24px 内起手（且 delta 为 Pixels，天然排除真实滚轮的 Lines）+ 横向主导 + 累计右滑 ≥48px → pop 返回；纵向主导废弃手势交给列表竖滚，只观察不消费，阅读页滚动/左上角返回按钮不受影响，根屏不挂。iOS/Android 同一份代码，零 vendoring。回归测试注入原始 TouchEvent 走真实识别管线：中部起手不触发、左缘右滑 72px 返回（端到端，仅 UIKit 触摸下发之下一层）。
- **修复：移动端系统键盘无法收起 + 输入框回车语义（issue 16）**：iOS 键盘与焦点绑定（vendored gpui-pre-mobile 随 FocusGained/FocusLost 显示/收起），而移动端 UI 从未主动交还焦点——搜索框只挂 Change 防抖、登录表单无任何事件，键盘一旦弹出永远不收。修复：① 搜索回车（`InputEvent::PressEnter`）= 丢弃残留防抖立即定稿 + 焦点移出（收键盘），点结果行跳阅读屏前同样收；② 登录回车链 host→email→password→提交，提交时统一收键盘（失败回错误提示时键盘不再压在界面上）。窗口操作走**构造期捕获的 `AnyWindowHandle`**——不能走 `cx.active_window()`：iOS 平台是返回 None 的 stub（真机静默空转，测试环境同构复现将其暴露）。回归测试（真实测试窗口 + 真实 helper）：回车链焦点逐级转移、提交后无输入框持有焦点、回车立即出结果。
- **修复：移动端笔记本列表与桌面不一致（issue 15）**：移动端自排「按 parent 字符串分组排序 + 两档缩进」，与桌面（roots 按 Seq → 子挂父下按 Seq → 深度优先）语义完全不同——子笔记本被甩到全部顶级之后按父 id 字典序堆放、孙辈嵌进丢失、孤儿照样显示。根因修复：把桌面 `browse.rs` 的纯树构建 `NavNode` + `notebook_nodes` 迁入两端共用的 `leanote-core`（`src/nav.rs`，单测随迁），移动端 `notebooks()` 删自排改同源调用——单一实现，两端不可能再漂移；桌面调用点与导入同步切换，行为不变（97 测试绿）。移动端补真实 Store 回归：子必须紧跟父且 depth 1、后位 root 排在其父子树之后（修复前红）。
- **修复：移动端真机图标全部消失（issue 14）**：`gpui-kit-assets` 图标走 rust-embed，其 `debug_assertions` 构建不内嵌资产、运行时按编译期绝对路径读编译机文件系统（cargo registry 目录）——模拟器进程跑在 macOS 上读得到故一切正常，真机上路径不存在 → `AllAssets::get()` 全空、图标静默不渲染（Android debug 真机同病）。修复：leanote-mobile 依赖 `rust-embed` 开 `debug-embed` feature，借 cargo feature 统一让依赖图中同一 rust-embed 实例在 debug 构建也内嵌（一行，无 vendoring）。二进制级验证：`aarch64-apple-ios` debug 静态库内含 1944 处 `<svg` 内容字节（文件系统读取模式下只有路径字符串、零内容字节）。
- **移动端：默认界面字号整体调大（issue 14）**：rem 基准自桌面 14px 提到 16px——移动端全部文字按 rem 表达（此前行标题仅 13px、次级 10.5px、Tab 标签 9.1px，真机实测过小），等比 +14% 后行标题 15.2 / 次级 12 / Tab 标签 10.4 / 阅读正文 16px，对齐 iOS 字号阶梯（subheadline 15 / caption 12 / tab ~10）；界面间距全为 `px()` 不随基准变，缩放档位（100/110/120%）语义不变仍在基准之上乘算。回归测试钉住基准值防退回桌面 14。
- **默认主题（Navop Dark）深色模式整体提亮 + 左栏层次感 + 主流语法着色**：原深色整体过深，且侧栏 `#1e1e1e` 与内容区 `#171717` 仅差 7/255，窗口糊成一块平面。锚点重定（用户裁决）：侧栏 `#262626`、中栏/阅读区 `#333333`（参考 Navop 体系引入前「侧栏最深、亮度向内容逐层爬升」的分层设计，步长 13）；markdown 编辑器面同为 `#262626`——深色收敛为两级灰阶（`#262626` 沉浸面：侧栏/列表/表格/编辑器，`#333333` 窗口面）。其余深色值按新锚点均衡重排：popover/secondary `#3a3a3a`、hover `#474747`、active `#575757`、border `#4a4a4a`、滚动条拇指 `#545454`；饱和色不动。**语法着色**：Navop 原语法表只覆盖 keyword/string 等 10 个通用 token，markdown 的 capture 词汇（标题/强调/链接/行内代码/列表标记）一个都没定义，编辑器里 markdown 几乎无色；深色换 **One Dark**、浅色换 **GitHub Light** 主流标准配色，并补全 kit schema 原生支持的 markdown token——标题粗体着色、`*强调*` 斜体、`**粗体**`、行内 `代码`、链接文字/地址、列表标记、围栏分隔符各得其所。单一数据源不变：kit 组件与应用面板仍读同一份 navop.json（ADR-0014 修订 8）。
- **修复：markdown 渲染强调色跟随用户色板与主题色（双向）**：阅读/预览链接色渲染自 `colors.link`——该键仅在主题配置 apply 时回落求值一次（schema `apply_color!(link, fallback = primary)`）。`apply_wake_theme` 现在**无条件**写入 `link`/`link_hover`/`link_active`（取 `theme::accent()`：色板色或当前主题 primary），色板与跟随两种来源都动态作用于 markdown 渲染；渲染侧每帧从活动主题折算，写入即生效。
- **修复：强调色切回「跟随主题」后界面组件残留旧色板（整体排查）**：`set_accent` 此前只重放 wake overlay、不重载主题配置——而 overlay 的自定义分支会覆写 `primary`/`accent`/`ring`/`selection`/`button_primary`/`list_active_border`/`drag_border`，从色板切回跟随时这些字段没有任何东西把它们恢复成主题文件值，于是图标按钮、设置面板选中胶囊等 kit 组件钉死在旧色板上。修复：抽出 `reapply_theme_config`（重装注册表当前主题对 + 显式 apply_config 当前模式，`set_ui_theme` 原有逻辑复用），`set_accent` 在 overlay 前先跑它——自定义分支随后按需覆写，跟随则保留文件原值，双向都干净。移动端排查：`PrefsChanged` 每次全量 `theme::apply`（含配置重载），跟随恢复本就正确，仅补齐自定义色板下漏写的 `link` 三件套（桌面同款）。
- **修复：跟随主题时图标按钮（ghost）hover 不用主题强调色**：kit 的 ghost 按钮悬停底色 = `tokens.accent` 的 50% 洗色（button.rs `Ghost` 分支），而主题文件自带的 `accent.background` 键是深/浅选择蒙版（Navop Dark `#1e3a8a`、Light `#e0f2fe`），不是「跟随主题」语义里的强调色——ADR-0014 定义跟随 = 主题 **primary**（`#3b82f6`）。`apply_wake_theme` 现在把 `accent`/`accent_foreground` 的写入移出自定义分支、两种模式统一取 `theme::accent()`（色板色或主题 primary）：自定义行为不变，跟随模式下 ghost hover、其他 accent 消费者（缺省 list.hover 回退等）全部与主题强调色一致。回归测试补断言：跟随时 `colors.accent` == 注册表主题 primary。
- **移动端（iOS）：主题/强调色体系对齐桌面（issue 13）**：单字段 `ui_theme` 族干驱动双外观、强调色默认跟随主题 primary（彩虹圈）+ 8 色板覆盖、主题下拉按 BUNDLED 文件序合并成对主题（默认对显示 "Default"）；TabBar 选中 tint、芯片、按钮全链路取主题 primary。模拟器实测：偏好持久化、冷启恢复、明暗切换全绿。
- **修复：移动端 Default 主题整个 tint 变黑**：设置页把 Navop 对以显示名 `"Default"` 落盘 `ui_theme`，而 gpui-kit 自带一对同名 "Default Light/Dark" 主题（primary 近黑）——`resolve` 的变体匹配先命中内置主题，选中 Tab/按钮/链接全部变黑（桌面存族干 "Navop" 无此坑）。修复：`resolve` 把 `""`/`"Default"`/`"Navop"` 统一收编为 Navop 再匹配；回归测试断言明/暗两模式解析为 Navop 且 primary == `#3b82f6`。
- **移动端（iOS）：Xcode Metal 校验崩溃（issue 12）**：vendored `wgpu-hal`（30.0.1），`shader_barycentrics` 探测补 `respondsToSelector` 守卫——Xcode Metal API validation 包一层 `CaptureMTLDevice`（不声明该 selector），objc2 生成调用启动即 panic。升级 wgpu 时按 Cargo.toml 注释 re-vendor。

## 3.1.11（2026-11-04）

- **修复：大陆无法下载霞鹜文楷等宽**：字体 ttf 此前仅从 GitHub Releases 直链下载，大陆访问 GitHub/objects.githubusercontent.com 常不通导致设置选了字体却始终回退系统字体。改为按序尝试三个同源地址：GitHub 直链 → ghfast.top → gh-proxy.com（均为 GitHub release 加速代理，字节级同源，魔数校验防代理返回 HTML 错误页）；实测死首链后经 ghfast.top 成功取回 25,957,300 字节 ttf。jsDelivr 因 20MB 文件上限（该字体 25.9MB）不可用，清华/南大镜像站未收录此项目。
- **主题体系移植 Navop：6 个硬编码家族 → 39 个 JSON 主题（ADR-0014）**：把 [Navop](https://github.com/feigeCode/navop) 的主题设计整体搬来——其 23 个主题文件（39 个浅/深主题：Navop、Catppuccin、Gruvbox、Tokyo Night、Solarized、Flexoki、Everforest、Ayu、Matrix…）原样入库，经 gpui-kit 的 `ThemeRegistry` 注册加载；表面色、组件色、**每主题自带语法高亮**全部由主题文件经 `ThemeConfig::apply_config` 驱动，删除 `theme_family.rs` 家族表与手写语法 JSON，`apply_wake_theme` 瘦身为纯应用覆盖层（玻璃 token、字体/缩放、圆角）。应用面板的 `theme::material()` 四层改为镜像同一 JSON（同 kit 回退规则），面板与组件色彩同源一致。强调色改用 Navop 模型：主题自带强调色，设置页强调色行最前的**彩虹圆圈表示跟随主题自带强调色**（默认选中），其后 8 个纯色圆圈为手动覆盖（含 Navop 的 hover/active 配方）；设置页只有一个「主题」下拉——成对主题（X Light/X Dark）合并为一行，浅色外观自动用 Light 变体、深色自动用 Dark 变体，单模主题（Matrix 等）原名列出；prefs 单字段 `ui_theme` 存主题名（老配置 solarized → Solarized，其余落默认）。新增主题=放一个 JSON + 一行 include_str。修复同模式切换主题不刷新：`Theme::update` 内部 reload_mode=false，仅换 config 不会重载颜色，须显式 `apply_config` 当前模式（回归测试：真实 AppShell 窗口内切 Gruvbox 断言表面色与语法主题即时变化）；顺带修复强调色哨兵值与「black」色板（0x000000）碰撞导致选黑色板变成跟随主题的问题（哨兵改用 u32::MAX）。圆角跟随主题：Matrix 主题以 radius/radius.lg = 0 全局直角，但覆盖层把 radius_lg 硬写回 12、且离开 Matrix 时残留 0（apply_config 只在主题带 radius 键时才写）；现从注册表显式设两档圆角——主题给了就用主题值（Matrix=0 直角），没给用应用默认（6/12）。
- **修复：CLI 添加笔记重复创建「AI札」**：`leanote add` 离线路径打开本地库时硬编码 `user1`，而已登录账户是真实 UserId，导致「按标题+用户找笔记本」永远落空、每次都在 `user1` 名下新建一本 AI札（实测数据里已有一本真实 AI札 + 一本 `user1` 孤儿副本）。`open_store` 现按 app 启动同款两段式开库：先以 `user1` 探测 `accounts` 表的激活账户，再按其 UserId 重开（未登录仍回落 `user1`）。顺带收紧 `ensure_notebook`：只复用 `LocalIsDelete=0` 的活笔记本，用户删过 AI札 后再 add 不会把新笔记塞进已删笔记本（原查询不过滤删除标记）。已重打包 dist 并对真实数据副本冒烟：两条 add 均落入真实账户既有 AI札，零新增笔记本。
- **同步可见化：底部同步图标转圈 + 修复同步后当前笔记不刷新**：① 主窗口左下角同步按钮在后台同步进行中显示旋转的 RefreshCw（gpui-kit `Button::loading` 内建 Spinner，跟随系统「减弱动态效果」），tooltip 显示「正在同步…」，同步期间按钮防重复触发（`syncing` 状态 + 重入保护）。② 根因修复「笔记同步下来有变化，必须切换笔记才能看到最新」：`AppShell` 只有单一 `_task` 槽位，自动保存定时器（每次敲键 800ms 防抖）/导出面板/图片另存都会覆写该槽位、**丢弃正在运行的同步任务**——同步线程已把新内容写进数据库，但负责 `cx.notify()` 重绘的完成回调被取消，阅读区便一直显示旧文，直到用户切换笔记强制重绘。同步任务现改存独立 `sync_task` 槽位，任何 `_task` 写入都不再取消它；完成回调统一「停转圈 → 写状态 → invalidate_lists → notify」，阅读区每帧直查数据库，同步一落库即刷新当前查看内容（正在编辑的笔记不覆盖本地缓冲，仍走本地优先的冲突语义）。回归测试：真实 AppShell 窗口 + 进程内 fake server，同步启动后立即覆写 `_task`（复刻自动保存的取消路径），断言完成回调照常执行、转圈状态正确结束、拉取的新正文即阅读区所读行（旧单槽实现该测试红）。


## 3.1.10（2026-11-03）

- **修复：CI macOS 签名第二轮失败（code object is not signed at all / In subcomponent: leanote-cli）**：签名循环按 glob 顺序先签主程序 `Leanote`——现代 `codesign` 把「签主程序」视同签整个 `.app`，会校验 `Contents/MacOS/` 下全部同级可执行文件，遇到尚未签名的 `leanote-cli` 即报错。改为先签辅助可执行文件（跳过主程序），`.app` 整体签名时自会签上主程序。本地以 ad-hoc 身份逐字执行新代码块验证：helper → bundle → `--verify --deep --strict` 全通过。

## 3.1.9（2026-11-03）

- **修复：CI macOS 签名必败（SecKeychainItemImport: Unknown format in import）**：`package-macos.sh` 曾把证书写入 `mktemp` 生成的**无扩展名**临时文件再 `security import`；现代 macOS 的 `security import` 按文件扩展名判定格式，无 `.p12` 后缀时对完全合法的证书同样报 "Unknown format in import"——与 secret 内容损坏的报错一字不差，极具误导性。改为 `$(mktemp).p12`（本地实测：合法 p12 → `1 identity imported`；垃圾内容 → 仍报 Unknown format；openssl 默认参数生成的 p12 → 报 MAC verification failed）。`import` 失败时现按 security 的错误信息分类输出排查提示（secret 不是 .p12 的 base64 / 密码不符或 p12 需 legacy 参数 `-macalg sha1 -certpbe PBE-SHA1-3DES -keypbe PBE-SHA1-3DES`），README「发布签名」补充 secret 本地自查命令。

## 3.1.8（2026-11-02）

- **文本光标粗细与颜色（对齐 Zed）**：① 粗细——vendored 输入组件的光标宽度此前按系统分裂（macOS 1.5px / Windows 2px 逻辑像素，且绘制不做像素对齐），macOS 视觉过细、Windows 在 125%/150% 缩放下渲染发虚偏粗；现采用 Zed 的做法：全平台统一 2 逻辑像素 + 绘制时 `pixel_snap_bounds` 对齐到整数设备像素（细而清晰、跨平台一致）。② 颜色——光标由正文前景色改为**强调色**（`tokens.colors.accent`，即用户可选的 Wake 强调色，默认蓝），vim 块状光标同步变为强调色 45% 透明覆盖。
## 3.1.7（2026-11-01）

- **修复：Windows 下霞鹜文楷等宽不生效（Issue 39）**：旧安装器把 ttf 写进 `~/Library/Fonts` —— 一个 Windows 根本不扫描的 macOS 路径（Windows 的用户字体需落 `%LOCALAPPDATA%\Microsoft\Windows\Fonts` 且在 HKCU `...\CurrentVersion\Fonts` 注册表登记，两者都没做）。根除而非补丁：ttf 下载一次进应用数据目录，启动时经 `TextSystem::add_fonts` **进程内注册**（DirectWrite 内存字体引用 / CoreText CGDataProvider / cosmic-text 三平台同构），当次启动即生效、无需重启，也不再触碰系统字体目录/注册表。首次运行后台下载完成后 live 注册并 `refresh_windows()` 重绘；本地 PDF 导出的 Typst 字体商店同步注入该 ttf（排在系统字体前，用户字体优先）。删除「重启后生效」提示（i18n `FontRestartHint`），设置页仅在下载中/失败时提示。

- **窗口位置记忆完善（Issue 39）**：① 几何持久化不再只在窗口关闭钩子里 —— `observe_window_bounds`（移动/resize 均触发）防抖 500ms 落盘，⌘Q/崩溃/注销等不走 should-close 的退出路径不再丢最后一次位置；② 新增 `maximized` 字段，最大化关闭后下次启动恢复最大化（旧状态文件缺字段按普通窗口加载，serde default 兼容）；③ 显示器列表主屏排首：上次所在显示器已拔除时，`clamped()` 的回退居中落点固定为主屏幕（此前落 `displays.first()`，顺序无保证）。顺带清理 `start_window_drag` 里遗留的调试探针写入。

- **Vim 模式补齐 `I`/`A` 行首、行尾插入（Issue 40）**：NORMAL 模式下 Shift+i 在**行首第一个非空白字符**处进入插入（整行空白回落行首，与 `^` 同语义），Shift+a 在**行尾**进入插入；此前两键被当作未知键吞掉无动作。实现上将 `EnterInsert` 的三个布尔字段重构为 `InsertAt` 枚举（`i`/`a`/`o`/`O` 行为不变），`^` 的首个非空白计算抽为 `first_non_blank` 供 `I` 共用；visual 模式刻意不进（子集无 block-insert）。

## 3.1.6（2026-11-01）

- **修复：Windows 窗口控制按钮位置不对、点击失效（ADR-0011 修订）**：三层根因。① 自绘最小化/最大化/关闭按钮从左栏顶行右端（窗口左上角约 240px 处，违反 Windows 惯例）移到**根级 overlay，贴窗口最右上角**，登录/同步期（侧栏隐藏）也存在；每钮 `occlude()` 截断下方 Drag hitbox，点击恢复原生 `HTMINBUTTON`/`HTMAXBUTTON`/`HTCLOSE` 应答（旧布局按钮嵌在 Drag 区里被抢答成 HTCAPTION，点击只会拖窗口）。② 编辑器标题行整体曾是 Drag 区 → 标题输入与右侧工具钮（预览/帮助/更多）全部不可点；标题行改为纯 client 区，右端为 caption band 预留。③ **Windows 高 DPI 视口错位**（175% 实测）：创建窗口时 OS 的 DPI 上下文翻转（96↔168）使后端把物理像素尺寸记成逻辑尺寸——布局视口 1.75 倍于可见窗口，右 ~43% 界面画在窗外且鼠标永远点不到（工具钮「点击失效」的真实根因）。`main.rs win_dpi_resync` 启动时 Win32 实测 client 尺寸，`视口×scale ≠ 实际 client` 时以 ±2px `SetWindowPos` 强制 WM_SIZE 按当前 scale 重算（物理尺寸不变）；修复介入时退出几何按设备像素保存，跨启动窗口尺寸稳定。

- **修复：拖动 markdown 标题栏直接崩溃退出（0xC0000409）**：标题行拖窗口的按压交接必须用**异步 `PostMessageW`**。首版同步 `SendMessageW(WM_NCLBUTTONDOWN)` 把 DefWindowProc 的模态移动循环嵌进 gpui 正在分发鼠标事件的 wndproc——消息泵持借重入（WER dump 实锤 `window_procedure → handle_nc_mouse_down_msg → draw_window → window_procedure` 循环）→ 主线程栈溢出 → `__fastfail`。`PostMessageW` 把按压排队，移动循环改在消息循环层进入（与原生 HTCAPTION 按压同层）。同轮修复偶发启动栈溢出：`main()` 在 Windows 上把应用搬到 64MB 栈工作线程运行（MSVC 默认 1MB 主栈被编辑器渲染深度击穿，抓栈定位在 `render_editor` 标签条，无循环、纯预算耗尽；栈页按需提交零常驻开销）。

- **编辑器标题输入框支持拖动窗口**：按下后位移超过 4px 阈值即把按压交给系统拖拽（Windows 走上述异步交接，macOS/Linux 保持 `start_window_move`）；纯点击仍定位光标、按钮照常。侧栏顶行拖拽区同样加阈值，顺带消除「点按钮手抖 1px 拖走窗口」。

- **左栏顶行补应用身份（Windows/Linux）**：拖拽区最左端新增应用图标 + 「Leanote」名称（非交互，可随 Drag 区拖动，Windows 标题栏惯例）；macOS 维持红绿灯让位原状。

- **应用图标统一 + 去锯齿**：Windows exe 图标（`packaging/icon.ico`）与标题栏内嵌图标（`assets/app-icon.png`）改由 `gen-icons.sh` **同帧产出，128px 条目逐字节一致**；形态为不透明纯黑方块 + 白蚁形字标（用户裁决），边缘 LANCZOS 全灰阶抗锯齿（16px=68 级、32px=138 级灰度；此前阈值化 alpha 只有 0/255 二值边缘，小尺寸锯齿明显）。

- 回归测试 +3：`win_controls_overlay_top_right_in_all_phases`（登录期/编辑期断言 overlay 贴右上角、工具钮避让 caption band、身份行在位，macOS 反断言）、`title_input_press_drag_triggers_window_drag`（2px 摆动不拖、40px 恰好交接一次、输入组件不吞监听）、`startup_restores_notebook_selection_and_last_note`（恢复语义）。leanote-app 102/102，ADR-0011 修订留痕。

- **修复：Linux CI 测试崩溃（`title_input_press_drag_triggers_window_drag` panic "not implemented"）**：vendored `gpui-pre` 测试平台（`TestWindow`）的 `start_window_move` 是 `unimplemented!()`，而标题行拖拽测试在非 Windows 平台走 gpui 的 `start_window_move` 路径（macOS/Linux 无 WM_NCLBUTTONDOWN 交接）——macOS 本地同样复现，非 Linux 特有。改为无操作：headless 测试窗没有 OS 拖拽循环，计数由 app 侧 `BEGIN_DRAG_CALLS` 测试钩子完成，能证明「交接请求已发出」即可，真正的拖拽是 OS 手递手。

## 3.1.5（2026-10-30）

- **发布产物签名与公证（CI secrets 门控）**：GitHub Releases 的 mac 包此前仅 ad-hoc 签名，浏览器下载后带隔离属性，Gatekeeper 报「已损坏，应移到废纸篓」（macOS 15 起右键打开绕过也已移除）；Windows 安装器未签名触发 SmartScreen「Windows 已保护你的电脑」。两个打包脚本现支持正式签名：macOS 在 `MACOS_CERT_P12_BASE64`/`MACOS_CERT_PASSWORD`/`APPLE_ID`/`APPLE_APP_SPECIFIC_PASSWORD`/`APPLE_TEAM_ID` 齐备时，Developer ID + hardened runtime 签名 → notarization → staple（.app 与 DMG 各自公证装订；Info.plist 版本号剥离 tag 前缀 `v` 满足公证纯数字要求；keychain 创建幂等，失败重跑不再卡死）；Windows 在 `WINDOWS_CERT_PFX_BASE64`/`WINDOWS_CERT_PASSWORD` 存在时用 signtool 对两个 exe（打 zip 前签名，便携包内同为签名件）与 NSIS setup 做 SHA-256 + RFC3161 时间戳签名。secrets 未配置时行为与从前完全一致；证书已配但 Apple 凭据缺失直接失败，不静默回退未签名发版。生效需购置 Apple Developer Program（$99/年）与 Windows 代码签名证书，详见 ADR-0012。
## 3.1.3（2026-10-29）

- **导出 Word（.docx）+ 导出二级菜单**：笔记右键菜单与编辑器「更多 (⋯)」菜单中，原先平铺的导出 HTML / Markdown / PDF 三项折叠为「导出为…」二级子菜单（Markdown / HTML / PDF / Word 四项），并新增 **导出 Word**：本地纯 Rust 转换（knit-md-docx：pulldown-cmark 解析 + docx-rs 生成），支持标题/粗斜体/链接/图片/列表/任务列表/代码块/引用/GFM 表格/脚注；富文本笔记走与 PDF 相同的 HTML→Markdown 桥。标题中的尖括号会被转义后再拼接，避免被 markdown 解析器当作内联 HTML 吞掉（`T & <title>` 之类标题此前导出只剩 `T &`）。

- **跨平台拖拽窗口（标题行）**：编辑器标题行（含标题输入框的整行）成为窗口拖拽区——Windows 走 OS 原生 HTCAPTION（`window_control_area(Drag)` 命中），Linux 走 `start_window_move`，macOS 维持 AppKit 标题栏拖拽。标题输入框本身不吃鼠标按下（文本选择不受影响），拖拽生效于行的空白与按钮间隙；与导航头 44px、阅读条 28px/编辑条 10px 三个既有拖拽区行为一致。

## 3.1.2（2026-10-29）

- **UI 批次六项**：① **Markdown `code` 强调淡化**——行内代码底色改为主题强调色 10% 透明度（保色调不换色，正文可读），覆盖全部 5 个预览面；② **跟随系统主题实时生效**——原先 `let _ = observe_window_appearance(...)` 被立即丢弃导致退订，现 `detach()` 常驻，系统切换明暗无需重启；③ **全屏弹窗不透明**——历史/语法帮助/附件 4 个对话框补 `Material::Popover` 背景（主窗口毛玻璃不受影响）；④ **编辑器更多 (⋯) 菜单**——标题工具栏新按钮整合笔记右键菜单（复用 `note_row_menu`：移动/星标/博客/导出等）+ 附件 + 历史版本；⑤ **预览图片右键菜单**——阅读/预览/全屏/历史/帮助面的图片支持 **复制图片 / 另存为…**（复制写系统剪贴板；另存为走原生保存面板，结果在工具栏下方状态条反馈）；⑥ **记住窗口位置/大小**——退出记忆窗口几何，重启还原；记录的显示器已拔除时自动在剩余显示器居中。

- **UI 批次三项（第二批）**：① **修复 vim `x` 行尾删除**——NORMAL 模式块状光标停在行尾之后时 `x` 原先无动作，现按 vim 语义向左删除末字符（空行仍为无操作）；② **「源码」按钮改为「编辑」**——标题工具栏该按钮本义即「以 Markdown 编辑此笔记」，文案与图标（Code → SquarePen）同步更正，tooltip 中英双语同改；③ **设置面板 macOS 系统设置风格**——左侧分区改图标+文字左对齐的胶囊行（选中底色用用户强调色而非黑色，行内保留真实内边距），左右两栏之间加 1px 竖向分隔线（与主窗口分栏一致），栏宽 110→148；**右侧选中项同改强调色（含闪白修复）**——外观/磨砂/字体/语言/字号五处分段控件的选中底色原为 kit 默认近黑（neutral-900/50），根因是 `tokens.button_primary` 是独立于 `colors.primary` 的字段、主题覆盖只写了后者；首修只写 token 侧，被 `reconcile` 在每次明暗切换时用 `colors.button_primary` 的 kit 分模式默认值（暗色下近白）盖回，表现为选中色在强调色与白色间跳动，现 colors+tokens 两侧同值写入（同背景透明化的既有手法），并加 reconcile 回归测试；**强调色圆圈选中态改对号**——6 个色块的选中标记由描边圆环改为色块内白色对号（macOS 系统设置同款），选哪个色块哪个显示对号。

- **全屏预览去玻璃磨砂**：全屏预览（⌘T）容器此前不画背景，窗口级毛玻璃（NSVisualEffectView）直接透出，长文阅读时文字浮在桌面模糊层上。现补 `Material::Background` 不透明表面（与阅读/编辑栏同源），仅主窗口左侧导航保留毛玻璃。

- **修复：Mermaid 图表右键菜单不出现**——右键菜单此前只挂在 Markdown 图片节点通路（node.rs 的 `![]()` 渲染）上，而 Mermaid 图由自定义块渲染器直接构造 `img()`，永远经过不了该通路。现渲染器持有同一处理闭包并直接挂到图表 `img` 的 `on_click`（内存 SVG 无真实 URL，用合成 `mermaid://diagram.svg`，另存为文件名稳定）。五个预览面共用该渲染器，一处修复全部生效。


- **导出/另存为记住上次目录，默认 Downloads**：HTML / Markdown / PDF / .leanote 备份导出与图片「另存为…」的保存面板此前总是从家目录起步。现首次默认打开 `~/Downloads`（目录不存在时回退家目录），导出成功后记住所选目录写入 `prefs.json`（`export_dir`），下次面板从该目录打开；记住的目录已被删除/卸载时自动回退 `~/Downloads`。取消导出不改记忆。


- **使用帮助迁移 + 纯文本化**：① 「语法帮助」从斜杠（`/`）面板的命令格迁出（15 项，4×4 格），改为编辑器标题栏工具栏按钮（预览右侧，CircleQuestionMark 图标，tooltip「使用帮助」），随时可达、不再占命令格；② 帮助内容改为纯文本显示（等宽字体 + 行距排版，不再经 markdown 渲染——速查表里的语法示例本身会被解析成真实标题/粗体/列表，显示混乱），并扩充为三节：Markdown 语法速查（标题/强调/列表/引用/代码/链接/图片/分隔线/表格/任务/脚注/转义，按通用定义）、`/` 快捷菜单用法（过滤/方向键/Enter/Tab/Esc/Backspace）、其他提示（保存、Mermaid）。中英双语。对话框加宽至 520、限高 420 内滚动。

## 3.1.1（2026-10-29）

- **修复：斜杠面板交互四连**：① 面板改纯 icon 紧凑格（去文字标签），格 36×36 再收到 32×32、gap 4→2、内边距 6→4；② 方向键接管——方向键在输入组件层是 `Move*` 动作而非按键事件，此前面板收不到、光标只在编辑器里动，现经 `capture_action` 截下做 ←→↑↓ 网格导航；③ **回车/Tab 执行**——Enter 与 Tab 同样是动作（`Enter`/`IndentInline`），不经过按键事件层，上一轮的按键分支永远收不到，现于 `capture_action` 顶部加面板分支，选中命令真正可执行；④ Esc 取消时 `/` 及已输入过滤串回填编辑器（此前触发键被吞，`/` 无法正常输入）。

- **斜杠面板交互补全（issue 37）**：① **格子点击执行**——每格接 `on_click`（回调经 shell 实体 `update` 回写：`execute` 后关面板），面板容器 `occlude()` 防点击穿透到编辑器；② **强调色选中/hover**——选中格与 hover 态改用 `theme::accent()`/`accent_foreground()`（与常规菜单 MenuItem 同源，可随用户换强调色），替换原中性灰高亮；③ **面板随 icon 自适应**——格子 32→28，icon 显式 `Size::XSmall`（16px，不再跟随编辑器环境字号约 20px），面板尺寸公式同步；④ **即时提示**——hover 格子零延迟出原生 tooltip（`tooltip_show_delay(ZERO)` + 组件 `Tooltip` 视图），方向键/鼠标切换选中时面板底部标签条同步显示命令名（原生 tooltip 只跟鼠标，键盘切换看不到）。

- **修复：Mermaid 图下方大面积空白（二次根因）**：上一轮裁剪已生效，但裁剪用的光栅化以 `usvg::Options::default()`（**空字体库**）计算 ink 包围盒——所有 `<text>` 被丢弃，包围盒漏掉全部文字标签；而实际渲染（gpui 光栅化）加载系统字体，文字画在包围盒之外被裁掉。gantt 底部日期轴是纯文字带，恰是首当其冲的图族。现 `ink_viewbox` 同样加载系统字体，两侧光栅化一致。另：merman 对 gantt/pie/mindmap 等图族在自身 viewBox 内部预留恒定空白（gantt 底部约 50px 轴带与任务数无关），渲染出口统一按实际 ink 包围盒裁剪 viewBox/width/height（含负 origin 平移）；全部预览面与 PDF/HTML 导出共用该出口。**进程内缓存**：同源码+明暗的 SVG 结果有缓存，升级后首次渲染即用新逻辑，无需操作。

## 3.1.0（2026-10-29）

- **斜杠命令面板**：编辑器内输入 `/` 呼出 4×4 命令面板（16 项：标题/粗斜/代码/链接/列表/表格/图片/帮助等），↑↓ 选择、Enter 执行、Esc 关闭；取代原 Markdown 工具栏及其设置项（ADR-0011）。
- **Mermaid 图表**：预览、阅读视图、历史弹窗、全屏预览、导出 PDF/HTML 全链路渲染 mermaid 代码块（纯 Rust merman 实现）；解析失败回退为代码块并附一行错误。
- **macOS ⌘Q 退出**：原生应用菜单（服务/退出），⌘Q 直接退出应用。
- **Windows/Linux 窗口控制按钮**：左侧导航头部自绘最小化/最大化/关闭按钮，区域交给平台层做原生点击处理。
- **GitHub Release 打包**：macOS DMG 带拖入 Applications 的安装背景与图标布局；Windows NSIS 安装器新增桌面快捷方式。
- **图标重绘**：统一从 `logo/icon-black-white.png` 生成 —— mac 中性黑 glyph，Windows 黑底白色 glyph 圆角矩形，多尺寸 ico/icns/hicolor。
