//! Settings and the list of monitors switched off, in `~/Library/Application Support/ScreenYield`.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Ask GitHub now and then for a newer version.
    pub check_updates: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self { check_updates: true }
    }
}

pub fn dir() -> PathBuf {
    std::env::var_os("SY_CONFIG_DIR").map(PathBuf::from).unwrap_or_else(|| {
        let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
        home.join("Library/Application Support/ScreenYield")
    })
}

impl Settings {
    /// Unknown or broken fields fall back to their defaults one by one.
    pub fn parse(text: &str) -> Self {
        let Ok(serde_json::Value::Object(map)) = serde_json::from_str::<serde_json::Value>(text) else {
            return Self::default();
        };
        let mut settings = Self::default();
        if let Some(v) = map.get("check_updates").and_then(serde_json::Value::as_bool) {
            settings.check_updates = v;
        }
        settings
    }

    pub fn load() -> Self {
        std::fs::read_to_string(dir().join("settings.json"))
            .map(|t| Self::parse(&t))
            .unwrap_or_default()
    }

    pub fn save(&self) {
        let dir = dir();
        let _ = std::fs::create_dir_all(&dir);
        if let Ok(text) = serde_json::to_string_pretty(self) {
            let tmp = dir.join("settings.json.tmp");
            if std::fs::write(&tmp, text).is_ok() {
                let _ = std::fs::rename(tmp, dir.join("settings.json"));
            }
        }
    }
}

fn off_file() -> PathBuf {
    dir().join("off.txt")
}

/// The monitors switched off by ScreenYield (see `model::encode`).
pub fn load_off() -> Vec<u32> {
    std::fs::read_to_string(off_file())
        .map(|t| crate::model::decode(&t))
        .unwrap_or_default()
}

pub fn save_off(ids: &[u32]) {
    if ids.is_empty() {
        let _ = std::fs::remove_file(off_file());
        return;
    }
    let _ = std::fs::create_dir_all(dir());
    let _ = std::fs::write(off_file(), crate::model::encode(ids));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_are_read_field_by_field() {
        assert!(Settings::parse("").check_updates);
        assert!(!Settings::parse(r#"{"check_updates": false}"#).check_updates);
        assert!(Settings::parse(r#"{"check_updates": "no", "other": 1}"#).check_updates);
    }
}
