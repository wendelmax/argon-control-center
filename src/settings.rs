//! User preferences persisted in `$XDG_CONFIG_HOME/argon-control-center/settings.json`
//! (default `~/.config/argon-control-center/settings.json`).

use crate::i18n::Language;
use crate::theme::ThemeModel;
use crate::ui::ActiveTab;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub language: Language,
    pub theme: ThemeModel,
    pub tab: ActiveTab,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            language: Language::EnUs,
            theme: ThemeModel::KdeCompactOverview,
            tab: ActiveTab::Dashboard,
        }
    }
}

pub fn path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("argon-control-center").join("settings.json"))
}

/// Loads saved settings; missing or invalid files fall back to defaults.
pub fn load() -> Settings {
    path()
        .and_then(|p| fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save(s: &Settings) -> std::io::Result<()> {
    let Some(p) = path() else {
        return Ok(());
    };
    if let Some(dir) = p.parent() {
        fs::create_dir_all(dir)?;
    }
    let json = serde_json::to_string_pretty(s).map_err(std::io::Error::other)?;
    // Write-then-rename so a crash never leaves a truncated file.
    let tmp = p.with_extension("json.tmp");
    fs::write(&tmp, json)?;
    fs::rename(tmp, p)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_to_english_and_tolerates_partial_files() {
        assert_eq!(Settings::default().language, Language::EnUs);
        let s: Settings = serde_json::from_str(r#"{"language":"PtBr"}"#).unwrap();
        assert_eq!(s.language, Language::PtBr);
        assert_eq!(s.theme, ThemeModel::KdeCompactOverview);
        let json = serde_json::to_string(&Settings::default()).unwrap();
        assert_eq!(
            serde_json::from_str::<Settings>(&json).unwrap(),
            Settings::default()
        );
    }
}
