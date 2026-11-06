//! 移动端主题（issue 07）：kit `ThemeRegistry` + 与桌面同一批 Navop JSON
//! （ADR-0014）。对齐桌面修订版模型：单 `ui_theme` 族干驱动双外观，
//! 变体自动匹配；强调色默认跟随主题 primary，色板可覆盖。

use crate::prefs::MobilePrefs;
use gpui_kit::component::theme::Colorize;
use gpui_kit::component::{Theme, ThemeMode, ThemeRegistry};
use gpui_kit::{hsla, App, Hsla, Window};

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

/// 桌面 theme.rs 同款色板（"black" 是真实的黑色圆片，不是哨兵）。
pub const ACCENTS: &[(&str, u32)] = &[
    ("blue", 0x3b82f6),
    ("purple", 0xbf5af2),
    ("pink", 0xff2d92),
    ("red", 0xff453a),
    ("orange", 0xff9f0a),
    ("yellow", 0xffd60a),
    ("green", 0x32d74b),
    ("black", 0x000000),
];

/// 桌面解析规则（ADR-0014 修订）：精确名（单模主题，模式需匹配）→
/// "{trunk} Light"/"{trunk} Dark" 兄弟变体 → 该模式 Navop 默认。
fn resolve(
    registry: &ThemeRegistry,
    trunk: &str,
    dark: bool,
) -> std::rc::Rc<gpui_kit::component::ThemeConfig> {
    // 选择器把 Navop 对显示为 "Default" 并可能落盘；kit 自带一对
    // "Default Light/Dark"（primary 近黑），必须在变体匹配前收编为 Navop，
    // 否则 Default 主题整个 UI 变黑（桌面无此坑：存的是族干 "Navop"）。
    let trunk = match trunk {
        "" | "Default" | "Navop" => "Navop",
        t => t,
    };
    let themes = registry.themes();
    if let Some(cfg) = themes.get(trunk) {
        if cfg.mode.is_dark() == dark {
            return cfg.clone();
        }
    }
    let variant = format!("{trunk} {}", if dark { "Dark" } else { "Light" });
    if let Some(cfg) = themes.get(variant.as_str()) {
        return cfg.clone();
    }
    let fallback = format!("Navop {}", if dark { "Dark" } else { "Light" });
    themes.get(fallback.as_str()).cloned().unwrap_or_else(|| {
        if dark {
            registry.default_dark_theme().clone()
        } else {
            registry.default_light_theme().clone()
        }
    })
}

/// 选中色片上仍可读的前景（桌面 accent_foreground 同规则）：亮色
/// （黄/绿/橙）配黑，其余配白。
pub fn accent_foreground_for(hex: u32) -> Hsla {
    let [r, g, b] = [(hex >> 16) & 0xff, (hex >> 8) & 0xff, hex & 0xff];
    let lum = (0.299 * r as f32 + 0.587 * g as f32 + 0.114 * b as f32) / 255.;
    if lum > 0.55 {
        hsla(0., 0., 0., 1.)
    } else {
        hsla(0., 0., 1., 1.)
    }
}

/// 自定义强调色覆盖（桌面 apply_wake_theme 同款配方）：hover lighten
/// 0.1、active darken 0.1、selection 25% 透明。必须在 `Theme::change`
/// 之后写——模式切换会重载配置并覆盖。tokens 整体从 colors 重建：
/// reconcile 只保留两侧一致的字段（桌面 glass/button_primary 同坑）。
fn apply_accent_override(id: &str, cx: &mut App) {
    let Some((_, hex)) = ACCENTS.iter().find(|(k, _)| *k == id) else {
        return;
    };
    let accent: Hsla = gpui_kit::rgb(*hex).into();
    let fg = accent_foreground_for(*hex);
    Theme::update(cx, |theme| {
        theme.colors.primary = accent;
        theme.colors.primary_hover = accent.lighten(0.1);
        theme.colors.primary_active = accent.darken(0.1);
        theme.colors.primary_foreground = fg;
        theme.colors.button_primary = accent;
        theme.colors.button_primary_hover = accent.lighten(0.1);
        theme.colors.button_primary_active = accent.darken(0.1);
        theme.colors.button_primary_foreground = fg;
        theme.colors.ring = accent;
        theme.colors.list_active_border = accent;
        theme.colors.drag_border = accent;
        theme.colors.selection = accent.opacity(0.25);
        theme.colors.accent = accent;
        theme.colors.accent_foreground = fg;
        // Markdown 链接色：无条件取有效强调色（跟随=主题 primary，
        // 覆盖=色板值）——桌面 c0110a96 同款。
        theme.colors.link = accent;
        theme.colors.link_hover = accent.lighten(0.1);
        theme.colors.link_active = accent.darken(0.1);
        theme.tokens = (&theme.colors).into();
    });
}

/// 注册捆绑主题（幂等）。只在首次注册时触碰注册表：kit 的
/// `ThemeRegistry` observer 会在 flush 期按当前名字重载活动主题，
/// 抹掉同步写入的颜色覆盖——apply 的强调色因此走 defer 重放。
pub fn register(cx: &mut App) {
    if ThemeRegistry::global(cx)
        .themes()
        .contains_key("Navop Light")
    {
        return;
    }
    for source in BUNDLED {
        let _ = ThemeRegistry::global_mut(cx).load_themes_from_str(source);
    }
}

/// 注册 + 应用偏好：主题族干、明暗模式（follow 读窗口外观）、缩放、强调色。
pub fn apply(prefs: &MobilePrefs, window: Option<&mut Window>, cx: &mut App) {
    register(cx);
    let registry = ThemeRegistry::global(cx);
    let light = resolve(registry, &prefs.ui_theme, false);
    let dark = resolve(registry, &prefs.ui_theme, true);
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
            if dark {
                ThemeMode::Dark
            } else {
                ThemeMode::Light
            }
        }
    };
    Theme::change(mode, None, cx);
    apply_accent(&prefs.accent, cx);
    // defer 重放（见 register）：首次注册触发的 observer 重载发生在
    // flush 期、晚于上面的同步写入；defer 排在其后，恢复最终强调色。
    // 后续偏好变更不再触碰注册表，同步写入已生效，重放幂等。
    let accent = prefs.accent.clone();
    cx.defer(move |cx| apply_accent(&accent, cx));
}

/// 强调色最终态（桌面 c0110a96 双分支）：
/// - 覆盖：整套 primary/accent/button/ring/selection/link 写色板值；
/// - 跟随：主题文件的 accent.background 是浅容器色而非“跟随”语义，
///   kit 表面 accent/link 统一取活动主题 primary。
fn apply_accent(accent: &str, cx: &mut App) {
    if !accent.is_empty() {
        apply_accent_override(accent, cx);
    } else {
        Theme::update(cx, |theme| {
            let primary = theme.colors.primary;
            let fg = theme.colors.primary_foreground;
            theme.colors.accent = primary;
            theme.colors.accent_foreground = fg;
            theme.colors.link = primary;
            theme.colors.link_hover = primary.lighten(0.1);
            theme.colors.link_active = primary.darken(0.1);
            theme.tokens = (&theme.colors).into();
        });
    }
}

/// 主题族干列表（选择器用，桌面 choices() 同款）：按 BUNDLED 文件序
/// （默认对第一）；成对 "X Light"/"X Dark" 合并为一行、默认对显示
/// "Default"；单模主题（Matrix、Tokyo Moon…）原名列出。注册表
/// themes() 是 HashMap（每次启动乱序），不能作顺序来源。
pub fn theme_trunks() -> Vec<gpui_kit::SharedString> {
    static ENTRIES: std::sync::LazyLock<Vec<(String, bool)>> = std::sync::LazyLock::new(|| {
        let mut v = Vec::new();
        for source in BUNDLED {
            if let Ok(set) = serde_json::from_str::<serde_json::Value>(source) {
                if let Some(themes) = set.get("themes").and_then(|t| t.as_array()) {
                    for theme in themes {
                        if let (Some(name), Some(mode)) = (
                            theme.get("name").and_then(|n| n.as_str()),
                            theme.get("mode").and_then(|m| m.as_str()),
                        ) {
                            v.push((name.to_string(), mode == "dark"));
                        }
                    }
                }
            }
        }
        v
    });
    let has = |name: &str| ENTRIES.iter().any(|(n, _)| n == name);
    let mut out: Vec<gpui_kit::SharedString> = Vec::new();
    for (name, dark) in ENTRIES.iter() {
        let (stem, sibling) = match name.strip_suffix(" Light") {
            Some(stem) => (stem, format!("{stem} Dark")),
            None => match name.strip_suffix(" Dark") {
                Some(stem) => (stem, format!("{stem} Light")),
                None => (name.as_str(), String::new()),
            },
        };
        let paired = !sibling.is_empty() && has(&sibling);
        if paired && *dark {
            continue;
        }
        let label = if paired {
            if stem == "Navop" {
                "Default"
            } else {
                stem
            }
        } else {
            name
        };
        if !out.iter().any(|t| *t == label) {
            out.push(label.into());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 注册表 themes() 是 HashMap（乱序）。选择器列表必须按 BUNDLED
    /// 文件序输出（默认对第一、显示 "Default"），成对主题合并、单模
    /// 原名列出。
    #[gpui_kit::test]
    fn trunk_list_is_file_ordered(cx: &mut gpui_kit::TestAppContext) {
        cx.update(gpui_kit::init);
        cx.update(|cx: &mut App| {
            for source in BUNDLED {
                assert!(
                    ThemeRegistry::global_mut(cx)
                        .load_themes_from_str(source)
                        .is_ok(),
                    "bundled theme must load into the kit registry"
                );
            }
            let themes = ThemeRegistry::global(cx).themes();
            assert!(themes.contains_key("Navop Light"), "navop.json registered");
            assert!(themes.contains_key("Navop Dark"));
        });
        let trunks = theme_trunks();
        assert_eq!(
            trunks.first().map(|t| t.to_string()),
            Some("Default".into()),
            "file order: the Navop pair leads as Default"
        );
        assert_eq!(
            trunks.get(1).map(|t| t.to_string()),
            Some("Adventure".into())
        );
        assert!(trunks.iter().any(|t| *t == "Gruvbox"));
        assert!(
            !trunks.iter().any(|t| *t == "Navop"),
            "pair collapses to Default"
        );
        // 单模主题原名列出（无 Light/Dark 配对）。
        assert!(trunks.iter().any(|t| *t == "Matrix"));
        assert!(trunks.iter().any(|t| *t == "Tokyo Moon"));
    }

    /// 回归：选择器把 Navop 对落盘为显示名 "Default"，而 kit 自带一对
    /// "Default Light/Dark"（primary 近黑）——必须收编为 Navop，否则
    /// 默认主题整个 tint 变黑（与桌面蓝色的差异来源）。
    #[gpui_kit::test]
    fn default_label_resolves_to_navop(cx: &mut gpui_kit::TestAppContext) {
        cx.update(gpui_kit::init);
        let mut p = MobilePrefs::default();
        p.ui_theme = "Default".into();
        for (dark, name) in [(false, "Navop Light"), (true, "Navop Dark")] {
            p.appearance = if dark { "dark" } else { "light" }.into();
            cx.update(|cx: &mut App| apply(&p, None, cx));
            cx.update(|cx: &mut App| {
                let theme = Theme::global(cx);
                assert_eq!(
                    theme.theme_name().to_string(),
                    name,
                    "kit 'Default' must not win"
                );
                assert_eq!(
                    theme.colors.primary,
                    gpui_kit::rgb(0x3b82f6).into(),
                    "Navop primary stays blue"
                );
            });
        }
    }

    /// 桌面对齐（c0110a96）：跟随模式 kit 表面 accent/link 取主题
    /// primary；色板覆盖时整套 primary/accent/button/ring/link 变色且
    /// tokens 同步；切回跟随恢复主题 primary。
    #[gpui_kit::test]
    fn accent_follow_and_override_match_desktop(cx: &mut gpui_kit::TestAppContext) {
        cx.update(gpui_kit::init);
        cx.update(|cx: &mut App| apply(&MobilePrefs::default(), None, cx));
        let theme_primary = cx.update(|cx: &mut App| {
            let theme = Theme::global(cx);
            assert_eq!(
                theme.colors.accent, theme.colors.primary,
                "follow: accent must be the theme primary, not the file's pale accent.background"
            );
            assert_eq!(theme.colors.link, theme.colors.primary);
            theme.colors.primary
        });

        let mut p = MobilePrefs::default();
        p.accent = "red".into();
        cx.update(|cx: &mut App| apply(&p, None, cx));
        let red: Hsla = gpui_kit::rgb(0xff453a).into();
        cx.update(|cx: &mut App| {
            let theme = Theme::global(cx);
            for (label, got) in [
                ("primary", theme.colors.primary),
                ("accent", theme.colors.accent),
                ("button_primary", theme.colors.button_primary),
                ("ring", theme.colors.ring),
                ("link", theme.colors.link),
            ] {
                assert_eq!(got, red, "override: {label} must take the red swatch");
            }
            assert_eq!(
                theme.tokens.primary.color, red,
                "tokens side must follow (reconcile keeps agreed fields only)"
            );
        });

        cx.update(|cx: &mut App| apply(&MobilePrefs::default(), None, cx));
        cx.update(|cx: &mut App| {
            assert_eq!(
                Theme::global(cx).colors.primary,
                theme_primary,
                "back to follow restores the theme's own primary"
            );
        });
    }
}
