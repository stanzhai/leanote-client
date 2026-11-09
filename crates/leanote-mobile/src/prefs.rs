//! 移动端本地偏好（issue 07）：JSON 存 app 数据目录。仅 UI 偏好。
//! 主题对齐桌面 ADR-0014 修订：单字段 `ui_theme` 存族干（"" = Navop），
//! 明暗自动匹配变体；强调色 "" = 跟随主题 primary，否则为色板 id。

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

#[derive(Clone, Serialize, Deserialize)]
pub struct MobilePrefs {
    pub appearance: String,
    /// 主题族干（"Gruvbox"）或单模名（"Matrix"）；"" = Navop 默认。
    pub ui_theme: String,
    /// "" = 跟随主题 primary；否则 theme::ACCENTS 的 id。
    pub accent: String,
    pub lang: String,
    pub zoom: f32,
}

impl Default for MobilePrefs {
    fn default() -> Self {
        Self {
            appearance: "follow".into(),
            ui_theme: String::new(),
            accent: String::new(),
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
        let Some(bytes) = std::fs::read(path()).ok() else {
            return Self::default();
        };
        let mut p: MobilePrefs = serde_json::from_slice(&bytes).unwrap_or_default();
        // 旧双字段格式迁移（ADR-0014 修订前的中间格式）：浅色主题名去掉
        // " Light" 后缀收编为族干；无浅色字段则保持默认。
        if p.ui_theme.is_empty() {
            if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&bytes) {
                if let Some(light) = v.get("light_theme").and_then(|s| s.as_str()) {
                    p.ui_theme = light.strip_suffix(" Light").unwrap_or(light).to_string();
                }
            }
        }
        p
    }

    pub fn save(&self) {
        if let Ok(json) = serde_json::to_vec_pretty(self) {
            let _ = std::fs::create_dir_all(app_data_dir());
            let _ = std::fs::write(path(), json);
        }
    }
}
