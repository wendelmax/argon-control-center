use crate::system_colors::{
    self, contrast_ratio, luminance, mix, on_color, DesktopKind, PaletteSource, SystemColors,
};
use egui::{
    Align2, Color32, CornerRadius, FontFamily, FontId, Pos2, Sense, Shape, Stroke, StrokeKind,
    TextStyle, Ui, Vec2,
};
use std::collections::VecDeque;
use std::f32::consts::PI;

/// Available UI Theme & Layout Models.
/// - `KdeCompactOverview`: All-in-One No-Scroll Compact Overview (`overview.page` sensors + Area charts + Direct Fan/OC/Daemon controls on a single screen).
/// - `KdeBreeze`: Full KDE Plasma System Monitor (`plasma-systemmonitor`) Kirigami sidebar + Donut rings + stacked `history.page` area charts.
/// - `BreezeClassicLayout`: KDE Breeze Dark color palette (`~/.config/kdeglobals`) & subtle borders applied over the classic v1 layout & radial gauges.
/// - `AorusGaming`: Saved v1.0 Gigabyte AORUS Gaming theme (top double bar, automotive radial gauges, neon orange/cyan accents).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeModel {
    KdeCompactOverview,
    KdeBreeze,
    BreezeClassicLayout,
    AorusGaming,
}

/// Desktop palette (historically KDE-only, now resolved from any desktop via [`crate::system_colors`]):
/// KDE `kdeglobals`, GTK theme `@define-color`s (GNOME/Cinnamon/MATE/XFCE…), the XDG portal
/// accent / dark-light preference, then Breeze Dark defaults.
#[derive(Debug, Clone, Copy)]
pub struct KdeBreezePalette {
    pub window_bg: Color32,
    pub window_alt_bg: Color32,
    pub view_bg: Color32,
    pub view_alt_bg: Color32,
    pub header_bg: Color32,
    pub button_bg: Color32,
    pub selection_bg: Color32,
    pub focus_blue: Color32,
    pub text_normal: Color32,
    pub text_inactive: Color32,
    pub positive_green: Color32,
    pub neutral_orange: Color32,
    pub negative_red: Color32,
    pub visited_purple: Color32,
    pub border_subtle: Color32,
    // Derived surfaces (computed in `finalize`)
    pub is_dark: bool,
    pub hover_bg: Color32,
    pub selected_fill: Color32,
    pub on_selected: Color32,
    pub track: Color32,
    pub grid: Color32,
    pub source: PaletteSource,
    pub desktop: DesktopKind,
}

impl Default for KdeBreezePalette {
    fn default() -> Self {
        let mut p = Self {
            window_bg: Color32::from_rgb(32, 35, 38),        // #202326
            window_alt_bg: Color32::from_rgb(41, 44, 48),    // #292C30
            view_bg: Color32::from_rgb(20, 22, 24),          // #141618
            view_alt_bg: Color32::from_rgb(29, 31, 34),      // #1D1F22
            header_bg: Color32::from_rgb(41, 44, 48),        // #292C30
            button_bg: Color32::from_rgb(41, 44, 48),        // #292C30
            selection_bg: Color32::from_rgb(61, 174, 233),   // #3DAEE9
            focus_blue: Color32::from_rgb(61, 174, 233),     // #3DAEE9
            text_normal: Color32::from_rgb(252, 252, 252),   // #FCFCFC
            text_inactive: Color32::from_rgb(161, 169, 177), // #A1A9B1
            positive_green: Color32::from_rgb(39, 174, 96),  // #27AE60
            neutral_orange: Color32::from_rgb(246, 116, 0),  // #F67400
            negative_red: Color32::from_rgb(218, 68, 83),    // #DA4453
            visited_purple: Color32::from_rgb(155, 89, 182), // #9B59B6
            border_subtle: Color32::from_rgb(58, 63, 68),    // Subtle 1px frameContrast=0.2
            is_dark: true,
            hover_bg: Color32::TRANSPARENT,
            selected_fill: Color32::TRANSPARENT,
            on_selected: Color32::WHITE,
            track: Color32::TRANSPARENT,
            grid: Color32::TRANSPARENT,
            source: PaletteSource::Fallback,
            desktop: DesktopKind::Other,
        };
        p.finalize();
        p
    }
}

impl KdeBreezePalette {
    /// Reads the live desktop palette (KDE, GNOME, Cinnamon, MATE, XFCE, LXQt or any WM with
    /// xdg-desktop-portal). Light schemes are supported; missing values fall back to Breeze.
    pub fn load_from_system() -> Self {
        let (colors, source, desktop) = system_colors::detect();
        Self::from_system_colors(&colors, source, desktop)
    }

    pub fn from_system_colors(
        c: &SystemColors,
        source: PaletteSource,
        desktop: DesktopKind,
    ) -> Self {
        let surface_dark = c.window_bg.or(c.view_bg).map(|bg| luminance(bg) < 0.5);
        let wants_light = surface_dark
            .map(|d| !d)
            .unwrap_or(c.prefer_dark == Some(false));
        let mut pal = if wants_light {
            Self::breeze_light()
        } else {
            Self::default()
        };
        macro_rules! set {
            ($($dst:ident <- $src:ident),*) => { $( if let Some(v) = c.$src { pal.$dst = v; } )* };
        }
        set!(
            window_bg <- window_bg, view_bg <- view_bg, header_bg <- header_bg,
            button_bg <- button_bg, selection_bg <- selection_bg, focus_blue <- accent,
            text_normal <- text_normal, text_inactive <- text_inactive,
            positive_green <- positive, neutral_orange <- neutral, negative_red <- negative,
            visited_purple <- visited, border_subtle <- border
        );
        // Derive missing surfaces from the ones the source did provide.
        let dark = luminance(pal.window_bg) < 0.5;
        let towards = if dark { Color32::WHITE } else { Color32::BLACK };
        match (c.window_bg, c.view_bg) {
            (Some(w), None) => {
                pal.view_bg = if dark {
                    mix(w, Color32::BLACK, 0.25)
                } else {
                    mix(w, Color32::WHITE, 0.7)
                }
            }
            (None, Some(v)) => {
                pal.window_bg = if dark {
                    mix(v, Color32::WHITE, 0.06)
                } else {
                    mix(v, Color32::BLACK, 0.05)
                }
            }
            _ => {}
        }
        if c.text_normal.is_none() {
            pal.text_normal = if dark {
                Color32::from_rgb(252, 252, 252)
            } else {
                Color32::from_rgb(35, 38, 41)
            };
        }
        pal.window_alt_bg = c
            .window_alt_bg
            .unwrap_or_else(|| mix(pal.window_bg, towards, 0.05));
        pal.view_alt_bg = c
            .view_alt_bg
            .unwrap_or_else(|| mix(pal.view_bg, towards, 0.04));
        if c.header_bg.is_none() {
            pal.header_bg = pal.window_alt_bg;
        }
        if c.button_bg.is_none() {
            pal.button_bg = mix(pal.window_bg, towards, 0.08);
        }
        if c.border.is_none() {
            pal.border_subtle = mix(pal.window_bg, pal.text_normal, 0.18);
        }
        if c.selection_bg.is_none() {
            pal.selection_bg = pal.focus_blue;
        }
        if c.text_inactive.is_none() {
            pal.text_inactive = mix(pal.text_normal, pal.window_bg, 0.4);
        }
        // Guarantee legible text whatever the theme shipped.
        if contrast_ratio(pal.text_normal, pal.window_bg) < 4.5 {
            pal.text_normal = on_color(pal.window_bg);
        }
        if contrast_ratio(pal.text_inactive, pal.window_bg) < 3.0 {
            pal.text_inactive = mix(pal.text_normal, pal.window_bg, 0.3);
        }
        pal.source = source;
        pal.desktop = desktop;
        pal.finalize();
        pal
    }

    fn breeze_light() -> Self {
        let mut p = Self {
            window_bg: Color32::from_rgb(239, 240, 241), // #EFF0F1
            window_alt_bg: Color32::from_rgb(227, 229, 231),
            view_bg: Color32::from_rgb(255, 255, 255),
            view_alt_bg: Color32::from_rgb(247, 247, 247),
            header_bg: Color32::from_rgb(222, 224, 226),
            button_bg: Color32::from_rgb(252, 252, 252),
            text_normal: Color32::from_rgb(35, 38, 41), // #232629
            text_inactive: Color32::from_rgb(112, 125, 138), // #707D8A
            positive_green: Color32::from_rgb(39, 174, 96),
            neutral_orange: Color32::from_rgb(246, 116, 0),
            negative_red: Color32::from_rgb(218, 68, 83),
            border_subtle: Color32::from_rgb(188, 192, 196),
            ..Self::default()
        };
        p.finalize();
        p
    }

    /// Computes derived surfaces so every widget adapts to dark *and* light schemes.
    fn finalize(&mut self) {
        self.is_dark = luminance(self.window_bg) < 0.5;
        let towards = if self.is_dark {
            Color32::WHITE
        } else {
            Color32::BLACK
        };
        self.hover_bg = mix(
            self.window_bg,
            towards,
            if self.is_dark { 0.10 } else { 0.07 },
        );
        self.selected_fill = mix(
            self.window_bg,
            self.focus_blue,
            if self.is_dark { 0.45 } else { 0.85 },
        );
        self.on_selected = on_color(self.selected_fill);
        self.track = mix(self.view_bg, self.text_normal, 0.14);
        self.grid = mix(self.view_bg, self.text_normal, 0.08);
    }

    /// Kept for tests / callers that feed a raw kdeglobals string.
    pub fn apply_ini_str(&mut self, content: &str) {
        *self = Self::from_system_colors(
            &system_colors::parse_kde_ini(content),
            PaletteSource::KdeGlobals,
            DesktopKind::Kde,
        );
    }

    /// Short human description, e.g. "GNOME · GTK theme (gtk.css @define-color) · dark".
    pub fn describe(&self) -> String {
        format!(
            "{} · {} · {}",
            self.desktop.name(),
            self.source.describe(),
            if self.is_dark { "dark" } else { "light" }
        )
    }

    /// Cheap equality on the colors that matter, used to skip needless restyles on live refresh.
    pub fn same_colors(&self, o: &Self) -> bool {
        self.window_bg == o.window_bg
            && self.view_bg == o.view_bg
            && self.focus_blue == o.focus_blue
            && self.selection_bg == o.selection_bg
            && self.text_normal == o.text_normal
            && self.header_bg == o.header_bg
            && self.button_bg == o.button_bg
    }
}

/// Applies the KDE Breeze Dark visual style to `egui::Context`.
pub fn configure_kde_breeze_style(ctx: &egui::Context, pal: &KdeBreezePalette) {
    let mut style = (*ctx.style()).clone();
    let mut visuals = if pal.is_dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };

    visuals.panel_fill = pal.window_bg;
    visuals.window_fill = pal.window_bg;
    visuals.faint_bg_color = pal.view_alt_bg;
    visuals.extreme_bg_color = pal.view_bg;
    visuals.code_bg_color = pal.view_bg;
    visuals.override_text_color = Some(pal.text_normal);

    // Selected items (ComboBox, selectable_label) use KDE Selection Blue with bright white text
    visuals.selection.bg_fill = pal.selected_fill;
    visuals.selection.stroke = Stroke::new(1.5_f32, pal.on_selected);

    // KDE Breeze subtle 4px rounded corners and thin 1px borders
    visuals.widgets.noninteractive.bg_fill = pal.view_alt_bg;
    visuals.widgets.noninteractive.weak_bg_fill = pal.view_alt_bg;
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, pal.border_subtle);
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, pal.text_normal);
    visuals.widgets.noninteractive.corner_radius = CornerRadius::same(4);

    visuals.widgets.inactive.bg_fill = pal.button_bg;
    visuals.widgets.inactive.weak_bg_fill = pal.button_bg;
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, pal.border_subtle);
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.2_f32, pal.text_normal);
    visuals.widgets.inactive.corner_radius = CornerRadius::same(4);

    visuals.widgets.hovered.bg_fill = pal.hover_bg;
    visuals.widgets.hovered.weak_bg_fill = pal.hover_bg;
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.2_f32, pal.focus_blue);
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.5_f32, pal.text_normal);
    visuals.widgets.hovered.corner_radius = CornerRadius::same(4);

    visuals.widgets.active.bg_fill = pal.selected_fill;
    visuals.widgets.active.weak_bg_fill = pal.selected_fill;
    visuals.widgets.active.bg_stroke = Stroke::new(1.5_f32, pal.focus_blue);
    visuals.widgets.active.fg_stroke = Stroke::new(1.8_f32, pal.on_selected);
    visuals.widgets.active.corner_radius = CornerRadius::same(4);

    visuals.widgets.open.bg_fill = pal.hover_bg;
    visuals.widgets.open.weak_bg_fill = pal.hover_bg;
    visuals.widgets.open.bg_stroke = Stroke::new(1.2_f32, pal.focus_blue);
    visuals.widgets.open.fg_stroke = Stroke::new(1.5_f32, pal.text_normal);
    visuals.widgets.open.corner_radius = CornerRadius::same(4);

    style.visuals = visuals;
    style.spacing.item_spacing = Vec2::new(8.0, 7.0);
    style.spacing.button_padding = Vec2::new(10.0, 5.0);

    style.text_styles.insert(
        TextStyle::Heading,
        FontId::new(16.5, FontFamily::Proportional),
    );
    style
        .text_styles
        .insert(TextStyle::Body, FontId::new(13.5, FontFamily::Proportional));
    style.text_styles.insert(
        TextStyle::Button,
        FontId::new(13.0, FontFamily::Proportional),
    );
    style.text_styles.insert(
        TextStyle::Small,
        FontId::new(11.5, FontFamily::Proportional),
    );
    style.text_styles.insert(
        TextStyle::Monospace,
        FontId::new(12.5, FontFamily::Monospace),
    );

    ctx.set_style(style);
}

/// Draws a KDE Plasma System Monitor `org.kde.ksysguard.piechart` style smooth Donut Ring widget.
/// Features a full 360-degree background track, smooth colored progress arc starting from 12 o'clock (-PI/2),
/// high-contrast centered primary value, and clean bottom label + subtitle.
pub fn draw_kde_donut_chart(
    ui: &mut Ui,
    title: &str,
    center_value: &str,
    subtitle: &str,
    fraction: f32,
    ring_color: Color32,
    pal: &KdeBreezePalette,
) -> egui::Response {
    draw_kde_donut_chart_sized(
        ui,
        title,
        center_value,
        subtitle,
        fraction,
        ring_color,
        pal,
        false,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn draw_kde_donut_chart_sized(
    ui: &mut Ui,
    title: &str,
    center_value: &str,
    subtitle: &str,
    fraction: f32,
    ring_color: Color32,
    pal: &KdeBreezePalette,
    compact: bool,
) -> egui::Response {
    let height = if compact { 122.0 } else { 156.0 };
    let desired = Vec2::new(ui.available_width().max(115.0), height);
    let (rect, response) = ui.allocate_exact_size(desired, Sense::hover());
    let painter = ui.painter_at(rect);

    let ring_center_y = if compact { 43.0 } else { 58.0 };
    let ring_center = Pos2::new(rect.center().x, rect.top() + ring_center_y);
    let max_r = if compact { 33.0_f32 } else { 44.0_f32 };
    let radius = max_r.min((rect.width() * 0.34).max(26.0));
    let stroke_w = if compact { 7.0_f32 } else { 8.5_f32 };

    // 1. Full 360° background ring track
    painter.circle_stroke(ring_center, radius, Stroke::new(stroke_w, pal.track));

    // 2. Smooth progress arc starting from top (-PI/2) clockwise
    let clamped = fraction.clamp(0.0, 1.0);
    if clamped > 0.005 {
        let start_angle = -PI * 0.5;
        let end_angle = start_angle + clamped * 2.0 * PI;
        let steps = ((clamped * 48.0) as usize).max(10);
        let pts: Vec<Pos2> = (0..=steps)
            .map(|i| {
                let t = i as f32 / steps as f32;
                let a = start_angle + t * (end_angle - start_angle);
                Pos2::new(
                    ring_center.x + radius * a.cos(),
                    ring_center.y + radius * a.sin(),
                )
            })
            .collect();
        painter.add(Shape::line(pts, Stroke::new(stroke_w, ring_color)));
    }

    // 3. Center primary metric inside the donut
    painter.text(
        ring_center,
        Align2::CENTER_CENTER,
        center_value,
        FontId::new(if compact { 13.5 } else { 15.5 }, FontFamily::Proportional),
        pal.text_normal,
    );

    // 4. Title & Subtitle below the ring (matching plasma-systemmonitor overview.page)
    let title_off = if compact { 14.0 } else { 18.0 };
    let sub_off = if compact { 28.0 } else { 35.0 };
    painter.text(
        Pos2::new(rect.center().x, ring_center.y + radius + title_off),
        Align2::CENTER_CENTER,
        title,
        FontId::new(if compact { 12.0 } else { 13.0 }, FontFamily::Proportional),
        pal.text_normal,
    );
    painter.text(
        Pos2::new(rect.center().x, ring_center.y + radius + sub_off),
        Align2::CENTER_CENTER,
        subtitle,
        FontId::new(11.0, FontFamily::Proportional),
        pal.text_inactive,
    );

    response
}

/// Draws a KDE Plasma System Monitor `org.kde.ksysguard.linechart` style chart
/// with translucent area fill under each series (`history.page` style).
pub fn draw_kde_area_line_chart(
    ui: &mut Ui,
    height: f32,
    series: &[(&VecDeque<f32>, Color32, f32)], // (history, color, max_val)
    pal: &KdeBreezePalette,
) {
    let desired = Vec2::new(ui.available_width().max(180.0), height);
    let (rect, _) = ui.allocate_exact_size(desired, Sense::hover());
    let painter = ui.painter_at(rect);

    // View background
    painter.rect_filled(rect, CornerRadius::same(4), pal.view_bg);
    painter.rect_stroke(
        rect,
        CornerRadius::same(4),
        Stroke::new(1.0_f32, pal.border_subtle),
        StrokeKind::Inside,
    );

    let plot = rect.shrink2(Vec2::new(8.0, 8.0));

    // Subtle horizontal & vertical grid lines
    for i in 1..4 {
        let y = plot.top() + plot.height() * (i as f32 / 4.0);
        painter.line_segment(
            [Pos2::new(plot.left(), y), Pos2::new(plot.right(), y)],
            Stroke::new(1.0_f32, pal.grid),
        );
    }
    for i in 1..6 {
        let x = plot.left() + plot.width() * (i as f32 / 6.0);
        painter.line_segment(
            [Pos2::new(x, plot.top()), Pos2::new(x, plot.bottom())],
            Stroke::new(1.0_f32, pal.grid),
        );
    }

    // Draw each series with translucent vertical strips / quads for smooth area fill + top stroke
    for (data, color, max_val) in series {
        if data.len() < 2 {
            continue;
        }
        let n = data.len();
        let pts: Vec<Pos2> = data
            .iter()
            .enumerate()
            .map(|(idx, &val)| {
                let x = plot.left() + (idx as f32 / (n.saturating_sub(1) as f32)) * plot.width();
                let norm = (val / *max_val).clamp(0.0, 1.0);
                let y = plot.bottom() - norm * plot.height();
                Pos2::new(x, y)
            })
            .collect();

        // Translucent fill below the line using vertical trapezoids/lines for convexity safety
        let fill_col = Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 38);
        for pair in pts.windows(2) {
            let p0 = pair[0];
            let p1 = pair[1];
            let poly = vec![
                p0,
                p1,
                Pos2::new(p1.x, plot.bottom()),
                Pos2::new(p0.x, plot.bottom()),
            ];
            painter.add(Shape::convex_polygon(poly, fill_col, Stroke::NONE));
        }

        // Crisp 2px top curve line
        painter.add(Shape::line(pts, Stroke::new(2.0_f32, *color)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn light_palette_accents_are_readable() {
        let c = crate::system_colors::SystemColors {
            window_bg: Some(Color32::from_rgb(250, 250, 250)),
            view_bg: Some(Color32::WHITE),
            accent: Some(Color32::from_rgb(30, 230, 255)),
            ..Default::default()
        };
        let pal =
            KdeBreezePalette::from_system_colors(&c, PaletteSource::GtkTheme, DesktopKind::Gnome);
        let ui = UiColors::from_palette(&pal);
        for col in [ui.cyan, ui.green, ui.red, ui.orange, ui.amber] {
            assert!(contrast_ratio(col, pal.window_bg) >= 3.0, "{col:?}");
        }
        assert!(contrast_ratio(ui.text_secondary, pal.view_alt_bg) >= 4.5);
        assert!(contrast_ratio(ui.btn_cyan, Color32::WHITE) >= 4.5);
    }

    #[test]
    fn test_parse_kdeglobals_palette() {
        let sample = "[Colors:Window]\nBackgroundNormal=32,35,38\n[Colors:View]\nBackgroundNormal=20,22,24\nDecorationFocus=61,174,233\n";
        let mut pal = KdeBreezePalette::default();
        pal.apply_ini_str(sample);
        assert_eq!(pal.window_bg, Color32::from_rgb(32, 35, 38));
        assert_eq!(pal.view_bg, Color32::from_rgb(20, 22, 24));
        assert_eq!(pal.focus_blue, Color32::from_rgb(61, 174, 233));
    }
}

/// Semantic colors used by the shared tabs (Smart Fan, Overclock, Features, GPIO, Services…).
/// AORUS keeps its neon look; the KDE/Breeze themes derive everything from the desktop palette,
/// with accents darkened/lightened until they stay readable on the window background.
#[derive(Debug, Clone, Copy)]
pub struct UiColors {
    pub bg_dark: Color32,
    pub bg_panel: Color32,
    pub bg_card: Color32,
    pub bg_subgroup: Color32,
    pub bg_card_hover: Color32,
    pub border_subtle: Color32,
    pub orange: Color32,
    pub amber: Color32,
    pub cyan: Color32,
    pub green: Color32,
    pub red: Color32,
    pub btn_orange: Color32,
    pub btn_green: Color32,
    pub btn_cyan: Color32,
    pub btn_slate: Color32,
    pub text_primary: Color32,
    pub text_secondary: Color32,
}

impl UiColors {
    pub const AORUS: UiColors = UiColors {
        bg_dark: Color32::from_rgb(10, 14, 20),
        bg_panel: Color32::from_rgb(16, 22, 33),
        bg_card: Color32::from_rgb(22, 30, 46),
        bg_subgroup: Color32::from_rgb(15, 21, 32),
        bg_card_hover: Color32::from_rgb(38, 52, 78),
        border_subtle: Color32::from_rgb(56, 72, 102),
        orange: Color32::from_rgb(255, 115, 20),
        amber: Color32::from_rgb(255, 185, 30),
        cyan: Color32::from_rgb(30, 230, 255),
        green: Color32::from_rgb(35, 235, 135),
        red: Color32::from_rgb(255, 75, 100),
        btn_orange: Color32::from_rgb(195, 65, 10),
        btn_green: Color32::from_rgb(12, 118, 68),
        btn_cyan: Color32::from_rgb(12, 102, 142),
        btn_slate: Color32::from_rgb(30, 41, 62),
        text_primary: Color32::from_rgb(248, 250, 252),
        text_secondary: Color32::from_rgb(203, 213, 225),
    };

    pub fn from_palette(p: &KdeBreezePalette) -> Self {
        let bg = p.window_bg;
        let card = p.view_alt_bg;
        // Accent used as text/strokes: >= 3:1 against both window and card surfaces.
        let accent = |c: Color32| readable_on(readable_on(c, bg, 3.0), card, 3.0);
        // Button fill carrying white text: >= 4.5:1 against white.
        let fill = |c: Color32| {
            let mut f = c;
            for _ in 0..12 {
                if contrast_ratio(f, Color32::WHITE) >= 4.5 {
                    break;
                }
                f = mix(f, Color32::BLACK, 0.12);
            }
            f
        };
        let amber_base = mix(p.neutral_orange, Color32::from_rgb(240, 190, 30), 0.55);
        let text_secondary = readable_on(p.text_inactive, card, 4.5);
        UiColors {
            bg_dark: bg,
            bg_panel: p.header_bg,
            bg_card: card,
            bg_subgroup: p.view_bg,
            bg_card_hover: p.hover_bg,
            border_subtle: p.border_subtle,
            orange: accent(p.neutral_orange),
            amber: accent(amber_base),
            cyan: accent(p.focus_blue),
            green: accent(p.positive_green),
            red: accent(p.negative_red),
            btn_orange: fill(p.neutral_orange),
            btn_green: fill(p.positive_green),
            btn_cyan: fill(p.focus_blue),
            btn_slate: p.button_bg,
            text_primary: readable_on(p.text_normal, card, 4.5),
            text_secondary,
        }
    }
}

/// Moves `c` towards black (light bg) or white (dark bg) until it reaches `min_ratio` on `bg`.
pub fn readable_on(c: Color32, bg: Color32, min_ratio: f32) -> Color32 {
    let target = if luminance(bg) > 0.4 {
        Color32::BLACK
    } else {
        Color32::WHITE
    };
    let mut out = c;
    for _ in 0..16 {
        if contrast_ratio(out, bg) >= min_ratio {
            break;
        }
        out = mix(out, target, 0.12);
    }
    out
}

thread_local! {
    static UI_COLORS: std::cell::Cell<UiColors> = const { std::cell::Cell::new(UiColors::AORUS) };
}

/// Active semantic colors for this frame (the UI runs on a single thread).
pub fn uc() -> UiColors {
    UI_COLORS.with(|c| c.get())
}

pub fn set_ui_colors(colors: UiColors) {
    UI_COLORS.with(|c| c.set(colors));
}
