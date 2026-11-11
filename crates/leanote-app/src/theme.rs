//! Visual language (ADR-0003 system, ADR-0014 Navop theme registry): 4
//! material layers, 8 type sizes, 4px spacing grid, panel/selection
//! radii, theme-following accent, SF Pro + Menlo. Dark/light follow the
//! system; this module only carries the token values.

pub use crate::theme_registry::{
    border, choices_for, foreground, glass_base, material, muted_foreground, register,
    resolved_name, select, MODE_DARK,
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
/// Motion (issue 41): the one timing for app transitions — nav/note-list
/// hover + selection fades and the reader's note-switch fade. 120ms
/// ease-out: fast enough to never read as lag, present enough to register.
/// Honors the system Reduce Motion preference (kit init reads it; every
/// kit animation snaps to its end state under it).
pub const MOTION_MS: u64 = 120;

/// The shared transition policy for every hover/selection fade.
pub fn motion() -> gpui_kit::base::motion::Transition {
    gpui_kit::base::motion::Transition::new(std::time::Duration::from_millis(MOTION_MS))
        .ease(gpui_kit::base::animation::ease_out_cubic)
}

/// User-selectable accent overrides, matching the macOS accent swatches
/// (System Settings → Appearance): blue/purple/pink/red/orange/yellow/
/// green, plus pure black. The DEFAULT ("" / no pick) follows the active
/// Navop theme's primary — Navop's own model, where the custom accent is
/// an optional override (ADR-0014).
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

/// u32::MAX = follow the theme's primary (the default); otherwise an ACCENTS hex
/// (0 is NOT the sentinel — the black swatch is 0x000000).
static CURRENT_ACCENT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(u32::MAX);

/// Set the accent by swatch id. "" / unknown ids clear the override and
/// follow the theme. Callers re-apply the theme afterwards so derived
/// colors update.
pub fn set_accent(id: &str) {
    let hex = ACCENTS
        .iter()
        .find(|(k, _)| *k == id)
        .map(|(_, v)| *v)
        .unwrap_or(u32::MAX);
    CURRENT_ACCENT.store(hex, std::sync::atomic::Ordering::Relaxed);
}

/// True when the user picked a swatch (the theme override is active).
pub fn accent_is_custom() -> bool {
    CURRENT_ACCENT.load(std::sync::atomic::Ordering::Relaxed) != u32::MAX
}

/// The active accent — the theme's primary unless the user picked a
/// swatch. Selection + actions only.
pub fn accent() -> Hsla {
    if accent_is_custom() {
        gpui_kit::rgb(CURRENT_ACCENT.load(std::sync::atomic::Ordering::Relaxed)).into()
    } else {
        crate::theme_registry::theme_primary()
    }
}

/// Foreground that stays legible on `accent()`: the theme's own choice
/// when following, else bright custom accents (yellow/green/orange/
/// graphite) get black and the rest white.
pub fn accent_foreground() -> Hsla {
    if !accent_is_custom() {
        crate::theme_registry::theme_primary_foreground()
    } else if accent_luminance() > 0.55 {
        hsla(0., 0., 0., 1.)
    } else {
        hsla(0., 0., 1., 1.)
    }
}

fn accent_luminance() -> f32 {
    let hex = CURRENT_ACCENT.load(std::sync::atomic::Ordering::Relaxed);
    let [r, g, b] = [(hex >> 16) & 0xff, (hex >> 8) & 0xff, hex & 0xff];
    (0.299 * r as f32 + 0.587 * g as f32 + 0.114 * b as f32) / 255.
}

/// Sans follows the system (".SystemUIFont" = SF Pro on macOS).
/// UI round 11 #2: no separate mono face — the editor binds to the UI
/// font (sans_for) so the font pref applies everywhere at once.
pub const SANS: &str = ".SystemUIFont";
/// LXGW WenKai Mono (霞鹜文楷等宽): the user-chosen CJK handwriting face,
/// cached in the data dir and registered in-process by fonts.rs (all
/// platforms). Mono variant: CJK stays 2 cells wide, Latin/ASCII aligns —
/// the right face for a note editor.
pub const WENKAI: &str = "LXGW WenKai Mono";

/// UI font by prefs id: "" (default) → system sans, "wenkai-mono" →
/// LXGW WenKai Mono ("wenkai" = the legacy non-mono id, still honored).
/// Falls back to the system face when the ttf hasn't been registered —
/// gpui resolves unknown families through the platform fallback chain.
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

/// The accent/mode globals (CURRENT_ACCENT, MODE_DARK) are process-wide;
/// every test that touches them must hold this lock — cargo runs tests
/// on parallel threads (the race surfaced with the theme-switch test).
#[cfg(test)]
pub(crate) static STATE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_completeness() {
        assert_eq!(MATERIALS.len(), 4);
        // Every layer resolves for both appearances (registry tests cover
        // the per-theme table itself).
        for dark in [false, true] {
            for m in MATERIALS {
                let _ = material(m, dark);
            }
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

    #[test]
    fn accent_foreground_contrast() {
        let _state = STATE_LOCK.lock().unwrap();
        // Bright custom accents get black text, the rest (incl. pure
        // black) white. Restores the no-pick default so the global state
        // holds for later tests.
        for (id, expect_black) in [
            ("blue", false),
            ("purple", false),
            ("pink", false),
            ("red", false),
            ("orange", true),
            ("yellow", true),
            ("green", true),
            ("black", false),
        ] {
            set_accent(id);
            assert_eq!(
                accent_foreground().l,
                if expect_black { 0. } else { 1. },
                "{id}"
            );
        }
        set_accent("");
    }

    /// ADR-0014: no pick (""/unknown) follows the active theme's primary
    /// — Navop Light's blue-500 with the default selection — and clears
    /// the custom flag.
    #[test]
    fn accent_follows_theme_by_default() {
        let _state = STATE_LOCK.lock().unwrap();
        set_accent("");
        assert!(!accent_is_custom());
        // Default selection: Navop Light in light mode.
        MODE_DARK.store(false, std::sync::atomic::Ordering::Relaxed);
        let a = accent();
        let p = crate::theme_registry::theme_primary();
        assert_eq!((a.h, a.s, a.l, a.a), (p.h, p.s, p.l, p.a));
        set_accent("bogus-id");
        assert!(!accent_is_custom(), "unknown ids clear the override");
        set_accent("");
    }
}
