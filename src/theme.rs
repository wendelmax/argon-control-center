use egui::{
    Align2, Color32, CornerRadius, FontFamily, FontId, Pos2, Sense, Shape, Stroke, StrokeKind,
    TextStyle, Ui, Vec2,
};
use std::collections::VecDeque;
use std::f32::consts::PI;
use std::fs;

/// Available UI Theme & Layout Models.
/// - `AorusGaming`: Saved v1.0 Gigabyte AORUS Gaming theme (top double bar, automotive radial gauges, neon orange/cyan accents).
/// - `KdeBreeze`: KDE Plasma System Monitor (`plasma-systemmonitor`) theme & layout (Kirigami left sidebar, contextual header bar, unified donut ring strip, translucent area line charts, and live `~/.config/kdeglobals` Breeze Dark palette).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeModel {
    KdeBreeze,
    AorusGaming,
}

/// Color palette loaded from `/home/user/.config/kdeglobals` (or `/usr/share/color-schemes/BreezeDark.colors` fallback).
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
}

impl Default for KdeBreezePalette {
    fn default() -> Self {
        Self {
            window_bg: Color32::from_rgb(32, 35, 38),       // #202326
            window_alt_bg: Color32::from_rgb(41, 44, 48),   // #292C30
            view_bg: Color32::from_rgb(20, 22, 24),         // #141618
            view_alt_bg: Color32::from_rgb(29, 31, 34),     // #1D1F22
            header_bg: Color32::from_rgb(41, 44, 48),       // #292C30
            button_bg: Color32::from_rgb(41, 44, 48),       // #292C30
            selection_bg: Color32::from_rgb(61, 174, 233),  // #3DAEE9
            focus_blue: Color32::from_rgb(61, 174, 233),    // #3DAEE9
            text_normal: Color32::from_rgb(252, 252, 252),  // #FCFCFC
            text_inactive: Color32::from_rgb(161, 169, 177),// #A1A9B1
            positive_green: Color32::from_rgb(39, 174, 96), // #27AE60
            neutral_orange: Color32::from_rgb(246, 116, 0), // #F67400
            negative_red: Color32::from_rgb(218, 68, 83),   // #DA4453
            visited_purple: Color32::from_rgb(155, 89, 182),// #9B59B6
            border_subtle: Color32::from_rgb(58, 63, 68),   // Subtle 1px frameContrast=0.2
        }
    }
}

impl KdeBreezePalette {
    /// Reads live KDE color scheme from `~/.config/kdeglobals` or `/usr/share/color-schemes/BreezeDark.colors`.
    pub fn load_from_system() -> Self {
        let mut pal = Self::default();
        let home = std::env::var("HOME").unwrap_or_else(|_| "/home/user".to_string());
        let kdeglobals_path = format!("{home}/.config/kdeglobals");
        let content = fs::read_to_string(&kdeglobals_path)
            .or_else(|_| fs::read_to_string("/usr/share/color-schemes/BreezeDark.colors"))
            .unwrap_or_default();

        if !content.is_empty() {
            pal.apply_ini_str(&content);
        }
        pal
    }

    pub fn apply_ini_str(&mut self, content: &str) {
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
            if let Some((k, v)) = line.split_once('=') {
                if let Some(col) = parse_rgb_triplet(v.trim()) {
                    match (section.as_str(), k.trim()) {
                        ("Colors:Window", "BackgroundNormal") => self.window_bg = col,
                        ("Colors:Window", "BackgroundAlternate") => self.window_alt_bg = col,
                        ("Colors:View", "BackgroundNormal") => self.view_bg = col,
                        ("Colors:View", "BackgroundAlternate") => self.view_alt_bg = col,
                        ("Colors:Header", "BackgroundNormal") => self.header_bg = col,
                        ("Colors:Button", "BackgroundNormal") => self.button_bg = col,
                        ("Colors:Selection", "BackgroundNormal") => self.selection_bg = col,
                        ("Colors:View", "DecorationFocus") => self.focus_blue = col,
                        ("Colors:View", "ForegroundNormal") => self.text_normal = col,
                        ("Colors:View", "ForegroundInactive") => self.text_inactive = col,
                        ("Colors:View", "ForegroundPositive") => self.positive_green = col,
                        ("Colors:View", "ForegroundNeutral") => self.neutral_orange = col,
                        ("Colors:View", "ForegroundNegative") => self.negative_red = col,
                        ("Colors:View", "ForegroundVisited") => self.visited_purple = col,
                        _ => {}
                    }
                }
            }
        }
    }
}

fn parse_rgb_triplet(s: &str) -> Option<Color32> {
    let mut parts = s.split(',');
    let r: u8 = parts.next()?.trim().parse().ok()?;
    let g: u8 = parts.next()?.trim().parse().ok()?;
    let b: u8 = parts.next()?.trim().parse().ok()?;
    Some(Color32::from_rgb(r, g, b))
}

/// Applies the KDE Breeze Dark visual style to `egui::Context`.
pub fn configure_kde_breeze_style(ctx: &egui::Context, pal: &KdeBreezePalette) {
    let mut style = (*ctx.style()).clone();
    let mut visuals = egui::Visuals::dark();

    visuals.panel_fill = pal.window_bg;
    visuals.window_fill = pal.window_bg;
    visuals.faint_bg_color = pal.view_alt_bg;
    visuals.extreme_bg_color = pal.view_bg;
    visuals.code_bg_color = pal.view_bg;
    visuals.override_text_color = Some(pal.text_normal);

    // Selected items (ComboBox, selectable_label) use KDE Selection Blue with bright white text
    visuals.selection.bg_fill = Color32::from_rgb(28, 108, 156);
    visuals.selection.stroke = Stroke::new(1.5_f32, Color32::WHITE);

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

    visuals.widgets.hovered.bg_fill = Color32::from_rgb(50, 56, 62);
    visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(50, 56, 62);
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.2_f32, pal.focus_blue);
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.5_f32, Color32::WHITE);
    visuals.widgets.hovered.corner_radius = CornerRadius::same(4);

    visuals.widgets.active.bg_fill = Color32::from_rgb(28, 108, 156);
    visuals.widgets.active.weak_bg_fill = Color32::from_rgb(28, 108, 156);
    visuals.widgets.active.bg_stroke = Stroke::new(1.5_f32, pal.focus_blue);
    visuals.widgets.active.fg_stroke = Stroke::new(1.8_f32, Color32::WHITE);
    visuals.widgets.active.corner_radius = CornerRadius::same(4);

    visuals.widgets.open.bg_fill = Color32::from_rgb(46, 52, 58);
    visuals.widgets.open.weak_bg_fill = Color32::from_rgb(46, 52, 58);
    visuals.widgets.open.bg_stroke = Stroke::new(1.2_f32, pal.focus_blue);
    visuals.widgets.open.fg_stroke = Stroke::new(1.5_f32, Color32::WHITE);
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
    let desired = Vec2::new(ui.available_width().max(130.0), 156.0);
    let (rect, response) = ui.allocate_exact_size(desired, Sense::hover());
    let painter = ui.painter_at(rect);

    let ring_center = Pos2::new(rect.center().x, rect.top() + 58.0);
    let radius = 44.0_f32.min((rect.width() * 0.36).max(32.0));
    let stroke_w = 8.5_f32;

    // 1. Full 360° background ring track
    painter.circle_stroke(
        ring_center,
        radius,
        Stroke::new(stroke_w, Color32::from_rgb(42, 46, 52)),
    );

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
                Pos2::new(ring_center.x + radius * a.cos(), ring_center.y + radius * a.sin())
            })
            .collect();
        painter.add(Shape::line(pts, Stroke::new(stroke_w, ring_color)));
    }

    // 3. Center primary metric inside the donut
    painter.text(
        ring_center,
        Align2::CENTER_CENTER,
        center_value,
        FontId::new(15.5, FontFamily::Proportional),
        pal.text_normal,
    );

    // 4. Title & Subtitle below the ring (matching plasma-systemmonitor overview.page)
    painter.text(
        Pos2::new(rect.center().x, ring_center.y + radius + 18.0),
        Align2::CENTER_CENTER,
        title,
        FontId::new(13.0, FontFamily::Proportional),
        pal.text_normal,
    );
    painter.text(
        Pos2::new(rect.center().x, ring_center.y + radius + 35.0),
        Align2::CENTER_CENTER,
        subtitle,
        FontId::new(11.5, FontFamily::Proportional),
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
            Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(255, 255, 255, 14)),
        );
    }
    for i in 1..6 {
        let x = plot.left() + plot.width() * (i as f32 / 6.0);
        painter.line_segment(
            [Pos2::new(x, plot.top()), Pos2::new(x, plot.bottom())],
            Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(255, 255, 255, 10)),
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
    fn test_parse_kdeglobals_palette() {
        let sample = "[Colors:Window]\nBackgroundNormal=32,35,38\n[Colors:View]\nBackgroundNormal=20,22,24\nDecorationFocus=61,174,233\n";
        let mut pal = KdeBreezePalette::default();
        pal.apply_ini_str(sample);
        assert_eq!(pal.window_bg, Color32::from_rgb(32, 35, 38));
        assert_eq!(pal.view_bg, Color32::from_rgb(20, 22, 24));
        assert_eq!(pal.focus_blue, Color32::from_rgb(61, 174, 233));
    }
}
