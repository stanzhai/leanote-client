//! 移动端主题（issue 07）：kit `ThemeRegistry` + 与桌面同一批 Navop JSON
//! （ADR-0014）。accent 跟随主题 primary（JSON 自带），不再手工覆盖。

use crate::prefs::MobilePrefs;
use gpui_kit::component::{Theme, ThemeMode, ThemeRegistry};
use gpui_kit::{App, Window};

/// 与桌面 leanote-app/themes 同一批文件（用户 ADR-0014 重构落地后可移到
/// 共享目录；现在引用其路径避免与 WIP 冲突）。navop 在前 = 默认。
const BUNDLED: &[&str] = &[
    include_str!("../../leanote-app/themes/navop.json"),
    include_str!("../../leanote-app/themes/adventure.json"),
    include_str!("../../leanote-app/themes/alduin.json"),
    include_str!("../../leanote-app/themes/asciinema.json"),
    include_str!("../../leanote-app/themes/aurora.json"),
    include_str!("../../leanote-app/themes/ayu.json"),
    include_str!("../../leanote-app/themes/catppuccin.json"),
    include_str!("../../leanote-app/themes/everforest.json"),
    include_str!("../../leanote-app/themes/fahrenheit.json"),
    include_str!("../../leanote-app/themes/flexoki.json"),
    include_str!("../../leanote-app/themes/gruvbox.json"),
    include_str!("../../leanote-app/themes/harper.json"),
    include_str!("../../leanote-app/themes/hybrid.json"),
    include_str!("../../leanote-app/themes/jellybeans.json"),
    include_str!("../../leanote-app/themes/kibble.json"),
    include_str!("../../leanote-app/themes/macos-classic.json"),
    include_str!("../../leanote-app/themes/matrix.json"),
    include_str!("../../leanote-app/themes/mellifluous.json"),
    include_str!("../../leanote-app/themes/molokai.json"),
    include_str!("../../leanote-app/themes/solarized.json"),
    include_str!("../../leanote-app/themes/spaceduck.json"),
    include_str!("../../leanote-app/themes/tokyonight.json"),
    include_str!("../../leanote-app/themes/twilight.json"),
];

/// 注册 + 应用偏好：主题对、明暗模式（follow 读窗口外观）、缩放（rem 基准）。
pub fn apply(prefs: &MobilePrefs, window: Option<&mut Window>, cx: &mut App) {
    for source in BUNDLED {
        let _ = ThemeRegistry::global_mut(cx).load_themes_from_str(source);
    }
    let registry = ThemeRegistry::global(cx);
    let light = registry
        .themes()
        .get(prefs.light_theme.as_str())
        .cloned()
        .unwrap_or_else(|| registry.default_light_theme().clone());
    let dark = registry
        .themes()
        .get(prefs.dark_theme.as_str())
        .cloned()
        .unwrap_or_else(|| registry.default_dark_theme().clone());
    let zoom = prefs.zoom;
    Theme::update(cx, move |theme| {
        theme.light_theme = light;
        theme.dark_theme = dark;
        // 缩放：kit Root 每帧从 theme.font_size 重订窗口 rem（桌面 issue 24 同款）。
        theme.font_size = gpui_kit::px(14. * zoom);
    });
    let mode = match prefs.appearance.as_str() {
        "light" => ThemeMode::Light,
        "dark" => ThemeMode::Dark,
        // follow：启动期 keyWindow 未建，优先读窗口自身外观；
        // 运行期（设置面板触发）走 App 级外观（keyWindow 已存在）。
        _ => {
            let dark = window
                .map(|w| w.appearance() == gpui_kit::WindowAppearance::Dark)
                .unwrap_or_else(|| cx.window_appearance() == gpui_kit::WindowAppearance::Dark);
            if dark { ThemeMode::Dark } else { ThemeMode::Light }
        }
    };
    Theme::change(mode, None, cx);
}

/// 主题名（选择器用）：注册表实读，(name, dark)。Navop 默认在前。
pub fn theme_names(cx: &App) -> Vec<(gpui_kit::SharedString, bool)> {
    let mut out: Vec<(gpui_kit::SharedString, bool)> = ThemeRegistry::global(cx)
        .themes()
        .iter()
        .map(|(name, cfg)| (name.clone(), cfg.mode.is_dark()))
        .collect();
    out.sort_by_key(|(n, _)| {
        (
            n.as_ref() != crate::prefs::DEFAULT_LIGHT && n.as_ref() != crate::prefs::DEFAULT_DARK,
            n.clone(),
        )
    });
    out
}
