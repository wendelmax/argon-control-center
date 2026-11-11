//! User preferences persisted in `$XDG_CONFIG_HOME/argon-control-center/settings.json`
//! (default `~/.config/argon-control-center/settings.json`).
//!
//! Appearance is split into three independent choices: navigation [`Layout`], [`ColorScheme`]
//! and [`DashboardStyle`]. Files written by older versions (single `theme` field) are migrated.

use crate::argon::TempUnit;
use crate::i18n::Language;
use crate::ui::ActiveTab;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// Navigation chrome.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Layout {
    /// Left sidebar navigation (KDE Kirigami style); collapses to a tab strip on narrow windows.
    Sidebar,
    /// Horizontal tab strip under a branded header.
    TopTabs,
}

/// Where colors come from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ColorScheme {
    /// Follow the desktop (KDE, GTK, xdg-desktop-portal), light or dark.
    System,
    /// Fixed dark neon palette.
    Aorus,
}

/// What the Dashboard page shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DashboardStyle {
    /// Single screen: compact rings, charts and quick controls.
    Compact,
    /// Ring charts plus stacked history graphs.
    Charts,
    /// Large radial gauges.
    Gauges,
}

pub const UI_SCALE_MIN: f32 = 0.8;
pub const UI_SCALE_MAX: f32 = 1.6;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub language: Language,
    pub layout: Layout,
    pub colors: ColorScheme,
    pub dashboard: DashboardStyle,
    /// Unit used to display temperatures in the app (independent of the Argon OLED setting).
    pub temp_unit: TempUnit,
    /// Interface zoom factor (egui zoom), clamped to `UI_SCALE_MIN..=UI_SCALE_MAX`.
    pub ui_scale: f32,
    /// Sidebar layout shows icons only.
    pub sidebar_collapsed: bool,
    pub tab: ActiveTab,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            language: Language::EnUs,
            layout: Layout::Sidebar,
            colors: ColorScheme::System,
            dashboard: DashboardStyle::Compact,
            temp_unit: TempUnit::Celsius,
            ui_scale: 1.0,
            sidebar_collapsed: false,
            tab: ActiveTab::Dashboard,
        }
    }
}

/// Shape of files written before appearance was split (`"theme": "KdeBreeze"`).
#[derive(Deserialize, Default)]
#[serde(default)]
struct Legacy {
    theme: Option<String>,
    temp_unit: Option<TempUnit>,
}

impl Settings {
    /// Parses a settings file, migrating the legacy `theme` field when present.
    /// `fallback_unit` is used when the file has no temperature unit yet.
    pub fn from_json(json: &str, fallback_unit: TempUnit) -> Option<Settings> {
        let mut s: Settings = serde_json::from_str(json).ok()?;
        let legacy: Legacy = serde_json::from_str(json).unwrap_or_default();
        if legacy.temp_unit.is_none() {
            s.temp_unit = fallback_unit;
        }
        if let Some(theme) = legacy.theme.as_deref() {
            let (layout, colors, dashboard) = match theme {
                "KdeCompactOverview" => (
                    Layout::Sidebar,
                    ColorScheme::System,
                    DashboardStyle::Compact,
                ),
                "KdeBreeze" => (Layout::Sidebar, ColorScheme::System, DashboardStyle::Charts),
                "BreezeClassicLayout" => {
                    (Layout::TopTabs, ColorScheme::System, DashboardStyle::Gauges)
                }
                "AorusGaming" => (Layout::TopTabs, ColorScheme::Aorus, DashboardStyle::Gauges),
                _ => (s.layout, s.colors, s.dashboard),
            };
            s.layout = layout;
            s.colors = colors;
            s.dashboard = dashboard;
        }
        s.ui_scale = clamp_scale(s.ui_scale);
        Some(s)
    }
}

pub fn clamp_scale(v: f32) -> f32 {
    if v.is_finite() {
        v.clamp(UI_SCALE_MIN, UI_SCALE_MAX)
    } else {
        1.0
    }
}

pub fn path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("argon-control-center").join("settings.json"))
}

/// Loads saved settings. Missing or invalid files fall back to defaults; the temperature unit
/// then starts from the Argon daemon setting (`/etc/argonunits.conf`).
pub fn load() -> Settings {
    let daemon_unit = crate::argon::load_temp_unit();
    path()
        .and_then(|p| fs::read_to_string(p).ok())
        .and_then(|s| Settings::from_json(&s, daemon_unit))
        .unwrap_or(Settings {
            temp_unit: daemon_unit,
            ..Settings::default()
        })
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
        let s = Settings::from_json(r#"{"language":"PtBr"}"#, TempUnit::Fahrenheit).unwrap();
        assert_eq!(s.language, Language::PtBr);
        assert_eq!(s.layout, Layout::Sidebar);
        assert_eq!(s.temp_unit, TempUnit::Fahrenheit);
        let json = serde_json::to_string(&Settings::default()).unwrap();
        assert_eq!(
            Settings::from_json(&json, TempUnit::Fahrenheit).unwrap(),
            Settings::default()
        );
    }

    #[test]
    fn migrates_legacy_theme_field() {
        let s = Settings::from_json(
            r#"{"language":"PtBr","theme":"AorusGaming","tab":"GpioLab"}"#,
            TempUnit::Celsius,
        )
        .unwrap();
        assert_eq!(
            (s.layout, s.colors, s.dashboard),
            (Layout::TopTabs, ColorScheme::Aorus, DashboardStyle::Gauges)
        );
        assert_eq!(s.tab, ActiveTab::GpioLab);
        let s = Settings::from_json(r#"{"theme":"KdeBreeze"}"#, TempUnit::Celsius).unwrap();
        assert_eq!(s.dashboard, DashboardStyle::Charts);
    }

    #[test]
    fn scale_is_clamped() {
        let s = Settings::from_json(r#"{"ui_scale":9.0}"#, TempUnit::Celsius).unwrap();
        assert_eq!(s.ui_scale, UI_SCALE_MAX);
        assert_eq!(clamp_scale(f32::NAN), 1.0);
    }
}
