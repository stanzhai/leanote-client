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

#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Prefs {
    pub appearance: AppearancePref,
    pub lang: LangPref,
    pub editor_palette: String, // "" = default (theme built-in)
    pub vim_default: bool,
    pub recents: Vec<String>,
    /// Issue 18: last user-set left/middle pane widths (px). None = default.
    #[serde(default)]
    pub pane_widths: PaneWidths,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PaneWidths {
    pub left: Option<f32>,
    pub mid: Option<f32>,
}

const FILE: &str = "prefs.json";

impl Prefs {
    pub fn load(dir: &std::path::Path) -> Self {
        std::fs::read_to_string(dir.join(FILE))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
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
        };
        let dir = std::env::temp_dir().join(format!("lnprefs-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        p.save(&dir);
        assert_eq!(Prefs::load(&dir), p);
        // Old prefs files (pre-issue-18) lack pane_widths: default survives.
        std::fs::write(
            dir.join(FILE),
            r#"{"appearance":"Follow","lang":"Follow","editor_palette":"","vim_default":false,"recents":[]}"#,
        )
        .unwrap();
        assert_eq!(Prefs::load(&dir).pane_widths, PaneWidths::default());
        std::fs::remove_dir_all(&dir).ok();
    }
}
