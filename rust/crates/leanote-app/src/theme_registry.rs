//! Navop theme system (ADR-0014): the 23 bundled Navop theme files
//! (feigeCode/navop `themes/*.json`, one `ThemeSet` per file) drive the
//! whole appearance. The kit's `ThemeRegistry` owns the same JSONs for
//! its widgets — colors flow through `ThemeConfig::apply_config`, the
//! per-theme `highlight` section becomes the editor syntax theme — while
//! this module mirrors the surface values the app's own panes paint via
//! `theme::material()` (same keys, same fallbacks as the kit:
//! sidebar ← background.blend(border@0.15), list/popover ← background).

use gpui_kit::component::theme::try_parse_color;
use gpui_kit::Hsla;
use std::sync::LazyLock;

use crate::theme::Material;

/// The bundled Navop theme files, `themes/navop.json` first so the
/// defaults ("Navop Light"/"Navop Dark") lead every picker.
const BUNDLED: &[&str] = &[
    include_str!("../themes/navop.json"),
    include_str!("../themes/adventure.json"),
    include_str!("../themes/alduin.json"),
    include_str!("../themes/asciinema.json"),
    include_str!("../themes/aurora.json"),
    include_str!("../themes/ayu.json"),
    include_str!("../themes/catppuccin.json"),
    include_str!("../themes/everforest.json"),
    include_str!("../themes/fahrenheit.json"),
    include_str!("../themes/flexoki.json"),
    include_str!("../themes/gruvbox.json"),
    include_str!("../themes/harper.json"),
    include_str!("../themes/hybrid.json"),
    include_str!("../themes/jellybeans.json"),
    include_str!("../themes/kibble.json"),
    include_str!("../themes/macos-classic.json"),
    include_str!("../themes/matrix.json"),
    include_str!("../themes/mellifluous.json"),
    include_str!("../themes/molokai.json"),
    include_str!("../themes/solarized.json"),
    include_str!("../themes/spaceduck.json"),
    include_str!("../themes/tokyonight.json"),
    include_str!("../themes/twilight.json"),
];

pub const DEFAULT_LIGHT: &str = "Navop Light";
pub const DEFAULT_DARK: &str = "Navop Dark";

/// One theme entry: the surface colors this app paints itself. Kit-only
/// keys (tabs, tables, danger…) stay in the JSON for the registry.
struct Entry {
    name: String,
    dark: bool,
    background: Hsla,
    sidebar: Hsla,
    list: Hsla,
    popover: Hsla,
    fg: Hsla,
    muted_fg: Hsla,
    border: Hsla,
    primary: Hsla,
    primary_fg: Hsla,
    /// The theme's own radius keys, when present (Matrix: 0/0). None =
    /// theme doesn't style radii; the app default applies.
    radius: Option<f32>,
    radius_lg: Option<f32>,
}

fn parse_entry(theme: &serde_json::Value) -> Option<Entry> {
    let name = theme.get("name")?.as_str()?.to_string();
    let dark = theme.get("mode")?.as_str()? == "dark";
    let colors = theme.get("colors")?;

    let get = |key: &str| {
        colors
            .get(key)
            .and_then(|v| v.as_str())
            .and_then(|s| try_parse_color(s).ok())
    };
    let background = get("background")?;
    let fg = get("foreground")?;
    let border = get("border").unwrap_or(background);
    let muted_fg = get("muted.foreground").unwrap_or(fg);
    // Kit fallbacks (schema.rs): sidebar ← background.blend(border@0.15);
    // list/popover ← background.
    let sidebar =
        get("sidebar.background").unwrap_or_else(|| background.blend(border.opacity(0.15)));
    let list = get("list.background").unwrap_or(background);
    let popover = get("popover.background").unwrap_or(background);
    // Every Navop theme defines primary; fall back to the app blue for
    // hand-edited imports.
    let primary = get("primary.background").unwrap_or(gpui_kit::rgb(0x3b82f6).into());
    let primary_fg = get("primary.foreground").unwrap_or(gpui_kit::white());
    let radius = theme
        .get("radius")
        .and_then(|v| v.as_f64())
        .map(|v| v as f32);
    let radius_lg = theme
        .get("radius.lg")
        .and_then(|v| v.as_f64())
        .map(|v| v as f32);

    Some(Entry {
        name,
        dark,
        background,
        sidebar,
        list,
        popover,
        fg,
        muted_fg,
        border,
        primary,
        primary_fg,
        radius,
        radius_lg,
    })
}

static ENTRIES: LazyLock<Vec<Entry>> = LazyLock::new(|| {
    let mut v = Vec::new();
    for source in BUNDLED {
        if let Ok(set) = serde_json::from_str::<serde_json::Value>(source) {
            if let Some(themes) = set.get("themes").and_then(|t| t.as_array()) {
                v.extend(themes.iter().filter_map(parse_entry));
            }
        }
    }
    v
});

static CURRENT_LIGHT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
static CURRENT_DARK: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(1);
/// Resolved appearance for mode-less accessors (`accent()` follows the
/// active theme's primary). Set on every `apply_wake_theme` replay.
pub static MODE_DARK: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Resolve one picker name to the entry for a mode: exact match first
/// ("Matrix"), then the "{name} Light"/"{name} Dark" sibling ("Gruvbox" →
/// "Gruvbox Light"/"Gruvbox Dark"), else the Navop default for that mode.
fn index_for(name: &str, dark: bool) -> usize {
    let default_name = if dark { DEFAULT_DARK } else { DEFAULT_LIGHT };
    let sibling = if dark {
        format!("{name} Dark")
    } else {
        format!("{name} Light")
    };
    ENTRIES
        .iter()
        .position(|e| e.name == name && e.dark == dark)
        .or_else(|| {
            ENTRIES
                .iter()
                .position(|e| e.name == sibling && e.dark == dark)
        })
        .unwrap_or_else(|| {
            ENTRIES
                .iter()
                .position(|e| e.name == default_name)
                .unwrap_or(0)
        })
}

static CURRENT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Picker choices: paired "X Light"/"X Dark" themes collapse to one "X"
/// row; single-mode themes (Matrix, Catppuccin Latte, …) list as-is.
/// Cached — `entry()` reads it on every render-path accessor call.
static CHOICES: LazyLock<Vec<&'static str>> = LazyLock::new(|| {
    let mut out: Vec<&'static str> = Vec::new();
    for e in ENTRIES.iter() {
        let name = e.name.as_str();
        let (stem, sibling) = match name.strip_suffix(" Light") {
            Some(stem) => (stem, format!("{stem} Dark")),
            None => match name.strip_suffix(" Dark") {
                Some(stem) => (stem, format!("{stem} Light")),
                None => (name, String::new()),
            },
        };
        let paired = !sibling.is_empty() && ENTRIES.iter().any(|o| o.name == sibling);
        // Light side of a pair lists the stem; the Dark side is the same
        // choice. Singles list verbatim.
        if paired && e.dark {
            continue;
        }
        // Display label: the bundled default pair shows as "Default".
        let label = if paired {
            if stem == "Navop" {
                "Default"
            } else {
                stem
            }
        } else {
            name
        };
        if !out.contains(&label) {
            out.push(label);
        }
    }
    out
});

/// The picker list, file order (Navop first).
pub fn choices() -> &'static [&'static str] {
    &CHOICES
}

/// Register every bundled theme with the kit's `ThemeRegistry` so kit
/// widgets paint the same JSON. Call once at boot.
pub fn register(cx: &mut gpui_kit::App) {
    use gpui_kit::component::ThemeRegistry;
    for source in BUNDLED {
        let _ = ThemeRegistry::global_mut(cx).load_themes_from_str(source);
    }
}

/// Pick the theme by chooser name (one choice drives both appearances:
/// the Light variant serves light mode, the Dark variant dark mode).
/// Unknown names fall back to the first choice (Navop).
pub fn select(name: &str) {
    let ix = choices().iter().position(|c| *c == name).unwrap_or(0);
    CURRENT.store(ix, std::sync::atomic::Ordering::Relaxed);
}

/// The entry name actually serving `dark` for the current choice (used
/// to fetch the kit config at boot / on theme switch).
pub fn resolved_name(dark: bool) -> &'static str {
    entry(dark).name.as_str()
}

fn entry(dark: bool) -> &'static Entry {
    let choice = choices()[CURRENT.load(std::sync::atomic::Ordering::Relaxed)];
    &ENTRIES[index_for(choice, dark)]
}

fn mode_dark() -> bool {
    MODE_DARK.load(std::sync::atomic::Ordering::Relaxed)
}

// ---------------------------------------------------------------------------
// Token accessors — same signatures the old family table carried.
// ---------------------------------------------------------------------------

pub fn material(layer: Material, dark: bool) -> Hsla {
    let e = entry(dark);
    match layer {
        Material::Base => e.sidebar,
        Material::List => e.list,
        Material::Background => e.background,
        Material::Popover => e.popover,
    }
}

pub fn foreground(dark: bool) -> Hsla {
    entry(dark).fg
}

pub fn muted_foreground(dark: bool) -> Hsla {
    entry(dark).muted_fg
}

pub fn border(dark: bool) -> Hsla {
    entry(dark).border
}

/// The active theme's radius keys (None = theme doesn't style radii;
/// callers apply their own defaults). ADR-0014: radii follow the theme —
/// Matrix squares everything off at 0.
pub fn theme_radii(dark: bool) -> (Option<f32>, Option<f32>) {
    let e = entry(dark);
    (e.radius, e.radius_lg)
}

/// User glass strength pref → nav Base alpha (light, dark). "off" is
/// fully opaque — glass disappears.
pub fn glass_alphas(id: &str) -> (f32, f32) {
    match id {
        "off" => (1.0, 1.0),
        "weak" => (0.88, 0.62),
        "strong" => (0.62, 0.30),
        _ => (0.75, 0.46), // medium
    }
}

/// Nav Base color at the user's glass strength: the theme's sidebar with
/// the pref's alpha swapped in.
pub fn glass_base(dark: bool, glass_id: &str) -> Hsla {
    let (light_a, dark_a) = glass_alphas(glass_id);
    let mut c = material(Material::Base, dark);
    c.a = if dark { dark_a } else { light_a };
    c
}

/// The active theme's primary — the accent when the user hasn't picked a
/// swatch (Navop's default behavior: accent follows the theme).
pub fn theme_primary() -> Hsla {
    entry(mode_dark()).primary
}

pub fn theme_primary_foreground() -> Hsla {
    entry(mode_dark()).primary_fg
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every bundled theme parses with the surfaces this app needs, the
    /// Navop defaults lead their pickers, fg contrasts bg, and both
    /// appearances are covered.
    #[test]
    fn bundled_table_completeness() {
        assert_eq!(ENTRIES.len(), 39, "23 bundled Navop theme files");
        assert_eq!(ENTRIES[0].name, DEFAULT_LIGHT);
        assert!(!ENTRIES[0].dark);
        assert_eq!(ENTRIES[1].name, DEFAULT_DARK);
        assert!(ENTRIES[1].dark);
        for e in ENTRIES.iter() {
            assert_ne!(e.fg.l, e.background.l, "{}: fg equals bg", e.name);
            assert_eq!(e.primary.a, 1.0, "{}: primary must be opaque", e.name);
        }
        // Picker: pairs collapse to the stem, singles list verbatim, and
        // one choice serves both appearances.
        assert_eq!(choices().first(), Some(&"Default"));
        assert!(choices().contains(&"Gruvbox"));
        assert!(!choices().contains(&"Gruvbox Light"));
        assert!(choices().contains(&"Matrix"), "dark-only theme lists as-is");
        assert!(choices().len() < ENTRIES.len());
        select("Gruvbox");
        assert_eq!(resolved_name(false), "Gruvbox Light");
        assert_eq!(resolved_name(true), "Gruvbox Dark");
        select("Matrix"); // dark-only: dark serves, light falls back
        assert_eq!(resolved_name(true), "Matrix");
        assert_eq!(resolved_name(false), DEFAULT_LIGHT);
        select("Tokyo Night");
        assert_eq!(resolved_name(true), "Tokyo Night");
        select("bogus"); // unknown → Navop pair
        assert_eq!(resolved_name(false), DEFAULT_LIGHT);
        assert_eq!(resolved_name(true), DEFAULT_DARK);
        select("Default"); // restore global default for other tests
    }

    /// The glass pref only touches the alpha channel of the sidebar.
    #[test]
    fn glass_swaps_alpha_only() {
        let medium = glass_base(false, "medium");
        let off = glass_base(false, "off");
        assert_eq!(medium.a, 0.75);
        assert_eq!(off.a, 1.0);
        assert_eq!(medium.h, off.h);
    }
}
