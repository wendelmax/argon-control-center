//! Cross-desktop system color detection.
//!
//! Resolution order (first source that yields colors wins, later sources only fill gaps):
//! 1. **KDE Plasma** – `~/.config/kdeglobals` `[Colors:*]` sections.
//! 2. **GTK desktops** (GNOME, Cinnamon, MATE, XFCE, Budgie, Pantheon, LXDE, Unity…) –
//!    active GTK theme name (gsettings / xfconf / `gtk-3.0/settings.ini`) and its
//!    `@define-color` palette from `gtk.css` / `gtk-dark.css`, plus GNOME 47+ `accent-color`.
//! 3. **XDG Desktop Portal** (`org.freedesktop.appearance`) – `color-scheme` (dark/light) and
//!    `accent-color`; works on any compositor/WM running xdg-desktop-portal.
//! 4. Built-in Breeze Dark / Adwaita defaults.

use egui::Color32;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DesktopKind {
    Kde,
    Gnome,
    Cinnamon,
    Mate,
    Xfce,
    Lxqt,
    Other,
}

impl DesktopKind {
    pub fn detect() -> Self {
        let raw = std::env::var("XDG_CURRENT_DESKTOP")
            .or_else(|_| std::env::var("XDG_SESSION_DESKTOP"))
            .or_else(|_| std::env::var("DESKTOP_SESSION"))
            .unwrap_or_default()
            .to_ascii_lowercase();
        Self::from_desktop_string(&raw)
    }

    pub fn from_desktop_string(raw: &str) -> Self {
        let raw = raw.to_ascii_lowercase();
        if raw.contains("kde") || raw.contains("plasma") {
            DesktopKind::Kde
        } else if raw.contains("cinnamon") {
            DesktopKind::Cinnamon
        } else if raw.contains("mate") {
            DesktopKind::Mate
        } else if raw.contains("xfce") {
            DesktopKind::Xfce
        } else if raw.contains("lxqt") {
            DesktopKind::Lxqt
        } else if raw.contains("gnome")
            || raw.contains("unity")
            || raw.contains("budgie")
            || raw.contains("pantheon")
            || raw.contains("cosmic")
        {
            DesktopKind::Gnome
        } else {
            DesktopKind::Other
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            DesktopKind::Kde => "KDE Plasma",
            DesktopKind::Gnome => "GNOME",
            DesktopKind::Cinnamon => "Cinnamon",
            DesktopKind::Mate => "MATE",
            DesktopKind::Xfce => "XFCE",
            DesktopKind::Lxqt => "LXQt",
            DesktopKind::Other => "WM",
        }
    }
}

/// Where the active palette came from (shown in tooltips).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaletteSource {
    KdeGlobals,
    GtkTheme,
    Portal,
    Fallback,
}

impl PaletteSource {
    pub fn describe(&self) -> &'static str {
        match self {
            PaletteSource::KdeGlobals => "~/.config/kdeglobals",
            PaletteSource::GtkTheme => "GTK theme (gtk.css @define-color)",
            PaletteSource::Portal => "xdg-desktop-portal (org.freedesktop.appearance)",
            PaletteSource::Fallback => "built-in default",
        }
    }
}

/// Partial palette: every field is optional so sources can be layered.
#[derive(Debug, Clone, Default)]
pub struct SystemColors {
    pub prefer_dark: Option<bool>,
    pub window_bg: Option<Color32>,
    pub window_alt_bg: Option<Color32>,
    pub view_bg: Option<Color32>,
    pub view_alt_bg: Option<Color32>,
    pub header_bg: Option<Color32>,
    pub button_bg: Option<Color32>,
    pub selection_bg: Option<Color32>,
    pub accent: Option<Color32>,
    pub text_normal: Option<Color32>,
    pub text_inactive: Option<Color32>,
    pub positive: Option<Color32>,
    pub neutral: Option<Color32>,
    pub negative: Option<Color32>,
    pub visited: Option<Color32>,
    pub border: Option<Color32>,
}

impl SystemColors {
    fn has_surface(&self) -> bool {
        self.window_bg.is_some() || self.view_bg.is_some()
    }

    /// Fill any missing field from `other`.
    fn fill_from(&mut self, other: &SystemColors) {
        macro_rules! fill {
            ($($f:ident),*) => { $( if self.$f.is_none() { self.$f = other.$f; } )* };
        }
        fill!(
            prefer_dark,
            window_bg,
            window_alt_bg,
            view_bg,
            view_alt_bg,
            header_bg,
            button_bg,
            selection_bg,
            accent,
            text_normal,
            text_inactive,
            positive,
            neutral,
            negative,
            visited,
            border
        );
    }
}

/// Detects the current desktop's palette. Returns the merged colors, the primary source and desktop.
pub fn detect() -> (SystemColors, PaletteSource, DesktopKind) {
    let desktop = DesktopKind::detect();
    let home = home_dir();

    let mut colors = SystemColors::default();
    let mut source = PaletteSource::Fallback;

    // 1. KDE (also honoured on other desktops when the user runs Plasma apps with a kdeglobals scheme
    //    and no GTK theme could be read).
    let portal = read_portal();
    let kde = if desktop == DesktopKind::Kde || desktop == DesktopKind::Lxqt {
        read_kdeglobals(&home)
    } else {
        None
    };
    if let Some(kde) = kde {
        colors = kde;
        source = PaletteSource::KdeGlobals;
    } else if let Some(gtk) = read_gtk_colors(desktop, &home, portal.prefer_dark) {
        colors = gtk;
        source = PaletteSource::GtkTheme;
    }

    // 2. Portal: authoritative for dark/light preference and accent on every desktop.
    if source == PaletteSource::Fallback
        && (portal.prefer_dark.is_some() || portal.accent.is_some())
    {
        source = PaletteSource::Portal;
    }
    if source != PaletteSource::KdeGlobals {
        // On GTK desktops the portal's accent reflects the user's choice better than the theme.
        if portal.accent.is_some() {
            colors.accent = portal.accent;
            colors.selection_bg = portal.accent;
        }
    }
    colors.fill_from(&portal);

    // 3. Last resort: KDE file even outside KDE (e.g. a WM with qt5ct + kdeglobals).
    if !colors.has_surface() {
        if let Some(kde) = read_kdeglobals(&home) {
            colors.fill_from(&kde);
            if source == PaletteSource::Fallback {
                source = PaletteSource::KdeGlobals;
            }
        }
    }

    (colors, source, desktop)
}

fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/root"))
}

fn config_dir(home: &Path) -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".config"))
}

// ---------------------------------------------------------------------------------------------
// KDE
// ---------------------------------------------------------------------------------------------

fn read_kdeglobals(home: &Path) -> Option<SystemColors> {
    let content = fs::read_to_string(config_dir(home).join("kdeglobals")).ok()?;
    let c = parse_kde_ini(&content);
    c.has_surface().then_some(c)
}

pub fn parse_kde_ini(content: &str) -> SystemColors {
    let mut c = SystemColors::default();
    let mut section = String::new();
    for raw_line in content.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            section = line[1..line.len() - 1].to_string();
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        let Some(col) = parse_rgb_triplet(v.trim()) else {
            continue;
        };
        let slot = match (section.as_str(), k.trim()) {
            ("Colors:Window", "BackgroundNormal") => &mut c.window_bg,
            ("Colors:Window", "BackgroundAlternate") => &mut c.window_alt_bg,
            ("Colors:View", "BackgroundNormal") => &mut c.view_bg,
            ("Colors:View", "BackgroundAlternate") => &mut c.view_alt_bg,
            ("Colors:Header", "BackgroundNormal") => &mut c.header_bg,
            ("Colors:Button", "BackgroundNormal") => &mut c.button_bg,
            ("Colors:Selection", "BackgroundNormal") => &mut c.selection_bg,
            ("Colors:View", "DecorationFocus") => &mut c.accent,
            ("Colors:View", "ForegroundNormal") => &mut c.text_normal,
            ("Colors:View", "ForegroundInactive") => &mut c.text_inactive,
            ("Colors:View", "ForegroundPositive") => &mut c.positive,
            ("Colors:View", "ForegroundNeutral") => &mut c.neutral,
            ("Colors:View", "ForegroundNegative") => &mut c.negative,
            ("Colors:View", "ForegroundVisited") => &mut c.visited,
            _ => continue,
        };
        *slot = Some(col);
    }
    if let Some(bg) = c.window_bg {
        c.prefer_dark = Some(luminance(bg) < 0.5);
    }
    c
}

fn parse_rgb_triplet(s: &str) -> Option<Color32> {
    let mut parts = s.split(',');
    let r: u8 = parts.next()?.trim().parse().ok()?;
    let g: u8 = parts.next()?.trim().parse().ok()?;
    let b: u8 = parts.next()?.trim().parse().ok()?;
    Some(Color32::from_rgb(r, g, b))
}

// ---------------------------------------------------------------------------------------------
// GTK (GNOME / Cinnamon / MATE / XFCE / generic)
// ---------------------------------------------------------------------------------------------

fn run(cmd: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(cmd)
        .args(args)
        .env("LC_ALL", "C")
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!s.is_empty()).then_some(s)
}

fn gsettings(schema: &str, key: &str) -> Option<String> {
    run("gsettings", &["get", schema, key]).map(|s| s.trim_matches('\'').to_string())
}

fn gtk_settings_ini(home: &Path) -> (Option<String>, Option<bool>) {
    for dir in ["gtk-4.0", "gtk-3.0"] {
        if let Ok(content) = fs::read_to_string(config_dir(home).join(dir).join("settings.ini")) {
            let mut theme = None;
            let mut dark = None;
            for line in content.lines() {
                if let Some((k, v)) = line.split_once('=') {
                    let v = v.trim().trim_matches('"');
                    match k.trim() {
                        "gtk-theme-name" => theme = Some(v.to_string()),
                        "gtk-application-prefer-dark-theme" => {
                            dark = Some(v == "1" || v.eq_ignore_ascii_case("true"))
                        }
                        _ => {}
                    }
                }
            }
            if theme.is_some() || dark.is_some() {
                return (theme, dark);
            }
        }
    }
    (None, None)
}

fn gtk_theme_name(desktop: DesktopKind, home: &Path) -> (Option<String>, Option<bool>) {
    let from_tool = match desktop {
        DesktopKind::Xfce => run("xfconf-query", &["-c", "xsettings", "-p", "/Net/ThemeName"]),
        DesktopKind::Cinnamon => gsettings("org.cinnamon.desktop.interface", "gtk-theme"),
        DesktopKind::Mate => gsettings("org.mate.interface", "gtk-theme"),
        DesktopKind::Gnome => gsettings("org.gnome.desktop.interface", "gtk-theme"),
        _ => None,
    };
    let (ini_theme, ini_dark) = gtk_settings_ini(home);
    let theme = from_tool
        .or(ini_theme)
        .or_else(|| gsettings("org.gnome.desktop.interface", "gtk-theme"));
    let dark = gsettings("org.gnome.desktop.interface", "color-scheme")
        .map(|s| s == "prefer-dark")
        .filter(|d| *d)
        .or(ini_dark);
    (theme, dark)
}

fn theme_roots(home: &Path) -> Vec<PathBuf> {
    let mut roots = vec![home.join(".themes"), home.join(".local/share/themes")];
    if let Ok(dirs) = std::env::var("XDG_DATA_DIRS") {
        roots.extend(
            dirs.split(':')
                .filter(|d| !d.is_empty())
                .map(|d| PathBuf::from(d).join("themes")),
        );
    }
    roots.push(PathBuf::from("/usr/share/themes"));
    roots.push(PathBuf::from("/usr/local/share/themes"));
    roots
}

/// Reads a css file, following `@import url("…")` (relative paths, up to 3 levels).
fn read_css_following_imports(path: &std::path::Path, depth: u8) -> Option<String> {
    let css = fs::read_to_string(path).ok()?;
    if css.contains("@define-color") || depth == 0 {
        return Some(css);
    }
    for line in css.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("@import url(") {
            let target = rest
                .trim_end_matches(';')
                .trim_end_matches(')')
                .trim_matches(|c| c == '"' || c == '\'');
            if target.starts_with("resource:") {
                continue;
            }
            let next = path.parent()?.join(target);
            if let Some(found) = read_css_following_imports(&next, depth - 1) {
                if found.contains("@define-color") {
                    return Some(found);
                }
            }
        }
    }
    Some(css)
}

/// Returns `(css, is_dark_variant)` for the theme.
fn find_theme_css(home: &Path, theme: &str, dark: bool) -> Option<(String, bool)> {
    let roots = theme_roots(home);
    let mut candidates: Vec<(String, &str, bool)> = Vec::new();
    if dark {
        for suffix in ["-Dark", "-dark", "_dark", "-Darker"] {
            candidates.push((format!("{theme}{suffix}"), "gtk.css", true));
        }
        candidates.push((theme.to_string(), "gtk-dark.css", true));
    }
    candidates.push((theme.to_string(), "gtk.css", false));

    for (name, file, is_dark_variant) in &candidates {
        for root in &roots {
            for ver in ["gtk-3.0", "gtk-4.0"] {
                let p = root.join(name).join(ver).join(file);
                if let Some(css) = read_css_following_imports(&p, 3) {
                    if css.contains("@define-color") {
                        return Some((css, *is_dark_variant));
                    }
                }
            }
        }
    }
    None
}

/// Parses `@define-color name #rrggbb;` / `rgb(...)` / `rgba(...)` / `@alias;` lines.
pub fn parse_gtk_define_colors(css: &str) -> std::collections::HashMap<String, Color32> {
    let mut raw: std::collections::HashMap<String, String> = Default::default();
    for line in css.lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix("@define-color") else {
            continue;
        };
        let rest = rest.trim().trim_end_matches(';').trim();
        if let Some((name, value)) = rest.split_once(char::is_whitespace) {
            raw.entry(name.to_string())
                .or_insert_with(|| value.trim().to_string());
        }
    }
    let mut out = std::collections::HashMap::new();
    for name in raw.keys() {
        let mut v = raw[name].clone();
        for _ in 0..8 {
            match v.strip_prefix('@') {
                Some(alias) => match raw.get(alias) {
                    Some(next) => v = next.clone(),
                    None => break,
                },
                None => break,
            }
        }
        if let Some(c) = parse_css_color(&v) {
            out.insert(name.clone(), c);
        }
    }
    out
}

fn parse_css_color(v: &str) -> Option<Color32> {
    let v = v.trim();
    if let Some(hex) = v.strip_prefix('#') {
        let hex = if hex.len() == 3 {
            hex.chars().flat_map(|c| [c, c]).collect::<String>()
        } else {
            hex.to_string()
        };
        if hex.len() >= 6 {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            return Some(Color32::from_rgb(r, g, b));
        }
        return None;
    }
    let inner = v
        .strip_prefix("rgba(")
        .or_else(|| v.strip_prefix("rgb("))?
        .trim_end_matches(')');
    let parts: Vec<&str> = inner.split(',').map(str::trim).collect();
    if parts.len() < 3 {
        return None;
    }
    let ch = |s: &str| -> Option<u8> {
        if let Some(p) = s.strip_suffix('%') {
            Some((p.parse::<f32>().ok()? * 2.55).round().clamp(0.0, 255.0) as u8)
        } else {
            Some(s.parse::<f32>().ok()?.round().clamp(0.0, 255.0) as u8)
        }
    };
    Some(Color32::from_rgb(
        ch(parts[0])?,
        ch(parts[1])?,
        ch(parts[2])?,
    ))
}

fn gnome_accent(name: &str) -> Option<Color32> {
    // GNOME 47+ `org.gnome.desktop.interface accent-color` named accents (libadwaita values).
    let c = match name {
        "blue" => (0x35, 0x84, 0xe4),
        "teal" => (0x21, 0x90, 0xa4),
        "green" => (0x3a, 0x94, 0x4a),
        "yellow" => (0xc8, 0x88, 0x00),
        "orange" => (0xed, 0x5b, 0x00),
        "red" => (0xe6, 0x2d, 0x42),
        "pink" => (0xd5, 0x61, 0x99),
        "purple" => (0x91, 0x41, 0xac),
        "slate" => (0x6f, 0x83, 0x96),
        _ => return None,
    };
    Some(Color32::from_rgb(c.0, c.1, c.2))
}

fn adwaita_defaults(dark: bool) -> SystemColors {
    let rgb = |r, g, b| Some(Color32::from_rgb(r, g, b));
    if dark {
        SystemColors {
            prefer_dark: Some(true),
            window_bg: rgb(0x22, 0x22, 0x26),
            window_alt_bg: rgb(0x2e, 0x2e, 0x32),
            view_bg: rgb(0x1d, 0x1d, 0x20),
            view_alt_bg: rgb(0x2e, 0x2e, 0x32),
            header_bg: rgb(0x2e, 0x2e, 0x32),
            button_bg: rgb(0x36, 0x36, 0x3a),
            text_normal: rgb(0xff, 0xff, 0xff),
            text_inactive: rgb(0xa8, 0xa8, 0xac),
            border: rgb(0x45, 0x45, 0x4a),
            positive: rgb(0x26, 0xa2, 0x69),
            neutral: rgb(0xe5, 0xa5, 0x0a),
            negative: rgb(0xe0, 0x1b, 0x24),
            visited: rgb(0x91, 0x41, 0xac),
            ..Default::default()
        }
    } else {
        SystemColors {
            prefer_dark: Some(false),
            window_bg: rgb(0xfa, 0xfa, 0xfb),
            window_alt_bg: rgb(0xeb, 0xeb, 0xed),
            view_bg: rgb(0xff, 0xff, 0xff),
            view_alt_bg: rgb(0xf4, 0xf4, 0xf6),
            header_bg: rgb(0xeb, 0xeb, 0xed),
            button_bg: rgb(0xe6, 0xe6, 0xe8),
            text_normal: rgb(0x2e, 0x34, 0x36),
            text_inactive: rgb(0x6b, 0x6f, 0x73),
            border: rgb(0xd0, 0xd0, 0xd4),
            positive: rgb(0x1b, 0x80, 0x53),
            neutral: rgb(0x9c, 0x6e, 0x03),
            negative: rgb(0xc0, 0x1c, 0x28),
            visited: rgb(0x81, 0x3d, 0x9c),
            ..Default::default()
        }
    }
}

fn read_gtk_colors(
    desktop: DesktopKind,
    home: &Path,
    portal_dark: Option<bool>,
) -> Option<SystemColors> {
    let (theme, pref_dark) = gtk_theme_name(desktop, home);
    let pref_dark = portal_dark.or(pref_dark);
    let theme_says_dark = theme
        .as_deref()
        .map(|t| t.to_ascii_lowercase().contains("dark"));
    let dark = pref_dark.or(theme_says_dark);

    let accent = gsettings("org.gnome.desktop.interface", "accent-color")
        .as_deref()
        .and_then(gnome_accent);

    let mut c = SystemColors::default();
    if let Some(theme) = theme.as_deref() {
        if let Some((css, _)) = find_theme_css(home, theme, dark.unwrap_or(false)) {
            let m = parse_gtk_define_colors(&css);
            let get = |keys: &[&str]| keys.iter().find_map(|k| m.get(*k).copied());
            c.window_bg = get(&["theme_bg_color", "bg_color", "window_bg_color"]);
            c.view_bg = get(&["theme_base_color", "base_color", "view_bg_color"]);
            c.header_bg = get(&["headerbar_bg_color", "theme_bg_color"]);
            c.text_normal = get(&["theme_fg_color", "fg_color", "window_fg_color"]);
            c.text_inactive = get(&[
                "insensitive_fg_color",
                "theme_unfocused_fg_color",
                "unfocused_fg_color",
            ]);
            c.selection_bg = get(&[
                "theme_selected_bg_color",
                "selected_bg_color",
                "accent_bg_color",
            ]);
            c.accent = get(&[
                "accent_color",
                "theme_selected_bg_color",
                "selected_bg_color",
            ]);
            c.border = get(&["borders", "unfocused_borders"]);
            c.positive = get(&["success_color"]);
            c.neutral = get(&["warning_color"]);
            c.negative = get(&["error_color"]);
            c.button_bg = get(&["button_bg_color", "insensitive_bg_color"]);
        }
    }

    // Nothing read at all → no GTK info whatsoever.
    if theme.is_none() && dark.is_none() && accent.is_none() && !c.has_surface() {
        return None;
    }

    if let Some(bg) = c.window_bg.or(c.view_bg) {
        let css_dark = luminance(bg) < 0.5;
        if dark.is_some_and(|d| d != css_dark) {
            // Theme only ships the opposite variant: keep accent/semantic colors, drop surfaces.
            for f in [
                &mut c.window_bg,
                &mut c.view_bg,
                &mut c.header_bg,
                &mut c.text_normal,
                &mut c.text_inactive,
                &mut c.border,
                &mut c.button_bg,
            ] {
                *f = None;
            }
            c.prefer_dark = dark;
        } else {
            c.prefer_dark = Some(css_dark);
        }
    } else {
        c.prefer_dark = dark;
    }
    if accent.is_some() {
        c.accent = accent;
        c.selection_bg = accent;
    }
    // Builtin themes (Adwaita, Yaru compiled into gresources) have no css on disk.
    if !c.has_surface() {
        c.fill_from(&adwaita_defaults(c.prefer_dark.unwrap_or(false)));
    }
    Some(c)
}

// ---------------------------------------------------------------------------------------------
// XDG Desktop Portal
// ---------------------------------------------------------------------------------------------

fn portal_read(key: &str) -> Option<String> {
    // `timeout` keeps a missing / hung portal from blocking the caller.
    let args = [
        "call",
        "--session",
        "--timeout",
        "2",
        "--dest",
        "org.freedesktop.portal.Desktop",
        "--object-path",
        "/org/freedesktop/portal/desktop",
        "--method",
        "org.freedesktop.portal.Settings.ReadOne",
        "org.freedesktop.appearance",
        key,
    ];
    run("gdbus", &args)
}

fn read_portal() -> SystemColors {
    let mut c = SystemColors::default();
    if let Some(s) = portal_read("color-scheme") {
        // `(<uint32 1>,)` → 1 = prefer dark, 2 = prefer light, 0 = no preference
        if s.contains("uint32 1") {
            c.prefer_dark = Some(true);
        } else if s.contains("uint32 2") {
            c.prefer_dark = Some(false);
        }
    }
    if let Some(s) = portal_read("accent-color") {
        c.accent = parse_portal_accent(&s);
    }
    c
}

/// Parses `(<(0.239, 0.682, 0.913)>,)`.
pub fn parse_portal_accent(s: &str) -> Option<Color32> {
    let start = s.find("(<(")? + 3;
    let end = s[start..].find(')')? + start;
    let nums: Vec<f32> = s[start..end]
        .split(',')
        .filter_map(|p| p.trim().parse::<f32>().ok())
        .collect();
    if nums.len() != 3 || nums.iter().any(|v| !(0.0..=1.0).contains(v)) {
        return None;
    }
    let to = |v: f32| (v * 255.0).round() as u8;
    Some(Color32::from_rgb(to(nums[0]), to(nums[1]), to(nums[2])))
}

// ---------------------------------------------------------------------------------------------
// Color math helpers
// ---------------------------------------------------------------------------------------------

/// Relative luminance (WCAG) in 0..1.
pub fn luminance(c: Color32) -> f32 {
    let f = |v: u8| {
        let v = v as f32 / 255.0;
        if v <= 0.03928 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * f(c.r()) + 0.7152 * f(c.g()) + 0.0722 * f(c.b())
}

pub fn contrast_ratio(a: Color32, b: Color32) -> f32 {
    let (la, lb) = (luminance(a), luminance(b));
    let (hi, lo) = if la > lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}

/// Linear mix: `t = 0` → `a`, `t = 1` → `b`.
pub fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()))
}

/// Best readable text color (white or near-black) on top of `bg`.
pub fn on_color(bg: Color32) -> Color32 {
    if contrast_ratio(bg, Color32::WHITE) >= contrast_ratio(bg, Color32::from_rgb(20, 20, 20)) {
        Color32::WHITE
    } else {
        Color32::from_rgb(20, 20, 20)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_strings() {
        assert_eq!(DesktopKind::from_desktop_string("KDE"), DesktopKind::Kde);
        assert_eq!(
            DesktopKind::from_desktop_string("ubuntu:GNOME"),
            DesktopKind::Gnome
        );
        assert_eq!(DesktopKind::from_desktop_string("XFCE"), DesktopKind::Xfce);
        assert_eq!(
            DesktopKind::from_desktop_string("X-Cinnamon"),
            DesktopKind::Cinnamon
        );
        assert_eq!(DesktopKind::from_desktop_string("i3"), DesktopKind::Other);
    }

    #[test]
    fn portal_accent() {
        let c = parse_portal_accent(
            "(<(0.23921568691730499, 0.68235296010971069, 0.91372549533843994)>,)",
        );
        assert_eq!(c, Some(Color32::from_rgb(61, 174, 233)));
        assert_eq!(parse_portal_accent("(<(-1.0, -1.0, -1.0)>,)"), None);
    }

    #[test]
    fn gtk_css() {
        let css = "@define-color theme_bg_color #f6f5f4;\n@define-color theme_selected_bg_color @blue;\n@define-color blue rgb(53,132,228);\n@define-color theme_fg_color #2e3436;";
        let m = parse_gtk_define_colors(css);
        assert_eq!(m["theme_bg_color"], Color32::from_rgb(0xf6, 0xf5, 0xf4));
        assert_eq!(
            m["theme_selected_bg_color"],
            Color32::from_rgb(53, 132, 228)
        );
        assert_eq!(m["theme_fg_color"], Color32::from_rgb(0x2e, 0x34, 0x36));
    }

    #[test]
    fn kde_ini() {
        let c = parse_kde_ini("[Colors:Window]\nBackgroundNormal=239,240,241\n[Colors:View]\nDecorationFocus=61,174,233\n");
        assert_eq!(c.prefer_dark, Some(false));
        assert_eq!(c.accent, Some(Color32::from_rgb(61, 174, 233)));
    }
}

#[cfg(test)]
mod live {
    /// `cargo test print_detected -- --ignored --nocapture` shows what this desktop resolves to.
    #[test]
    #[ignore]
    fn print_detected() {
        let pal = crate::theme::KdeBreezePalette::load_from_system();
        println!("{}\n{:?}", pal.describe(), pal);
    }
}
