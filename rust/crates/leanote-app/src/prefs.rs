//! App preferences: JSON in the data dir. Covers the issue-17 surfaces:
//! appearance override, language override, editor palette, vim default,
//! recent notes (top 6, most recent first).

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum AppearancePref {
    #[default]
    Follow,
    Light,
    Dark,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum LangPref {
    #[default]
    Follow,
    Zh,
    En,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Prefs {
    pub appearance: AppearancePref,
    pub lang: LangPref,
    pub editor_palette: String, // "" = default (theme built-in)
    // Issue 19: vim keys on by default; old prefs files default to true too
    // (field is `default = ...` so pre-existing files load without it).
    #[serde(default = "default_vim_true")]
    pub vim_default: bool,
    pub recents: Vec<String>,
    /// Issue 18: last user-set left/middle pane widths (px). None = default.
    #[serde(default)]
    pub pane_widths: PaneWidths,
    /// Issue 19 Q13: accent color id, e.g. "blue" (default) … "pink".
    #[serde(default)]
    pub accent: String,
    /// Issue 23 #5: UI zoom factor (1.0/1.1/1.2 → rem size). Older files
    /// stored 1.25/1.5/1.6 — `load` clamps anything above 1.2 down to it.
    #[serde(default)]
    pub zoom: f32,
    /// UI font id: "" = system default, "wenkai" = LXGW WenKai (needs the
    /// ttf installed into ~/Library/Fonts — see fonts.rs).
    #[serde(default)]
    pub font: String,
    /// Editor font size id (UI round 12): "" = regular (14px),
    /// "medium" = 16px, "large" = 18px. Scaled by the UI zoom at apply time.
    #[serde(default)]
    pub editor_font_size: String,
    /// Issue 23 #10: markdown insertion toolbar in the editor (default off).
    #[serde(default)]
    pub editor_toolbar: bool,
    /// Issue 19 Q14: left-nav selection + fold states, restored on launch.
    #[serde(default)]
    pub nav: NavStatePref,
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            appearance: AppearancePref::default(),
            lang: LangPref::default(),
            editor_palette: String::new(),
            vim_default: true,
            recents: Vec::new(),
            pane_widths: PaneWidths::default(),
            accent: String::new(),
            zoom: 1.0,
            font: String::new(),
            editor_font_size: String::new(),
            editor_toolbar: false,
            nav: NavStatePref::default(),
        }
    }
}

/// Issue 19 Q14: persisted nav state. `selection` is a tagged
/// (kind, id) pair — a deleted/renamed notebook or tag simply fails the
/// restore lookup and falls back to Recent.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct NavStatePref {
    #[serde(default)]
    pub selection: Option<(String, String)>,
    #[serde(default = "default_true")]
    pub notebooks_open: bool,
    #[serde(default = "default_true")]
    pub tags_open: bool,
    /// Note the reader last showed (any nav context).
    #[serde(default)]
    pub last_note: Option<String>,
}

fn default_vim_true() -> bool {
    true
}

fn default_true() -> bool {
    true
}

#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PaneWidths {
    pub left: Option<f32>,
    pub mid: Option<f32>,
}

const FILE: &str = "prefs.json";

impl Prefs {
    pub fn load(dir: &std::path::Path) -> Self {
        let mut p: Prefs = std::fs::read_to_string(dir.join(FILE))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        // 1.25/1.5/1.6 collapse to the new top step.
        if p.zoom > 1.21 {
            p.zoom = 1.2;
        }
        p
    }

    pub fn save(&self, dir: &std::path::Path) {
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(dir.join(FILE), json);
        }
    }

    /// Push a note id to the front, dedup, cap at 6.
    pub fn push_recent(&mut self, id: &str) {
        self.recents.retain(|r| r != id);
        self.recents.insert(0, id.to_string());
        self.recents.truncate(6);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recents_dedup_and_cap() {
        let mut p = Prefs::default();
        for i in 0..8 {
            p.push_recent(&format!("n{i}"));
        }
        assert_eq!(p.recents.len(), 6);
        assert_eq!(p.recents[0], "n7");
        p.push_recent("n3"); // existing moves to front, no dup
        assert_eq!(p.recents[0], "n3");
        assert_eq!(p.recents.iter().filter(|r| *r == "n3").count(), 1);
    }

    #[test]
    fn round_trip() {
        let p = Prefs {
            appearance: AppearancePref::Dark,
            lang: LangPref::Zh,
            editor_palette: "solarized".into(),
            vim_default: true,
            recents: vec!["a".into()],
            pane_widths: PaneWidths {
                left: Some(260.),
                mid: Some(380.),
            },
            accent: "purple".into(),
            zoom: 1.2,
            font: "wenkai".into(),
            editor_font_size: "medium".into(),
            editor_toolbar: true,
            nav: NavStatePref {
                selection: Some(("notebook".into(), "n1".into())),
                notebooks_open: false,
                tags_open: true,
                last_note: Some("note9".into()),
            },
        };
        let dir = std::env::temp_dir().join(format!("lnprefs-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        p.save(&dir);
        assert_eq!(Prefs::load(&dir), p);
        // Old prefs files (pre-issue-18/19) lack pane_widths/accent/nav:
        // defaults survive, vim flips to the new default (true).
        std::fs::write(
            dir.join(FILE),
            r#"{"appearance":"Follow","lang":"Follow","editor_palette":"","vim_default":false,"recents":[]}"#,
        )
        .unwrap();
        let old = Prefs::load(&dir);
        assert_eq!(old.pane_widths, PaneWidths::default());
        assert_eq!(old.accent, "");
        assert_eq!(old.nav, NavStatePref::default());
        // UI round 10: legacy zoom steps (1.25/1.5/1.6) collapse to 1.2.
        // The JSON carries every non-defaulted field (editor_palette,
        // recents) — without them deserialization fails and falls to
        // Default, which would mask the clamp.
        std::fs::write(
            dir.join(FILE),
            r#"{"appearance":"Follow","lang":"Follow","editor_palette":"","recents":[],"zoom":1.6}"#,
        )
        .unwrap();
        assert_eq!(Prefs::load(&dir).zoom, 1.2);
        assert_eq!(Prefs::load(&dir).font, "");
        // In-range values survive untouched.
        std::fs::write(
            dir.join(FILE),
            r#"{"appearance":"Follow","lang":"Follow","editor_palette":"","recents":[],"zoom":1.1}"#,
        )
        .unwrap();
        assert_eq!(Prefs::load(&dir).zoom, 1.1);
        assert!(!old.vim_default); // explicit false in file wins over default
        std::fs::remove_dir_all(&dir).ok();
    }
}
