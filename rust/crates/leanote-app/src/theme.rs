//! Wake visual language (ADR-0003): 4 material layers, 8 type sizes, 4px
//! spacing grid, panel/selection radii, Wake-blue accent, SF Pro + Menlo.
//! Dark/light follow the system; this module only carries the token values.

pub use crate::theme_family::{
    border, family_highlight, family_index, foreground, glass_base, material, muted_foreground,
    set_family, FAMILIES,
};
use gpui_kit::{hsla, px, Hsla, Pixels};

/// The four material layers (ADR-0003: titlebar/sidebar, list, background, popover).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Material {
    /// Layer 0: titlebar + left sidebar.
    Base,
    /// Layer 1: middle list column.
    List,
    /// Layer 2: right reading column / window background.
    Background,
    /// Layer 3: overlays (popovers, menus).
    Popover,
}

#[cfg_attr(not(test), allow(dead_code))]
pub const MATERIALS: [Material; 4] = [
    Material::Base,
    Material::List,
    Material::Background,
    Material::Popover,
];

/// The 8 type sizes (11/12/13/14/16/18/22/28), index 0 smallest.
#[cfg_attr(not(test), allow(dead_code))]
pub const FONT_SIZES: [f32; 8] = [11., 12., 13., 14., 16., 18., 22., 28.];

/// 4px spacing grid: n steps of 4px.
#[cfg_attr(not(test), allow(dead_code))]
pub fn spacing(n: i32) -> Pixels {
    px((n.max(0) as f32) * 4.)
}

/// Tiered radii (ADR-0003): panel 12 / selection 8.
pub const RADIUS_PANEL: f32 = 12.;
pub const RADIUS_SELECTION: f32 = 8.;

/// Issue 19 Q13: user-selectable accents. "blue" is the ADR-0003 default.
pub const ACCENTS: &[(&str, u32)] = &[
    ("blue", 0x3b82f6),
    ("purple", 0x8b5cf6),
    ("green", 0x22c55e),
    ("orange", 0xf97316),
    ("red", 0xef4444),
    ("pink", 0xec4899),
];

static CURRENT_ACCENT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0x3b82f6);

/// Set the current accent by id; unknown ids (incl. "") fall back to blue.
/// Callers re-apply the theme afterwards so derived colors update.
pub fn set_accent(id: &str) {
    let hex = ACCENTS
        .iter()
        .find(|(k, _)| *k == id)
        .map(|(_, v)| *v)
        .unwrap_or(0x3b82f6);
    CURRENT_ACCENT.store(hex, std::sync::atomic::Ordering::Relaxed);
}

/// The active accent color (Wake blue by default) — selection + actions only.
pub fn accent() -> Hsla {
    gpui_kit::rgb(CURRENT_ACCENT.load(std::sync::atomic::Ordering::Relaxed)).into()
}

/// Foreground color that stays legible on `accent()` (all six are mid-lightness;
/// white wins for every swatch).
pub fn accent_foreground() -> Hsla {
    hsla(0., 0., 1., 1.)
}

/// Sans follows the system (".SystemUIFont" = SF Pro on macOS).
/// UI round 11 #2: no separate mono face — the editor binds to the UI
/// font (sans_for) so the font pref applies everywhere at once.
pub const SANS: &str = ".SystemUIFont";
/// LXGW WenKai Mono (霞鹜文楷等宽): the user-installed CJK handwriting
/// face. The family name CoreText publishes once the ttf lands in
/// ~/Library/Fonts. Mono variant: CJK stays 2 cells wide, Latin/ASCII
/// aligns — the right face for a note editor.
pub const WENKAI: &str = "LXGW WenKai Mono";

/// UI font by prefs id: "" (default) → system sans, "wenkai-mono" →
/// LXGW WenKai Mono ("wenkai" = the legacy non-mono id, still honored).
/// Falls back to the system face when the ttf isn't installed yet — gpui
/// resolves unknown families through the platform fallback chain.
pub fn sans_for(font_pref: &str) -> &'static str {
    if font_pref == "wenkai" || font_pref == "wenkai-mono" {
        WENKAI
    } else {
        SANS
    }
}

/// Editor font size by prefs id (UI round 12): "" (regular) → 14px,
/// "medium" → 16px, "large" → 18px. The kit default was 13px; regular
/// nudges it up one step while staying independent of the UI zoom, which
/// scales this value in apply_wake_theme.
pub fn editor_font_px(size_pref: &str) -> f32 {
    match size_pref {
        "medium" => 16.,
        "large" => 18.,
        _ => 14.,
    }
}

/// Preview body text size (px): the editor font pref + UI zoom — note
/// content views (reader/preview) share the editor's size, unlike app
/// chrome which stays at 14px × zoom.
pub fn preview_font_px(size_pref: &str, zoom: f32) -> f32 {
    editor_font_px(size_pref) * zoom
}

/// Zoom-scaled UI length (UI round 12): the kit Root plugin stamps the
/// window's rem from `theme.font_size` (= 14px × zoom), so a value expressed
/// as a fraction of that rem scales with the zoom pref. All app-chrome
/// text/geometry goes through this instead of bare `px()`, which never
/// scaled — that's why the zoom slider only resized the editor.
pub fn ui(n: f32) -> gpui_kit::AbsoluteLength {
    gpui_kit::rems(n / 14.).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_completeness() {
        assert_eq!(MATERIALS.len(), 4);
        // Every layer has a light and dark value, and dark layers darken as
        // they rise (base < list < background < popover) so depth reads.
        for dark in [false, true] {
            for m in MATERIALS {
                let _ = material(m, dark);
            }
        }
        for dark in [false, true] {
            assert!(material(Material::Base, dark).l < material(Material::List, dark).l);
            assert!(material(Material::List, dark).l < material(Material::Background, dark).l);
            assert!(material(Material::Background, dark).l <= material(Material::Popover, dark).l);
        }
        // 8 type sizes, strictly ascending.
        assert_eq!(FONT_SIZES.len(), 8);
        for w in FONT_SIZES.windows(2) {
            assert!(w[0] < w[1], "{FONT_SIZES:?}");
        }
        // Radii 12/8 descending.
        assert_eq!((RADIUS_PANEL, RADIUS_SELECTION), (12., 8.));
        // 4px spacing grid.
        assert_eq!(spacing(0), px(0.));
        assert_eq!(spacing(1), px(4.));
        assert_eq!(spacing(6), px(24.));
        // Fonts.
        assert_eq!(SANS, ".SystemUIFont");
        assert_eq!(sans_for(""), SANS);
        assert_eq!(sans_for("wenkai"), WENKAI);
        assert_eq!(sans_for("wenkai-mono"), WENKAI);
        // UI round 12: editor size steps strictly ascending.
        assert!(editor_font_px("") < editor_font_px("medium"));
        assert!(editor_font_px("medium") < editor_font_px("large"));
        assert_eq!(editor_font_px("bogus"), editor_font_px(""));
        // UI round 12: ui() expresses a px design length as rems of the
        // 14px base rem, so it scales with theme.font_size (14 × zoom).
        assert_eq!(ui(14.), gpui_kit::rems(1.).into());
    }

    fn lum(hex: &str) -> f32 {
        // Relative luminance-lite: mean of max/min channel — enough to
        // order grays and near-grays. Pure table test, no global state.
        let h = hex.strip_prefix('#').unwrap_or(hex);
        let ch: Vec<f32> = (0..3)
            .map(|i| u8::from_str_radix(&h[i * 2..i * 2 + 2], 16).unwrap() as f32 / 255.)
            .collect();
        (ch.iter().cloned().fold(f32::MIN, f32::max) + ch.iter().cloned().fold(f32::MAX, f32::min))
            / 2.
    }

    /// Families (ADR-0010): every family × appearance has all 4 layers as
    /// parseable hex, Base/List translucent, fg distinct from bg, and
    /// layer lightness ordering so depth reads in every family. Pure
    /// table assertions — no CURRENT_FAMILY global, no test-order coupling.
    #[test]
    fn family_table_completeness() {
        use crate::theme_family::FAMILIES;
        assert_eq!(FAMILIES.len(), 6);
        for f in FAMILIES {
            for dark in [false, true] {
                let c = if dark { &f.dark } else { &f.light };
                let base_alpha = u8::from_str_radix(&c.base[7..9], 16).unwrap() as f32 / 255.;
                let list_alpha = u8::from_str_radix(&c.list[7..9], 16).unwrap() as f32 / 255.;
                assert!(base_alpha < 1., "{} base must stay translucent", f.id);
                assert!(list_alpha < 1., "{} list must stay translucent", f.id);
                assert_ne!(
                    lum(c.fg),
                    lum(c.background),
                    "{} fg must differ from bg",
                    f.id
                );
                assert!(
                    lum(c.base) < lum(c.list),
                    "{}: base {} !< list {}",
                    f.id,
                    c.base,
                    c.list
                );
                assert!(
                    lum(c.list) < lum(c.background),
                    "{}: list {} !< bg {}",
                    f.id,
                    c.list,
                    c.background
                );
                assert!(
                    lum(c.background) <= lum(c.popover),
                    "{}: bg {} !<= popover {}",
                    f.id,
                    c.background,
                    c.popover
                );
            }
        }
        // legacy palette ids map onto the same-named families
        assert_eq!(family_index("warm"), 2);
        assert_eq!(family_index("solarized"), 3);
        assert_eq!(family_index("bogus"), 0);
        assert_eq!(family_index(""), 0);
    }
}
