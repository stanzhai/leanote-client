//! UI theme families (Zed-style): 6 families × light/dark, driven from one
//! table. Replaces the old split where `material()`/`foreground()` carried
//! hardcoded Wake values while the editor palettes hand-wrote their own
//! JSON. ADR-0010: `editor_palette` pref folded into `ui_theme`.
//!
//! Design rules:
//! - Base/List stay translucent (window blur shows through); Background/
//!   Popover opaque (readability) — same contract as ADR-0003.
//! - Each family carries its own syntax highlight JSON; the editor JSON is
use gpui_kit::component::highlighter::HighlightTheme;
use gpui_kit::Hsla;

use crate::theme::Material;

/// One appearance (light or dark) of a family: all colors as hex.
#[derive(Clone, Copy)]
pub struct FamilyColors {
    pub base: &'static str,
    pub list: &'static str,
    pub background: &'static str,
    pub popover: &'static str,
    pub fg: &'static str,
    pub muted_fg: &'static str,
    pub border: &'static str,
    /// Editor chrome: bg must equal Background for a seamless pane.
    pub active_line: &'static str,
}

/// A theme family: id (prefs value), i18n key index, both appearances,
/// and the syntax table shared by light/dark (colors differ enough per
/// appearance that each carries its own).
pub struct ThemeFamily {
    pub id: &'static str,
    pub name_zh: &'static str,
    pub name_en: &'static str,
    pub light: FamilyColors,
    pub light_syntax: &'static str,
    pub dark: FamilyColors,
    pub dark_syntax: &'static str,
}

/// The 6 families, in settings-menu order. Index 0 = default (Wake).
pub const FAMILIES: &[ThemeFamily] = &[
    // ── 默认 / Wake (ADR-0003 original) ──────────────────────────────
    ThemeFamily {
        id: "",
        name_zh: "默认",
        name_en: "Default",
        light: FamilyColors {
            base: "#f2f2f2d9",
            list: "#fbfbfbc2",
            background: "#ffffff",
            popover: "#ffffff",
            fg: "#1f1f1f",
            muted_fg: "#5f5f5f",
            border: "#e0e0e0",
            active_line: "#f5f5f5",
        },
        light_syntax: SYNTAX_DEFAULT,
        dark: FamilyColors {
            base: "#2121218c",
            list: "#2a2a2ac7",
            background: "#303030",
            popover: "#3b3b3b",
            fg: "#ebebeb",
            muted_fg: "#b8b8b8",
            border: "#333333",
            active_line: "#3a3a3a",
        },
        dark_syntax: SYNTAX_DEFAULT,
    },
    // ── 豆沙绿 / Moss green ──────────────────────────────────────────
    ThemeFamily {
        id: "moss",
        name_zh: "豆沙绿",
        name_en: "Moss",
        light: FamilyColors {
            // 豆沙绿 #C7EDCC — reserved for the READING surface (editor/
            // reader background), the only saturated green in the family.
            // Sidebar/list are sage-grays at ⅕–⅙ its chroma: enough hue
            // kinship to read as one family, quiet enough for all-day
            // chrome. Luminance still climbs base < list < background so
            // depth ordering survives.
            base: "#b3c2b3d9",
            list: "#bed1bec2",
            background: "#c7edcc",
            popover: "#eaf5e8",
            fg: "#2f3b2e",
            muted_fg: "#4c5a4b",
            border: "#aebfae",
            active_line: "#b5e3ba",
        },
        light_syntax: SYNTAX_MOSS,
        dark: FamilyColors {
            base: "#2224208c",
            list: "#2a2d26c7",
            background: "#30332c",
            popover: "#3a3e35",
            fg: "#e4e6dc",
            muted_fg: "#bfc4b4",
            border: "#3c4037",
            active_line: "#393d34",
        },
        dark_syntax: SYNTAX_MOSS,
    },
    // ── 暖色 / Warm ──────────────────────────────────────────────────
    ThemeFamily {
        id: "warm",
        name_zh: "暖色",
        name_en: "Warm",
        light: FamilyColors {
            base: "#f0e9dfd9",
            list: "#faf5ecc2",
            background: "#faf6f0",
            popover: "#faf6f0",
            fg: "#3d332b",
            muted_fg: "#6f6459",
            border: "#e6dccd",
            active_line: "#f0e8dc",
        },
        light_syntax: SYNTAX_WARM,
        dark: FamilyColors {
            base: "#241f1a8c",
            list: "#282420c7",
            background: "#2b2622",
            popover: "#35302a",
            fg: "#e8dfd5",
            muted_fg: "#a39688",
            border: "#3a332c",
            active_line: "#35302a",
        },
        dark_syntax: SYNTAX_WARM,
    },
    // ── Solarized ────────────────────────────────────────────────────
    ThemeFamily {
        id: "solarized",
        name_zh: "Solarized",
        name_en: "Solarized",
        light: FamilyColors {
            base: "#efe8d5d9",
            list: "#f8f2e2c2",
            background: "#fdf6e3",
            popover: "#fdf6e3",
            fg: "#657b83",
            muted_fg: "#5b6b6b",
            border: "#e3ddc8",
            active_line: "#eee8d5",
        },
        light_syntax: SYNTAX_SOLARIZED,
        dark: FamilyColors {
            base: "#00222b8c",
            list: "#012630c7",
            background: "#002b36",
            popover: "#073642",
            fg: "#5b6b6b",
            muted_fg: "#6c7c7c",
            border: "#0d3f4a",
            active_line: "#073642",
        },
        dark_syntax: SYNTAX_SOLARIZED,
    },
    // ── Nord ─────────────────────────────────────────────────────────
    ThemeFamily {
        id: "nord",
        name_zh: "Nord",
        name_en: "Nord",
        light: FamilyColors {
            base: "#eceff4d9",
            list: "#f2f5f9c2",
            background: "#fbfcfe",
            popover: "#fbfcfe",
            fg: "#2e3440",
            muted_fg: "#5c6883",
            border: "#d8dee9",
            active_line: "#eceff4",
        },
        light_syntax: SYNTAX_NORD,
        dark: FamilyColors {
            base: "#2429338c",
            list: "#2a3040c7",
            background: "#2e3440",
            popover: "#3b4252",
            fg: "#e5e9f0",
            muted_fg: "#a7b1c2",
            border: "#434c5e",
            active_line: "#3b4252",
        },
        dark_syntax: SYNTAX_NORD,
    },
    // ── 墨 / Ink ─────────────────────────────────────────────────────
    ThemeFamily {
        id: "ink",
        name_zh: "墨",
        name_en: "Ink",
        light: FamilyColors {
            base: "#f4f4f2d9",
            list: "#fafaf8c2",
            background: "#fefefc",
            popover: "#fefefc",
            fg: "#1a1a18",
            muted_fg: "#5b5b55",
            border: "#e4e4df",
            active_line: "#f4f4f0",
        },
        light_syntax: SYNTAX_INK,
        dark: FamilyColors {
            base: "#1212128c",
            list: "#1a1a1ac7",
            background: "#1e1e1e",
            popover: "#262626",
            fg: "#d4d4cf",
            muted_fg: "#8a8a84",
            border: "#2e2e2a",
            active_line: "#282826",
        },
        dark_syntax: SYNTAX_INK,
    },
];

/// Active family id ("" = default). Set from prefs at boot and on change.
static CURRENT_FAMILY: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Map a prefs id to a family index; unknown → 0 (default). Legacy
/// `editor_palette` values ("warm"/"solarized") resolve to the same family.
pub fn family_index(id: &str) -> usize {
    FAMILIES.iter().position(|f| f.id == id).unwrap_or(0)
}

pub fn set_family(id: &str) {
    CURRENT_FAMILY.store(family_index(id), std::sync::atomic::Ordering::Relaxed);
}

fn current() -> &'static ThemeFamily {
    // FAMILIES is non-empty; index always in range.
    &FAMILIES[CURRENT_FAMILY.load(std::sync::atomic::Ordering::Relaxed)]
}

fn colors(dark: bool) -> &'static FamilyColors {
    let f = current();
    if dark {
        &f.dark
    } else {
        &f.light
    }
}

fn parse(hex: &str) -> Hsla {
    let raw = u32::from_str_radix(hex.strip_prefix('#').unwrap_or(hex), 16).unwrap_or(0);
    // 8 digits carry alpha (translucent Base/List); 6 are opaque. rgb()
    // drops the high byte, so 8-digit values must go through rgba().
    if hex.trim_start_matches('#').len() == 8 {
        gpui_kit::rgba(raw).into()
    } else {
        gpui_kit::rgb(raw).into()
    }
}

// ---------------------------------------------------------------------------
// Token accessors — same signatures as the old hardcoded ones.
// ---------------------------------------------------------------------------

pub fn material(layer: Material, dark: bool) -> Hsla {
    let c = colors(dark);
    match layer {
        Material::Base => parse(c.base),
        Material::List => parse(c.list),
        Material::Background => parse(c.background),
        Material::Popover => parse(c.popover),
    }
}

pub fn foreground(dark: bool) -> Hsla {
    parse(colors(dark).fg)
}

pub fn muted_foreground(dark: bool) -> Hsla {
    parse(colors(dark).muted_fg)
}

/// Round 5: user glass strength pref → nav Base alpha (light, dark).
/// "off" is fully opaque — glass disappears. Family hexes carry `d9`/`8c`
/// (= medium); the other steps scale from the family's own hue.
pub fn glass_alphas(id: &str) -> (f32, f32) {
    match id {
        "off" => (1.0, 1.0),
        "weak" => (0.88, 0.62),
        "strong" => (0.62, 0.30),
        _ => (0.75, 0.46), // medium
    }
}

/// Nav Base color at the user's glass strength: the family hue with the
/// pref's alpha swapped in.
pub fn glass_base(dark: bool, glass_id: &str) -> Hsla {
    let (light_a, dark_a) = glass_alphas(glass_id);
    let mut c = parse(colors(dark).base);
    c.a = if dark { dark_a } else { light_a };
    c
}

pub fn border(dark: bool) -> Hsla {
    parse(colors(dark).border)
}

// ---------------------------------------------------------------------------
// Editor highlight theme generated from the family (single source).
// ---------------------------------------------------------------------------

fn highlight_json(f: &ThemeFamily, dark: bool) -> String {
    let (c, syntax, appearance, name) = if dark {
        (
            &f.dark,
            f.dark_syntax,
            "dark",
            format!("{} Dark", f.name_en),
        )
    } else {
        (
            &f.light,
            f.light_syntax,
            "light",
            format!("{} Light", f.name_en),
        )
    };
    format!(
        r##"{{"name":"{name}","appearance":"{appearance}","style":{{
  "editor.background":"{}","editor.foreground":"{}",
  "editor.active_line.background":"{}",
  "editor.line_number":"#8a8a8a","editor.active_line_number":"{}",
  "editor.invisible":"#8a8a8a66",
  "syntax":{syntax}
}}}}"##,
        c.background, c.fg, c.active_line, c.fg
    )
}

/// Parsed highlight theme for the current family; "" (default) keeps the
/// kit's built-in default (identical Wake values, zero parse cost).
/// Cached per (family index, dark) — theme switches replay this.
pub fn family_highlight(id: &str, dark: bool) -> std::sync::Arc<HighlightTheme> {
    use std::collections::HashMap;
    use std::sync::{Arc, LazyLock, Mutex};
    static CACHE: LazyLock<Mutex<HashMap<(usize, bool), Arc<HighlightTheme>>>> =
        LazyLock::new(|| Mutex::new(HashMap::new()));
    let ix = family_index(id);
    let key = (ix, dark);
    if let Some(hit) = CACHE.lock().unwrap().get(&key) {
        return hit.clone();
    }
    let parsed: std::sync::Arc<HighlightTheme> = if ix == 0 {
        // Default family: kit built-ins are the same values — reuse them.
        if dark {
            HighlightTheme::default_dark()
        } else {
            HighlightTheme::default_light()
        }
    } else {
        let f = &FAMILIES[ix];
        serde_json::from_str::<HighlightTheme>(&highlight_json(f, dark))
            .map(std::sync::Arc::new)
            .unwrap_or_else(|_| fallback(dark))
    };
    CACHE.lock().unwrap().insert(key, parsed.clone());
    parsed
}

fn fallback(dark: bool) -> std::sync::Arc<HighlightTheme> {
    if dark {
        HighlightTheme::default_dark()
    } else {
        HighlightTheme::default_light()
    }
}

// ---------------------------------------------------------------------------
// Syntax tables (keyword/function/string/comment color sets per family).
// ---------------------------------------------------------------------------

const SYNTAX_DEFAULT: &str = r##"{}"##; // kit default handles it

const SYNTAX_MOSS: &str = r##"{
  "attribute":{"color":"#4e7a3a"},"boolean":{"color":"#8a3b3b"},
  "comment":{"color":"#4c5a4b"},"comment.doc":{"color":"#4c5a4b"},
  "constant":{"color":"#8a3b3b"},"constructor":{"color":"#2d5f8f"},
  "embedded":{"color":"#2f3b2e"},"emphasis":{"font_style":"italic"},
  "emphasis.strong":{"font_weight":700},
  "function":{"color":"#2d5f8f"},"keyword":{"color":"#8a3b3b"},
  "link_text":{"color":"#2d5f8f"},"link_uri":{"color":"#52685a","font_style":"italic"},
  "number":{"color":"#7a5a1e"},"string":{"color":"#3d6b4f"},
  "string.escape":{"color":"#3d6b4f"},"string.regex":{"color":"#7a3a5e"},
  "string.special":{"color":"#7a3a5e"},"string.special.symbol":{"color":"#7a3a5e"},
  "tag":{"color":"#8a3b3b"},"text.literal":{"color":"#3f4a3e"},
  "text.code.span":{"color":"#3f4a3e"},"title":{"color":"#2d5f8f"},
  "type":{"color":"#5a4a8a"},"property":{"color":"#2f3b2e"},
  "variable":{"color":"#2f3b2e"},"variable.special":{"color":"#8a3b3b"}
}"##;

const SYNTAX_WARM: &str = r##"{
  "attribute":{"color":"#8a6d3b"},"boolean":{"color":"#a05252"},
  "comment":{"color":"#7f8c8d"},"comment.doc":{"color":"#7f8c8d"},
  "constant":{"color":"#a05252"},"constructor":{"color":"#2f5f8f"},
  "embedded":{"color":"#3d332b"},"emphasis":{"font_style":"italic"},
  "emphasis.strong":{"font_weight":700},
  "function":{"color":"#2f5f8f"},"keyword":{"color":"#a05252"},
  "link_text":{"color":"#2f5f8f"},"link_uri":{"color":"#708090","font_style":"italic"},
  "number":{"color":"#8a6d3b"},"string":{"color":"#5f7a4f"},
  "string.escape":{"color":"#5f7a4f"},"string.regex":{"color":"#5f7a4f"},
  "string.special":{"color":"#b0563a"},"string.special.symbol":{"color":"#b0563a"},
  "tag":{"color":"#a05252"},"text.literal":{"color":"#6b5344"},
  "text.code.span":{"color":"#6b5344"},"title":{"color":"#2f5f8f"},
  "type":{"color":"#6b5344"},"property":{"color":"#3d332b"},
  "variable":{"color":"#3d332b"},"variable.special":{"color":"#a05252"}
}"##;

const SYNTAX_SOLARIZED: &str = r##"{
  "attribute":{"color":"#268bd2"},"boolean":{"color":"#b58900"},
  "comment":{"color":"#5b6b6b"},"comment.doc":{"color":"#5b6b6b"},
  "constant":{"color":"#cb4b16"},"constructor":{"color":"#268bd2"},
  "embedded":{"color":"#657b83"},"emphasis":{"font_style":"italic"},
  "emphasis.strong":{"font_weight":700},
  "function":{"color":"#268bd2"},"keyword":{"color":"#859900"},
  "link_text":{"color":"#268bd2"},"link_uri":{"color":"#5b6b6b","font_style":"italic"},
  "number":{"color":"#d33682"},"string":{"color":"#2aa198"},
  "string.escape":{"color":"#2aa198"},"string.regex":{"color":"#dc322f"},
  "string.special":{"color":"#dc322f"},"string.special.symbol":{"color":"#dc322f"},
  "tag":{"color":"#268bd2"},"text.literal":{"color":"#586e75"},
  "text.code.span":{"color":"#586e75"},"title":{"color":"#268bd2"},
  "type":{"color":"#b58900"},"property":{"color":"#657b83"},
  "variable":{"color":"#657b83"},"variable.special":{"color":"#cb4b16"}
}"##;

const SYNTAX_NORD: &str = r##"{
  "attribute":{"color":"#8fbcbb"},"boolean":{"color":"#b48ead"},
  "comment":{"color":"#7b88a1"},"comment.doc":{"color":"#7b88a1"},
  "constant":{"color":"#d08770"},"constructor":{"color":"#88c0d0"},
  "embedded":{"color":"#2e3440"},"emphasis":{"font_style":"italic"},
  "emphasis.strong":{"font_weight":700},
  "function":{"color":"#88c0d0"},"keyword":{"color":"#81a1c1"},
  "link_text":{"color":"#88c0d0"},"link_uri":{"color":"#7b88a1","font_style":"italic"},
  "number":{"color":"#b48ead"},"string":{"color":"#a3be8c"},
  "string.escape":{"color":"#ebcb8b"},"string.regex":{"color":"#ebcb8b"},
  "string.special":{"color":"#ebcb8b"},"string.special.symbol":{"color":"#ebcb8b"},
  "tag":{"color":"#81a1c1"},"text.literal":{"color":"#616e88"},
  "text.code.span":{"color":"#616e88"},"title":{"color":"#88c0d0"},
  "type":{"color":"#8fbcbb"},"property":{"color":"#2e3440"},
  "variable":{"color":"#2e3440"},"variable.special":{"color":"#d08770"}
}"##;

const SYNTAX_INK: &str = r##"{
  "attribute":{"color":"#8a8a7a"},"boolean":{"color":"#9a6a6a"},
  "comment":{"color":"#8a8a84"},"comment.doc":{"color":"#8a8a84"},
  "constant":{"color":"#9a6a6a"},"constructor":{"color":"#5a5a4a"},
  "embedded":{"color":"#1a1a18"},"emphasis":{"font_style":"italic"},
  "emphasis.strong":{"font_weight":700},
  "function":{"color":"#5a5a4a"},"keyword":{"color":"#9a6a6a"},
  "link_text":{"color":"#5a5a4a"},"link_uri":{"color":"#8a8a84","font_style":"italic"},
  "number":{"color":"#8a8a7a"},"string":{"color":"#6a7a5a"},
  "string.escape":{"color":"#6a7a5a"},"string.regex":{"color":"#6a7a5a"},
  "string.special":{"color":"#a08a6a"},"string.special.symbol":{"color":"#a08a6a"},
  "tag":{"color":"#9a6a6a"},"text.literal":{"color":"#4a4a40"},
  "text.code.span":{"color":"#4a4a40"},"title":{"color":"#5a5a4a"},
  "type":{"color":"#4a4a40"},"property":{"color":"#1a1a18"},
  "variable":{"color":"#1a1a18"},"variable.special":{"color":"#9a6a6a"}
}"##;
