//! Wake visual language (ADR-0003): 4 material layers, 8 type sizes, 4px
//! spacing grid, panel/selection radii, Wake-blue accent, SF Pro + Menlo.
//! Dark/light follow the system; this module only carries the token values.

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
pub const MATERIALS: [Material; 4] = [Material::Base, Material::List, Material::Background, Material::Popover];

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
