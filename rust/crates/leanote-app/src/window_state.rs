//! Window size/position persistence: JSON in the data dir, clamped to the
//! current displays on load (display gone → default centered).

use gpui_kit::{Bounds, Pixels, Point, Size};

pub const FILE: &str = "window-state.json";
pub const DEFAULT_W: f32 = 1280.;
pub const DEFAULT_H: f32 = 800.;

#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct WindowState {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl WindowState {
    pub fn default_centered() -> Self {
        // 100,100 is only a placeholder origin; callers pass display bounds.
        Self {
            x: 100.,
            y: 100.,
            w: DEFAULT_W,
            h: DEFAULT_H,
        }
    }

    pub fn bounds(&self) -> Bounds<Pixels> {
        Bounds {
            origin: Point {
                x: gpui_kit::px(self.x),
                y: gpui_kit::px(self.y),
            },
            size: Size {
                width: gpui_kit::px(self.w),
                height: gpui_kit::px(self.h),
            },
        }
    }

    pub fn from_bounds(b: &Bounds<Pixels>) -> Self {
        Self {
            x: b.origin.x.as_f32(),
            y: b.origin.y.as_f32(),
            w: b.size.width.as_f32(),
            h: b.size.height.as_f32(),
        }
    }

    /// Clamp to `displays` (screen-coordinate union rects). A state that
    /// intersects no display falls back to the default size — this is the
    /// "monitor unplugged" path. A state merely sticking out is pulled in.
    pub fn clamped(self, displays: &[Bounds<Pixels>]) -> Self {
        let mut s = self;
        // Sanitize degenerate sizes first.
        if !(200.0..=8192.).contains(&s.w) {
            s.w = DEFAULT_W;
        }
        if !(200.0..=8192.).contains(&s.h) {
            s.h = DEFAULT_H;
        }
        let b = s.bounds();
        if let Some(d) = displays.iter().find(|d| d.intersects(&b)) {
            // Pull origin inside the display's visible area; cap size.
            let x = b.origin.x.as_f32().clamp(
                d.origin.x.as_f32(),
                (d.origin.x.as_f32() + d.size.width.as_f32() - s.w).max(d.origin.x.as_f32()),
            );
            let y = b.origin.y.as_f32().clamp(
                d.origin.y.as_f32(),
                (d.origin.y.as_f32() + d.size.height.as_f32() - s.h).max(d.origin.y.as_f32()),
            );
            Self {
                x,
                y,
                w: s.w,
                h: s.h,
            }
        } else {
            Self::default_centered()
        }
    }

    pub fn load(dir: &std::path::Path) -> Option<Self> {
        serde_json::from_str(&std::fs::read_to_string(dir.join(FILE)).ok()?).ok()
    }

    pub fn save(&self, dir: &std::path::Path) {
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(dir.join(FILE), json);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_kit::px;

    fn display(x: f32, y: f32, w: f32, h: f32) -> Bounds<Pixels> {
        Bounds {
            origin: Point { x: px(x), y: px(y) },
            size: Size {
                width: px(w),
                height: px(h),
            },
        }
    }

    #[test]
    fn serde_round_trip() {
        let s = WindowState {
            x: 42.5,
            y: -3.,
            w: 1280.,
            h: 800.,
        };
        let json = serde_json::to_string(&s).unwrap();
        let back: WindowState = serde_json::from_str(&json).unwrap();
        assert_eq!(s, back);
        // Stays stable through a file save/load cycle.
        let dir = std::env::temp_dir().join(format!("lnwstest-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        s.save(&dir);
        assert_eq!(WindowState::load(&dir), Some(s));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn display_gone_falls_back_to_default() {
        let s = WindowState {
            x: 5000.,
            y: 5000.,
            w: 1280.,
            h: 800.,
        };
        let out = s.clamped(&[display(0., 0., 1512., 982.)]);
        assert_eq!(out.w, DEFAULT_W);
        assert_eq!(out.h, DEFAULT_H);
    }

    #[test]
    fn sticking_out_is_pulled_in() {
        let s = WindowState {
            x: 1400.,
            y: 400.,
            w: 1280.,
            h: 800.,
        };
        let out = s.clamped(&[display(0., 0., 1512., 982.)]);
        assert_eq!(out.x, 232.); // 1512 - 1280: pulled in, capped at display edge
        assert_eq!(out.y, 182.); // 982 - 800: also pulled in vertically
        assert_eq!(out.w, 1280.);
    }

    #[test]
    fn degenerate_size_reset() {
        let s = WindowState {
            x: 10.,
            y: 10.,
            w: 0.,
            h: -5.,
        };
        let out = s.clamped(&[display(0., 0., 1512., 982.)]);
        assert_eq!((out.w, out.h), (DEFAULT_W, DEFAULT_H));
    }

    #[test]
    fn intersecting_display_kept() {
        let s = WindowState {
            x: 100.,
            y: 100.,
            w: 1000.,
            h: 700.,
        };
        assert_eq!(s.clamped(&[display(0., 0., 1512., 982.)]), s);
    }
}
