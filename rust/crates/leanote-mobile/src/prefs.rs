//! 移动端本地偏好（issue 07）：JSON 存 app 数据目录。仅 UI 偏好，
//! 语义对齐桌面 prefs（appearance/lang/zoom/light_theme/dark_theme）。

use leanote_core::app_data_dir;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub const APPEARANCES: &[(&str, &str, &str)] = &[
    // (id, zh, en)
    ("follow", "跟随系统", "Follow System"),
    ("light", "浅色", "Light"),
    ("dark", "深色", "Dark"),
];
pub const ZOOMS: &[f32] = &[1.0, 1.1, 1.2];
pub const LANGS: &[(&str, &str)] = &[("zh", "中文"), ("en", "English")];
pub const DEFAULT_LIGHT: &str = "Navop Light";
pub const DEFAULT_DARK: &str = "Navop Dark";

#[derive(Clone, Serialize, Deserialize)]
pub struct MobilePrefs {
    pub appearance: String,
    pub light_theme: String,
    pub dark_theme: String,
    pub lang: String,
    pub zoom: f32,
}

impl Default for MobilePrefs {
    fn default() -> Self {
        Self {
            appearance: "follow".into(),
            light_theme: DEFAULT_LIGHT.into(),
            dark_theme: DEFAULT_DARK.into(),
            lang: "zh".into(),
            zoom: 1.0,
        }
    }
}

fn path() -> PathBuf {
    app_data_dir().join("mobile-prefs.json")
}

impl MobilePrefs {
    pub fn load() -> Self {
        std::fs::read(path())
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        if let Ok(json) = serde_json::to_vec_pretty(self) {
            let _ = std::fs::create_dir_all(app_data_dir());
            let _ = std::fs::write(path(), json);
        }
    }
}
