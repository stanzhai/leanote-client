//! Wake visual language (ADR-0003): 4 material layers, 8 type sizes, 4px
//! spacing grid, panel/selection radii, Wake-blue accent, SF Pro + Menlo.
//! Dark/light follow the system; this module only carries the token values.

use gpui_kit::component::highlighter::HighlightTheme;
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

/// Wake blue (Q16): used for selection + actions only.
pub fn accent() -> Hsla {
    gpui_kit::rgb(0x3b82f6).into()
}

/// Foreground color that stays legible on `accent()`.
pub fn accent_foreground() -> Hsla {
    hsla(0., 0., 1., 1.)
}

pub fn border(dark: bool) -> Hsla {
    if dark {
        hsla(220. / 360., 0.09, 0.2, 1.)
    } else {
        hsla(220. / 360., 0.13, 0.88, 1.)
    }
}

/// Material layer color for a light or dark theme.
pub fn material(layer: Material, dark: bool) -> Hsla {
    let (h, s) = (222. / 360., 0.);
    let l = match (layer, dark) {
        (Material::Base, false) => 0.955,
        (Material::List, false) => 0.985,
        (Material::Background, false) => 1.0,
        (Material::Popover, false) => 1.0,
        (Material::Base, true) => 0.13,
        (Material::List, true) => 0.165,
        (Material::Background, true) => 0.19,
        (Material::Popover, true) => 0.23,
    };
    hsla(h, s, l, 1.)
}

pub fn foreground(dark: bool) -> Hsla {
    if dark {
        hsla(220. / 360., 0.14, 0.92, 1.)
    } else {
        hsla(222. / 360., 0.12, 0.12, 1.)
    }
}

/// Secondary text (metadata, placeholders).
pub fn muted_foreground(dark: bool) -> Hsla {
    if dark {
        hsla(220. / 360., 0.09, 0.62, 1.)
    } else {
        hsla(220. / 360., 0.08, 0.45, 1.)
    }
}

/// Sans follows the system (".SystemUIFont" = SF Pro on macOS); mono = Menlo.
pub const SANS: &str = ".SystemUIFont";
pub const MONO: &str = "Menlo";

/// Editor palettes (issue 17): 2 non-default options, each with light/dark
/// variants in Zed highlight-theme JSON. `id` matches the prefs value;
/// `""` = keep the kit's default highlight theme.
pub struct PaletteDef {
    pub id: &'static str,
    pub light: fn() -> &'static str,
    pub dark: fn() -> &'static str,
}

pub fn editor_palettes() -> &'static [PaletteDef] {
    &[
        PaletteDef {
            id: "warm",
            light: warm_light,
            dark: warm_dark,
        },
        PaletteDef {
            id: "solarized",
            light: solarized_light,
            dark: solarized_dark,
        },
    ]
}

/// Parse the palette for `id`; unknown or malformed → kit default.
pub fn editor_palette(id: &str, dark: bool) -> std::sync::Arc<HighlightTheme> {
    let def = editor_palettes().iter().find(|p| p.id == id);
    match def {
        Some(d) => {
            serde_json::from_str::<HighlightTheme>(if dark { (d.dark)() } else { (d.light)() })
                .map(std::sync::Arc::new)
                .unwrap_or_else(|_| fallback(dark))
        }
        None => fallback(dark),
    }
}

fn fallback(dark: bool) -> std::sync::Arc<HighlightTheme> {
    if dark {
        HighlightTheme::default_dark()
    } else {
        HighlightTheme::default_light()
    }
}

/// Editor background must match the Background material so the editor pane
/// stays seamless with the rest of the shell.
fn palette_json(
    name: &str,
    appearance: &str,
    bg: &str,
    fg: &str,
    active_line: &str,
    syntax: &str,
) -> String {
    format!(
        r##"{{"name":"{name}","appearance":"{appearance}","style":{{
  "editor.background":"{bg}","editor.foreground":"{fg}",
  "editor.active_line.background":"{active_line}",
  "editor.line_number":"#8a8a8a","editor.active_line_number":"{fg}",
  "editor.invisible":"#8a8a8a66",
  "syntax":{syntax}
}}}}"##
    )
}

fn warm_light() -> &'static str {
    static S: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
        palette_json(
            "Warm Light",
            "light",
            "#faf6f0",
            "#3d332b",
            "#f0e8dc",
            PALETTE_WARM_SYNTAX,
        )
    });
    &S
}
fn warm_dark() -> &'static str {
    static S: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
        palette_json(
            "Warm Dark",
            "dark",
            "#2b2622",
            "#e8dfd5",
            "#35302a",
            PALETTE_WARM_SYNTAX,
        )
    });
    &S
}
fn solarized_light() -> &'static str {
    static S: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
        palette_json(
            "Solarized Light",
            "light",
            "#fdf6e3",
            "#657b83",
            "#eee8d5",
            PALETTE_SOLARIZED_SYNTAX,
        )
    });
    &S
}
fn solarized_dark() -> &'static str {
    static S: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
        palette_json(
            "Solarized Dark",
            "dark",
            "#002b36",
            "#93a1a1",
            "#073642",
            PALETTE_SOLARIZED_SYNTAX,
        )
    });
    &S
}

const PALETTE_WARM_SYNTAX: &str = r##"{
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

const PALETTE_SOLARIZED_SYNTAX: &str = r##"{
  "attribute":{"color":"#268bd2"},"boolean":{"color":"#b58900"},
  "comment":{"color":"#93a1a1"},"comment.doc":{"color":"#93a1a1"},
  "constant":{"color":"#cb4b16"},"constructor":{"color":"#268bd2"},
  "embedded":{"color":"#657b83"},"emphasis":{"font_style":"italic"},
  "emphasis.strong":{"font_weight":700},
  "function":{"color":"#268bd2"},"keyword":{"color":"#859900"},
  "link_text":{"color":"#268bd2"},"link_uri":{"color":"#93a1a1","font_style":"italic"},
  "number":{"color":"#d33682"},"string":{"color":"#2aa198"},
  "string.escape":{"color":"#2aa198"},"string.regex":{"color":"#dc322f"},
  "string.special":{"color":"#dc322f"},"string.special.symbol":{"color":"#dc322f"},
  "tag":{"color":"#268bd2"},"text.literal":{"color":"#586e75"},
  "text.code.span":{"color":"#586e75"},"title":{"color":"#268bd2"},
  "type":{"color":"#b58900"},"property":{"color":"#657b83"},
  "variable":{"color":"#657b83"},"variable.special":{"color":"#cb4b16"}
}"##;

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
        assert_eq!(MONO, "Menlo");
    }
}
