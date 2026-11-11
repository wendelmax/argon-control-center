//! GPIO Lab page, laid out as a workbench: a status toolbar, the pinout and the
//! inspector of the selected pin side by side, and a bottom panel with tabs for the
//! logic analyzer, the serial port, the I2C bus and the log.

use super::*;
use crate::gpio_lab::{
    format_console_entry_payload, ConsoleEntry, DataFormat, GpioSerialLabState, HeaderPinKind,
    LineEnding, LogSource, PhysicalPinDef, PinDirection, PinPull, RPI_40PIN_HEADER,
};
use egui::text::{LayoutJob, TextFormat};
use std::path::Path;

/// Tabs of the bottom panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(super) enum GpioPanel {
    #[default]
    Analyzer,
    Serial,
    I2c,
    Log,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Health {
    Ok,
    Warn,
    Off,
}

impl Health {
    fn color(self) -> Color32 {
        match self {
            Health::Ok => uc().green,
            Health::Warn => uc().amber,
            Health::Off => uc().text_secondary,
        }
    }
}

/// Pin role in the header diagram, each with its own color.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PinCategory {
    Power3v3,
    Power5v,
    Ground,
    Gpio,
    I2c,
    Uart,
    Spi,
    Pcm,
    Pwm,
    Argon,
    Eeprom,
}

/// Opacity of a role color behind a label.
const LABEL_TINT: f32 = 0.18;

impl PinCategory {
    const ALL: [PinCategory; 11] = [
        PinCategory::Power3v3,
        PinCategory::Power5v,
        PinCategory::Ground,
        PinCategory::Gpio,
        PinCategory::I2c,
        PinCategory::Uart,
        PinCategory::Spi,
        PinCategory::Pcm,
        PinCategory::Pwm,
        PinCategory::Argon,
        PinCategory::Eeprom,
    ];

    fn of(def: &PhysicalPinDef) -> Self {
        match def.kind {
            HeaderPinKind::Power3v3 => Self::Power3v3,
            HeaderPinKind::Power5v => Self::Power5v,
            HeaderPinKind::Ground => Self::Ground,
            HeaderPinKind::IdEeprom => Self::Eeprom,
            HeaderPinKind::Gpio(_) => {
                let f = pin_function(def);
                if f.contains("I2C") {
                    Self::I2c
                } else if f.contains("UART") {
                    Self::Uart
                } else if f.contains("SPI") {
                    Self::Spi
                } else if f.contains("PCM") {
                    Self::Pcm
                } else if f.contains("PWM") {
                    Self::Pwm
                } else if f.contains("ARGON") {
                    Self::Argon
                } else {
                    Self::Gpio
                }
            }
        }
    }

    fn color(self) -> Color32 {
        let hex = match self {
            Self::Power3v3 => 0xe8a33d,
            Self::Power5v => 0xd9534f,
            Self::Ground => 0x8a9099,
            Self::Gpio => 0x7d8fa3,
            Self::I2c => 0x4a90d9,
            Self::Uart => 0x3fae6a,
            Self::Spi => 0x9b6fd1,
            Self::Pcm => 0xd45d9a,
            Self::Pwm => 0xc9a227,
            Self::Argon => 0xe2683c,
            Self::Eeprom => 0x2aa198,
        };
        Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
    }

    fn name(self) -> &'static str {
        match self {
            Self::Power3v3 => "3.3V",
            Self::Power5v => "5V",
            Self::Ground => "GND",
            Self::Gpio => "GPIO",
            Self::I2c => "I2C",
            Self::Uart => "UART",
            Self::Spi => "SPI",
            Self::Pcm => "PCM",
            Self::Pwm => "PWM",
            Self::Argon => "Argon",
            Self::Eeprom => "ID EEPROM",
        }
    }
}

const I2C_DEV: &str = "/dev/i2c-1";
const ANALYZER_MAX_CHANNELS: usize = 4;
const OUTPUT_HEIGHT: f32 = 210.0;

fn i2c_health() -> Health {
    if !Path::new(I2C_DEV).exists() {
        Health::Off
    } else if crate::doctor::rw(I2C_DEV) {
        Health::Ok
    } else {
        Health::Warn
    }
}

fn trace_colors() -> [Color32; ANALYZER_MAX_CHANNELS] {
    [uc().green, uc().cyan, uc().orange, uc().amber]
}

/// Physical header pin of a BCM GPIO number.
fn physical_pin(bcm: u8) -> Option<&'static PhysicalPinDef> {
    RPI_40PIN_HEADER
        .iter()
        .find(|d| d.kind == HeaderPinKind::Gpio(bcm))
}

/// Alternate-function name from the header label, e.g. "SDA1 I2C" in "GPIO 2 (SDA1 I2C)".
fn pin_function(def: &PhysicalPinDef) -> &'static str {
    let label = def.label;
    match (label.find('('), label.rfind(')')) {
        (Some(a), Some(b)) if b > a => &label[a + 1..b],
        _ => "",
    }
}

/// Search rules: `17` or `gpio17` is a GPIO (BCM), `p11` a physical pin, anything else
/// matches the pin label (SDA, TX, GND...).
fn pin_matches(query: &str, def: &PhysicalPinDef) -> bool {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return true;
    }
    let gpio_num = q
        .strip_prefix("gpio")
        .unwrap_or(&q)
        .trim()
        .parse::<u8>()
        .ok();
    if let Some(n) = gpio_num {
        return def.kind == HeaderPinKind::Gpio(n);
    }
    if let Some(n) = q
        .strip_prefix('p')
        .and_then(|r| r.trim().parse::<u8>().ok())
    {
        return def.phys_num == n;
    }
    def.label.to_lowercase().contains(&q)
}

fn first_gpio_match(query: &str) -> Option<u8> {
    RPI_40PIN_HEADER
        .iter()
        .filter(|d| pin_matches(query, d))
        .find_map(|d| match d.kind {
            HeaderPinKind::Gpio(bcm) => Some(bcm),
            _ => None,
        })
}

fn pull_label(pull: PinPull) -> &'static str {
    match pull {
        PinPull::Up => "up",
        PinPull::Down => "down",
        PinPull::None => "none",
    }
}

fn mode_color(dir: PinDirection) -> Color32 {
    match dir {
        PinDirection::Output => uc().orange,
        PinDirection::Input => uc().cyan,
        _ => uc().text_secondary,
    }
}

/// Shell and Python commands equivalent to the current state of a pin.
fn equivalent_commands(bcm: u8, dir: PinDirection, pull: PinPull, high: bool) -> Vec<String> {
    let mut cmds = vec![format!("pinctrl get {bcm}"), format!("gpioget GPIO{bcm}")];
    let pull_arg = match pull {
        PinPull::Up => "pu",
        PinPull::Down => "pd",
        PinPull::None => "pn",
    };
    match dir {
        PinDirection::Output => {
            let (lvl, val) = if high { ("dh", 1) } else { ("dl", 0) };
            cmds.push(format!("pinctrl set {bcm} op {lvl}"));
            cmds.push(format!("gpioset -t0 GPIO{bcm}={val}"));
            cmds.push(format!(
                "python3 -c \"from gpiozero import LED; LED({bcm}).{}()\"",
                if high { "on" } else { "off" }
            ));
        }
        _ => {
            let py_pull = match pull {
                PinPull::Up => "pull_up=True".to_string(),
                PinPull::Down => "pull_up=False".to_string(),
                PinPull::None => "pull_up=None, active_state=True".to_string(),
            };
            cmds.push(format!("pinctrl set {bcm} ip {pull_arg}"));
            cmds.push(format!(
                "python3 -c \"from gpiozero import DigitalInputDevice as D; print(D({bcm}, {py_pull}).value)\""
            ));
        }
    }
    cmds
}

/// How much of a pin label the diagram shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LabelDetail {
    /// GPIO2 · SDA1 I2C
    Full,
    /// GPIO2 · SDA1
    Short,
    /// GPIO2
    Number,
}

fn pin_label(def: &PhysicalPinDef, detail: LabelDetail) -> String {
    match (def.kind, pin_function(def)) {
        (HeaderPinKind::Gpio(bcm), "") => format!("GPIO{bcm}"),
        (HeaderPinKind::Gpio(bcm), f) => match detail {
            LabelDetail::Full => format!("GPIO{bcm} · {f}"),
            LabelDetail::Short => format!("GPIO{bcm} · {}", f.split(' ').next().unwrap_or(f)),
            LabelDetail::Number => format!("GPIO{bcm}"),
        },
        (HeaderPinKind::Power3v3, _) => "3.3V".to_string(),
        (HeaderPinKind::Power5v, _) => "5V".to_string(),
        (HeaderPinKind::Ground, _) => "GND".to_string(),
        (HeaderPinKind::IdEeprom, _) => def.label.split(' ').next().unwrap_or("ID").to_string(),
    }
}

/// Label size and detail that fit the width available beside the pin columns.
struct DiagramFit {
    font_size: f32,
    label: LabelDetail,
    show_mode: bool,
}

impl DiagramFit {
    fn choose(ui: &Ui, width: f32, row_h: f32) -> Self {
        let radius = (row_h - 6.0) / 2.0;
        // Width on each side: half the diagram minus half a pin pitch, the circle and the gap.
        let side = width / 2.0 - row_h / 2.0 - (radius + 10.0) - 4.0;
        let mode_w = ui.fonts(|f| {
            f.layout_no_wrap("ALT0".into(), FontId::monospace(10.5), Color32::WHITE)
                .size()
                .x
        }) + 6.0;
        let candidates = [
            (12.5, LabelDetail::Full, true),
            (11.5, LabelDetail::Full, true),
            (11.5, LabelDetail::Full, false),
            (11.5, LabelDetail::Short, false),
            (11.0, LabelDetail::Number, false),
        ];
        for (font_size, label, show_mode) in candidates {
            let font = FontId::new(font_size, FontFamily::Proportional);
            let widest = ui.fonts(|f| {
                RPI_40PIN_HEADER
                    .iter()
                    .map(|d| {
                        f.layout_no_wrap(pin_label(d, label), font.clone(), Color32::WHITE)
                            .size()
                            .x
                    })
                    .fold(0.0_f32, f32::max)
            }) + 18.0;
            let needed = widest + if show_mode { mode_w } else { 0.0 };
            if needed <= side {
                return Self {
                    font_size,
                    label,
                    show_mode,
                };
            }
        }
        Self {
            font_size: 10.5,
            label: LabelDetail::Number,
            show_mode: false,
        }
    }
}

/// Color swatch (square, or circle for a level) followed by its name.
fn legend_item(ui: &mut Ui, name: &str, color: Color32, round: bool) {
    let font = FontId::new(11.5, FontFamily::Proportional);
    let galley = ui.fonts(|f| f.layout_no_wrap(name.to_string(), font, uc().text_secondary));
    let swatch = 12.0;
    let size = Vec2::new(swatch + 4.0 + galley.size().x, galley.size().y.max(swatch));
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let sw = Rect::from_center_size(
        Pos2::new(rect.left() + swatch / 2.0, rect.center().y),
        Vec2::splat(swatch),
    );
    if round {
        ui.painter()
            .circle_filled(sw.center(), swatch / 2.0 - 0.5, color);
    } else {
        ui.painter().rect_filled(sw, CornerRadius::same(3), color);
    }
    ui.painter().galley(
        Pos2::new(sw.right() + 4.0, rect.center().y - galley.size().y / 2.0),
        galley,
        Color32::PLACEHOLDER,
    );
}

fn mono(text: impl Into<String>) -> RichText {
    RichText::new(text).monospace().size(12.0)
}

/// Small filled circle for a logic level.
fn level_dot(ui: &mut Ui, high: bool, diameter: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(diameter), Sense::hover());
    let r = diameter / 2.0 - 1.0;
    if high {
        ui.painter().circle_filled(rect.center(), r, uc().green);
    } else {
        ui.painter()
            .circle_stroke(rect.center(), r, Stroke::new(1.5_f32, uc().text_secondary));
    }
}

/// Rounded status pill: colored dot, name and an optional value.
fn status_chip(ui: &mut Ui, label: &str, value: &str, health: Health) -> Response {
    egui::Frame::new()
        .fill(uc().bg_subgroup)
        .stroke(Stroke::new(1.0_f32, uc().border_subtle))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(egui::Margin::symmetric(9, 3))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 5.0;
                let (r, _) = ui.allocate_exact_size(Vec2::splat(8.0), Sense::hover());
                ui.painter().circle_filled(r.center(), 4.0, health.color());
                ui.label(
                    RichText::new(label)
                        .size(12.0)
                        .strong()
                        .color(uc().text_primary),
                );
                if !value.is_empty() {
                    ui.label(RichText::new(value).size(12.0).color(uc().text_secondary));
                }
            });
        })
        .response
}

/// Section title inside a card.
fn sub_heading(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).size(12.0).strong().color(uc().heading));
}

/// Frame with the card look used by the bottom panel (tabs instead of a title).
fn panel_frame(ui: &mut Ui, add_contents: impl FnOnce(&mut Ui)) {
    egui::Frame::new()
        .fill(ui.visuals().faint_bg_color)
        .stroke(ui.visuals().widgets.noninteractive.bg_stroke)
        .corner_radius(ui.visuals().widgets.noninteractive.corner_radius)
        .inner_margin(egui::Margin::same(10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add_contents(ui);
        });
}

impl ArgonControlCenterApp {
    pub(super) fn draw_gpio_lab_tab(&mut self, ui: &mut Ui) {
        self.gpio_toolbar(ui);
        ui.add_space(8.0);
        responsive_two_columns(
            ui,
            self,
            |app, ui| app.gpio_pinout_card(ui),
            |app, ui| app.gpio_inspector_card(ui),
        );
        ui.add_space(10.0);
        self.gpio_bottom_panel(ui);
    }

    // ------------------------------------------------------------------ toolbar

    fn gpio_toolbar(&mut self, ui: &mut Ui) {
        let t = texts(self.lang);
        let g = t.gpio;
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;

            let gpio_ok = self.gpio_lab.hw_access_unlocked;
            let r = status_chip(
                ui,
                g.status_gpio,
                if gpio_ok { "" } else { g.status_no_access },
                if gpio_ok { Health::Ok } else { Health::Warn },
            );
            aorus_tooltip(
                r,
                g.status_gpio,
                if gpio_ok {
                    g.status_gpio_ok_tip
                } else {
                    g.status_gpio_off_tip
                },
                Some("pinctrl get 0-27"),
            );

            let i2c = i2c_health();
            let (value, tip) = match i2c {
                Health::Ok => ("", g.status_i2c_ok_tip),
                Health::Warn => (g.status_no_access, g.status_i2c_noperm_tip),
                Health::Off => (g.status_off, g.status_i2c_off_tip),
            };
            let r = status_chip(ui, g.status_i2c, value, i2c);
            aorus_tooltip(r, g.status_i2c, tip, Some(I2C_DEV));

            let port = if self.gpio_lab.serial_connected {
                self.gpio_lab
                    .selected_port
                    .rsplit('/')
                    .next()
                    .unwrap_or_default()
                    .to_string()
            } else {
                g.status_serial_closed.to_string()
            };
            let r = status_chip(
                ui,
                g.status_serial,
                &port,
                if self.gpio_lab.serial_connected {
                    Health::Ok
                } else {
                    Health::Off
                },
            );
            aorus_tooltip(
                r,
                g.status_serial,
                g.status_serial_tip,
                Some(&self.gpio_lab.selected_port),
            );

            if !gpio_ok || i2c == Health::Warn {
                let r = aorus_action_button(ui, t.btn_unlock_gpio, uc().btn_orange, uc().orange);
                if r.clicked() {
                    let res = self.gpio_lab.unlock_hardware_access();
                    self.log_result("Unlock hardware", res);
                }
                aorus_tooltip(
                    r,
                    t.btn_unlock_gpio,
                    t.btn_unlock_gpio_tip,
                    Some("/usr/lib/argon-control-center/setup-permissions.sh"),
                );
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let r = ui.checkbox(&mut self.gpio_lab.simulate_pulses, t.chk_sim_pulses);
                aorus_tooltip(r, t.chk_sim_pulses, t.chk_sim_pulses_tip, None);

                let search = ui.add(
                    egui::TextEdit::singleline(&mut self.gpio_query)
                        .hint_text(g.search_hint)
                        .desired_width(170.0),
                );
                let enter = search.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                if search.changed() || enter {
                    if let Some(bcm) = first_gpio_match(&self.gpio_query) {
                        self.gpio_lab.selected_bcm = bcm;
                    }
                }
                aorus_tooltip(search, g.search_hint, g.search_tip, None);
            });
        });
    }

    // ------------------------------------------------------------------ pinout

    fn gpio_pinout_card(&mut self, ui: &mut Ui) {
        let t = texts(self.lang);
        let g = t.gpio;
        aorus_card_with_tip(ui, g.pinout, g.pinout_tip, uc().orange, |ui| {
            ui.horizontal(|ui| {
                for (diagram, label) in [(true, t.gpio_view_diagram), (false, t.gpio_view_list)] {
                    if segmented_pill(
                        ui,
                        label,
                        self.gpio_view_diagram == diagram,
                        uc().btn_cyan,
                        uc().cyan,
                    )
                    .clicked()
                    {
                        self.gpio_view_diagram = diagram;
                    }
                }
            });
            ui.add_space(6.0);
            if self.gpio_view_diagram {
                self.draw_pinout_diagram(ui);
            } else {
                self.draw_pin_table(ui);
            }
        });
    }

    /// Draws the 40-pin header natively: the two pin columns in the middle, a pastel label per
    /// pin colored by function, live levels in the pin circles and the mode next to each label.
    fn draw_pinout_diagram(&mut self, ui: &mut Ui) {
        let t = texts(self.lang);
        let width = ui.available_width();
        let row_h = 26.0_f32;
        let top_pad = 6.0;
        let height = 20.0 * row_h + top_pad * 2.0;
        let (rect, _) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
        let painter = ui.painter_at(rect);

        let center_x = rect.center().x;
        let pitch = row_h;
        let radius = (row_h - 6.0) / 2.0;
        let fit = DiagramFit::choose(ui, width, row_h);
        let label_font = FontId::new(fit.font_size, FontFamily::Proportional);
        let number_font = FontId::new(11.0, FontFamily::Proportional);
        let mode_font = FontId::monospace(10.5);

        // Header body behind the pins.
        let body = Rect::from_min_max(
            Pos2::new(center_x - pitch - 6.0, rect.top() + 1.0),
            Pos2::new(center_x + pitch + 6.0, rect.bottom() - 1.0),
        );
        painter.rect_filled(body, CornerRadius::same(8), uc().bg_subgroup);
        painter.rect_stroke(
            body,
            CornerRadius::same(8),
            Stroke::new(1.0_f32, uc().border_subtle),
            StrokeKind::Inside,
        );

        let searching = !self.gpio_query.trim().is_empty();
        let selected = self.gpio_lab.selected_bcm;
        let mut clicked: Option<u8> = None;
        let mut double: Option<u8> = None;

        for (i, def) in RPI_40PIN_HEADER.iter().enumerate() {
            let row = (i / 2) as f32;
            let left = i % 2 == 0;
            let cy = rect.top() + top_pad + row * row_h + row_h / 2.0;
            let cx = if left {
                center_x - pitch / 2.0
            } else {
                center_x + pitch / 2.0
            };
            let category = PinCategory::of(def);
            let dim = searching && !pin_matches(&self.gpio_query, def);
            let fade = |c: Color32| if dim { c.gamma_multiply(0.3) } else { c };

            let gpio = match def.kind {
                HeaderPinKind::Gpio(bcm) => self
                    .gpio_lab
                    .pins
                    .get(bcm as usize)
                    .map(|p| (bcm, p.level_high, p.direction)),
                _ => None,
            };

            // Pin circle: category color for power and ground, live level for GPIO.
            let center = Pos2::new(cx, cy);
            let filled = match gpio {
                Some((_, true, _)) => Some(uc().green),
                Some((_, false, _)) => None,
                None => Some(category.color()),
            };
            let number_color = match filled {
                Some(fill) => {
                    painter.circle_filled(center, radius, fade(fill));
                    crate::system_colors::on_color(fill)
                }
                None => {
                    painter.circle_filled(center, radius, fade(uc().bg_card));
                    painter.circle_stroke(
                        center,
                        radius,
                        Stroke::new(1.5_f32, fade(category.color())),
                    );
                    uc().text_primary
                }
            };
            painter.text(
                center,
                Align2::CENTER_CENTER,
                def.phys_num.to_string(),
                number_font.clone(),
                fade(number_color),
            );

            // Label pill toward the outside.
            let text = pin_label(def, fit.label);
            let galley = painter.layout_no_wrap(text, label_font.clone(), fade(uc().text_primary));
            let pill_w = galley.size().x + 18.0;
            let pill_h = row_h - 6.0;
            let gap = radius + 10.0;
            let pill = if left {
                Rect::from_min_size(
                    Pos2::new(cx - gap - pill_w, cy - pill_h / 2.0),
                    Vec2::new(pill_w, pill_h),
                )
            } else {
                Rect::from_min_size(
                    Pos2::new(cx + gap, cy - pill_h / 2.0),
                    Vec2::new(pill_w, pill_h),
                )
            };
            // Light tint of the role color, with a solid bar on the edge facing the pin.
            painter.rect_filled(
                pill,
                CornerRadius::same(5),
                fade(category.color().gamma_multiply(LABEL_TINT)),
            );
            let bar = if left {
                Rect::from_min_max(
                    Pos2::new(pill.right() - 3.0, pill.top()),
                    pill.right_bottom(),
                )
            } else {
                Rect::from_min_max(pill.left_top(), Pos2::new(pill.left() + 3.0, pill.bottom()))
            };
            painter.rect_filled(bar, CornerRadius::same(2), fade(category.color()));
            let text_x = if left {
                pill.left() + 8.0
            } else {
                pill.left() + 10.0
            };
            painter.galley(
                Pos2::new(text_x, cy - galley.size().y / 2.0),
                galley,
                Color32::PLACEHOLDER,
            );

            let Some((bcm, high, dir)) = gpio else {
                continue;
            };

            // Mode next to the label, outside (when it fits).
            let mode_pos = if left {
                Pos2::new(pill.left() - 6.0, cy)
            } else {
                Pos2::new(pill.right() + 6.0, cy)
            };
            if fit.show_mode {
                painter.text(
                    mode_pos,
                    if left {
                        Align2::RIGHT_CENTER
                    } else {
                        Align2::LEFT_CENTER
                    },
                    dir.short_label(),
                    mode_font.clone(),
                    fade(mode_color(dir)),
                );
            }

            if selected == bcm {
                painter.circle_stroke(center, radius + 3.0, Stroke::new(2.5_f32, uc().orange));
                painter.rect_stroke(
                    pill.expand(1.5),
                    CornerRadius::same(6),
                    Stroke::new(2.0_f32, uc().orange),
                    StrokeKind::Outside,
                );
            }

            let hit_rect = Rect::from_center_size(center, Vec2::splat(radius * 2.0 + 4.0));
            let tip = format!(
                "{}\n{} | pull {} | {}",
                self.gpio_lab.pins[bcm as usize].role_note,
                dir.short_label(),
                pull_label(self.gpio_lab.pins[bcm as usize].pull),
                if high { "HIGH" } else { "LOW" },
            );
            for (part, area) in [("c", hit_rect), ("l", pill)] {
                let resp = ui.interact(
                    area,
                    ui.id().with(("pinout-pin", bcm, part)),
                    Sense::click(),
                );
                if resp.clicked() {
                    clicked = Some(bcm);
                }
                if resp.double_clicked() {
                    double = Some(bcm);
                }
                if resp.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                aorus_tooltip(
                    resp,
                    &format!("GPIO{bcm} · pin {}", def.phys_num),
                    &tip,
                    Some(&format!("pinctrl get {bcm}")),
                );
            }
        }
        if let Some(bcm) = clicked {
            self.gpio_lab.selected_bcm = bcm;
        }
        if let Some(bcm) = double {
            self.pin_double_click(bcm);
        }

        // Legend: each item is one widget of known width, so the row wraps.
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(12.0, 4.0);
            for c in PinCategory::ALL {
                legend_item(ui, c.name(), c.color(), false);
            }
            legend_item(ui, "HIGH", uc().green, true);
        });
        ui.label(
            RichText::new(t.gpio_diagram_hint)
                .size(11.5)
                .color(uc().text_secondary),
        );
    }

    fn draw_pin_table(&mut self, ui: &mut Ui) {
        let g = texts(self.lang).gpio;
        let rows: Vec<&PhysicalPinDef> = RPI_40PIN_HEADER
            .iter()
            .filter(|d| pin_matches(&self.gpio_query, d))
            .collect();
        if rows.is_empty() {
            ui.label(RichText::new(g.no_match).color(uc().text_secondary));
            return;
        }
        let selected = self.gpio_lab.selected_bcm;
        let mut clicked: Option<u8> = None;
        let mut double: Option<u8> = None;
        egui::ScrollArea::vertical()
            .id_salt("gpio_pin_table")
            .max_height(380.0)
            .show(ui, |ui| {
                egui::Grid::new("gpio_pin_table_grid")
                    .num_columns(7)
                    .striped(true)
                    .spacing([14.0, 4.0])
                    .show(ui, |ui| {
                        for h in [
                            g.col_pin,
                            g.col_gpio,
                            g.col_function,
                            g.col_mode,
                            g.col_pull,
                            g.col_level,
                            g.col_edges,
                        ] {
                            ui.label(
                                RichText::new(h)
                                    .size(11.5)
                                    .strong()
                                    .color(uc().text_secondary),
                            );
                        }
                        ui.end_row();

                        for def in rows {
                            let HeaderPinKind::Gpio(bcm) = def.kind else {
                                let color = match def.kind {
                                    HeaderPinKind::Power3v3 => uc().amber,
                                    HeaderPinKind::Power5v => uc().red,
                                    HeaderPinKind::IdEeprom => uc().cyan,
                                    _ => uc().text_secondary,
                                };
                                ui.label(
                                    mono(format!("{:>2}", def.phys_num)).color(uc().text_secondary),
                                );
                                ui.label("");
                                ui.label(mono(def.label).color(color));
                                for _ in 0..4 {
                                    ui.label("");
                                }
                                ui.end_row();
                                continue;
                            };
                            let Some(pin) = self.gpio_lab.pins.get(bcm as usize) else {
                                continue;
                            };
                            let is_sel = selected == bcm;
                            let cells = [
                                ui.selectable_label(is_sel, mono(format!("{:>2}", def.phys_num))),
                                ui.selectable_label(is_sel, mono(format!("GPIO{bcm}")).strong()),
                                ui.selectable_label(is_sel, mono(pin_function(def))),
                            ];
                            let hit = cells.iter().any(|r| r.clicked());
                            if cells.iter().any(|r| r.double_clicked()) {
                                double = Some(bcm);
                            }
                            ui.label(
                                mono(pin.direction.short_label()).color(mode_color(pin.direction)),
                            );
                            ui.label(mono(pull_label(pin.pull)).color(uc().text_secondary));
                            ui.horizontal(|ui| {
                                level_dot(ui, pin.level_high, 10.0);
                                ui.label(mono(if pin.level_high { "1" } else { "0" }));
                            });
                            ui.label(
                                mono(format!("⬆{} ⬇{}", pin.rising_edges, pin.falling_edges))
                                    .color(uc().text_secondary),
                            );
                            if hit {
                                clicked = Some(bcm);
                            }
                            ui.end_row();
                        }
                    });
            });
        if let Some(bcm) = clicked {
            self.gpio_lab.selected_bcm = bcm;
        }
        if let Some(bcm) = double {
            self.pin_double_click(bcm);
        }
    }

    /// Double-click on a pin: an output toggles its level, an input is added to or removed
    /// from the analyzer, and a pin used by an alternate function (I2C, UART...) is left alone.
    fn pin_double_click(&mut self, bcm: u8) {
        let Some(pin) = self.gpio_lab.pins.get(bcm as usize) else {
            return;
        };
        match pin.direction {
            PinDirection::Output => {
                let level = !pin.level_high;
                let res = self.gpio_lab.set_gpio_output(bcm, level);
                self.log_result(
                    &format!("GPIO{bcm} {}", if level { "HIGH" } else { "LOW" }),
                    res,
                );
            }
            PinDirection::Input => {
                let watched = self.gpio_lab.watched_bcm.contains(&bcm);
                toggle_channel(&mut self.gpio_lab, bcm, !watched);
                self.gpio_panel = GpioPanel::Analyzer;
            }
            _ => {}
        }
    }

    // ------------------------------------------------------------------ inspector

    fn gpio_inspector_card(&mut self, ui: &mut Ui) {
        let g = texts(self.lang).gpio;
        let bcm = self.gpio_lab.selected_bcm;
        let Some(pin) = self.gpio_lab.pins.get(bcm as usize) else {
            return;
        };
        let (dir, pull, high, rising, falling, role) = (
            pin.direction,
            pin.pull,
            pin.level_high,
            pin.rising_edges,
            pin.falling_edges,
            pin.role_note,
        );
        let phys = physical_pin(bcm).map(|d| d.phys_num).unwrap_or(0);
        let title = format!("GPIO{bcm} · {} {phys}", g.pin);

        let mut copied: Option<String> = None;
        aorus_card_with_tip(ui, &title, g.inspector_tip, uc().cyan, |ui| {
            ui.label(RichText::new(role).size(12.0).color(uc().text_secondary));
            if matches!(dir, PinDirection::Alt(_)) {
                ui.label(RichText::new(g.alt_warning).size(11.5).color(uc().amber));
            }
            ui.add_space(6.0);

            egui::Grid::new("gpio_inspector_grid")
                .num_columns(2)
                .spacing([14.0, 8.0])
                .show(ui, |ui| {
                    ui.label(RichText::new(g.mode).color(uc().text_secondary));
                    ui.horizontal(|ui| {
                        if segmented_pill(
                            ui,
                            "IN",
                            dir == PinDirection::Input,
                            uc().btn_cyan,
                            uc().cyan,
                        )
                        .clicked()
                            && dir != PinDirection::Input
                        {
                            let res = self.gpio_lab.set_gpio_input(bcm, pull);
                            self.log_result(&format!("GPIO{bcm} IN"), res);
                        }
                        if segmented_pill(
                            ui,
                            "OUT",
                            dir == PinDirection::Output,
                            uc().btn_orange,
                            uc().orange,
                        )
                        .clicked()
                            && dir != PinDirection::Output
                        {
                            let res = self.gpio_lab.set_gpio_output(bcm, high);
                            self.log_result(&format!("GPIO{bcm} OUT"), res);
                        }
                        if let PinDirection::Alt(_) | PinDirection::Unknown = dir {
                            segmented_pill(
                                ui,
                                dir.short_label(),
                                true,
                                uc().btn_slate,
                                uc().border_subtle,
                            );
                        }
                    });
                    ui.end_row();

                    ui.label(RichText::new(g.pull).color(uc().text_secondary));
                    ui.horizontal(|ui| {
                        for (p, label) in [
                            (PinPull::None, g.pull_none),
                            (PinPull::Up, g.pull_up),
                            (PinPull::Down, g.pull_down),
                        ] {
                            if segmented_pill(ui, label, pull == p, uc().btn_cyan, uc().cyan)
                                .clicked()
                                && pull != p
                            {
                                let res = self.gpio_lab.set_gpio_pull(bcm, p);
                                self.log_result(&format!("GPIO{bcm} pull {}", pull_label(p)), res);
                            }
                        }
                    });
                    ui.end_row();

                    ui.label(RichText::new(g.level).color(uc().text_secondary));
                    ui.horizontal(|ui| {
                        level_dot(ui, high, 12.0);
                        ui.label(
                            mono(if high { "HIGH" } else { "LOW" })
                                .strong()
                                .color(if high { uc().green } else { uc().text_primary }),
                        );
                        ui.add_space(8.0);
                        ui.label(mono(format!("⬆{rising} ⬇{falling}")).color(uc().text_secondary));
                        let r = ui.small_button(g.reset_counters);
                        if r.clicked() {
                            self.gpio_lab.reset_edge_counters(bcm);
                        }
                        r.on_hover_text(g.reset_counters_tip);
                    });
                    ui.end_row();

                    ui.label(RichText::new(g.drive).color(uc().text_secondary));
                    ui.horizontal(|ui| {
                        let lo = aorus_action_button(ui, "LOW", uc().btn_slate, uc().border_subtle);
                        let hi = aorus_action_button(ui, "HIGH", uc().btn_green, uc().green);
                        let tg = ui.button(g.toggle);
                        let target = if lo.clicked() {
                            Some(false)
                        } else if hi.clicked() {
                            Some(true)
                        } else if tg.clicked() {
                            Some(!high)
                        } else {
                            None
                        };
                        lo.on_hover_text(g.drive_tip);
                        hi.on_hover_text(g.drive_tip);
                        tg.on_hover_text(g.drive_tip);
                        if let Some(level) = target {
                            let res = self.gpio_lab.set_gpio_output(bcm, level);
                            self.log_result(
                                &format!("GPIO{bcm} {}", if level { "HIGH" } else { "LOW" }),
                                res,
                            );
                        }
                    });
                    ui.end_row();

                    ui.label("");
                    let mut watched = self.gpio_lab.watched_bcm.contains(&bcm);
                    if ui.checkbox(&mut watched, g.show_in_analyzer).changed() {
                        toggle_channel(&mut self.gpio_lab, bcm, watched);
                    }
                    ui.end_row();
                });

            ui.add_space(8.0);
            sub_heading(ui, g.commands);
            ui.add_space(2.0);
            for cmd in equivalent_commands(bcm, dir, pull, high) {
                egui::Frame::new()
                    .fill(uc().bg_subgroup)
                    .stroke(Stroke::new(1.0_f32, uc().border_subtle))
                    .corner_radius(CornerRadius::same(4))
                    .inner_margin(egui::Margin::symmetric(8, 4))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui.small_button(g.copy).clicked() {
                                        ui.ctx().copy_text(cmd.clone());
                                        copied = Some(cmd.clone());
                                    }
                                    ui.with_layout(
                                        egui::Layout::left_to_right(egui::Align::Center)
                                            .with_main_wrap(true),
                                        |ui| {
                                            ui.label(mono(&cmd).color(uc().text_primary));
                                        },
                                    );
                                },
                            );
                        });
                    });
                ui.add_space(3.0);
            }
        });
        if let Some(cmd) = copied {
            self.status_message = Some(format!("✔ {}: {cmd}", g.copied));
            self.status_is_error = false;
        }
    }

    // ------------------------------------------------------------------ bottom panel

    fn gpio_bottom_panel(&mut self, ui: &mut Ui) {
        let g = texts(self.lang).gpio;
        panel_frame(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                for (panel, label) in [
                    (GpioPanel::Analyzer, g.tab_analyzer),
                    (GpioPanel::Serial, g.tab_serial),
                    (GpioPanel::I2c, g.tab_i2c),
                    (GpioPanel::Log, g.tab_log),
                ] {
                    if segmented_pill(
                        ui,
                        label,
                        self.gpio_panel == panel,
                        uc().btn_orange,
                        uc().orange,
                    )
                    .clicked()
                    {
                        self.gpio_panel = panel;
                    }
                }
                if self.gpio_panel != GpioPanel::Analyzer {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        self.output_options(ui);
                    });
                }
            });
            ui.separator();
            match self.gpio_panel {
                GpioPanel::Analyzer => self.analyzer_tab(ui),
                GpioPanel::Serial => self.serial_tab(ui),
                GpioPanel::I2c => self.i2c_tab(ui),
                GpioPanel::Log => self.log_tab(ui),
            }
        });
    }

    /// Clear / pause / follow / time options shared by the text outputs (right to left).
    fn output_options(&mut self, ui: &mut Ui) {
        let g = texts(self.lang).gpio;
        if ui.small_button(g.clear).clicked() {
            let keep = match self.gpio_panel {
                GpioPanel::Serial => Some(LogSource::Serial),
                GpioPanel::I2c => Some(LogSource::I2c),
                _ => None,
            };
            match keep {
                Some(source) => self.gpio_lab.entries.retain(|e| e.source != source),
                None => self.gpio_lab.entries.clear(),
            }
        }
        ui.checkbox(&mut self.gpio_lab.paused, g.pause);
        ui.checkbox(&mut self.gpio_lab.auto_scroll, g.follow)
            .on_hover_text(g.follow_tip);
        ui.checkbox(&mut self.gpio_lab.show_timestamps, g.time);
    }

    fn analyzer_tab(&mut self, ui: &mut Ui) {
        let g = texts(self.lang).gpio;
        let colors = trace_colors();
        let mut remove: Option<u8> = None;
        ui.horizontal_wrapped(|ui| {
            for (i, &bcm) in self.gpio_lab.watched_bcm.iter().enumerate() {
                let col = colors[i % colors.len()];
                let r = ui.add(
                    egui::Button::new(mono(format!("GPIO{bcm}  ×")).strong().color(col))
                        .fill(uc().bg_subgroup)
                        .stroke(Stroke::new(1.0_f32, col))
                        .corner_radius(CornerRadius::same(10)),
                );
                if r.clicked() {
                    remove = Some(bcm);
                }
                r.on_hover_text(g.remove_channel);
            }
            let sel = self.gpio_lab.selected_bcm;
            if !self.gpio_lab.watched_bcm.contains(&sel)
                && self.gpio_lab.watched_bcm.len() < ANALYZER_MAX_CHANNELS
            {
                let r = ui.button(format!("+ {} GPIO{sel}", g.add_channel));
                if r.clicked() {
                    toggle_channel(&mut self.gpio_lab, sel, true);
                }
                r.on_hover_text(g.add_channel_tip);
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.checkbox(&mut self.gpio_lab.log_edges, g.log_edges)
                    .on_hover_text(g.log_edges_tip);
            });
        });
        if let Some(bcm) = remove {
            toggle_channel(&mut self.gpio_lab, bcm, false);
        }
        ui.add_space(6.0);
        if self.gpio_lab.watched_bcm.is_empty() {
            ui.label(RichText::new(g.analyzer_empty).color(uc().text_secondary));
        } else {
            draw_logic_analyzer_waveforms(ui, &self.gpio_lab);
        }
    }

    fn serial_tab(&mut self, ui: &mut Ui) {
        let g = texts(self.lang).gpio;
        ui.horizontal_wrapped(|ui| {
            let connected = self.gpio_lab.serial_connected;
            ui.add_enabled_ui(!connected, |ui| {
                let ports = self.gpio_lab.available_ports.clone();
                egui::ComboBox::from_id_salt("serial_port_combo")
                    .width(130.0)
                    .selected_text(mono(&self.gpio_lab.selected_port))
                    .show_ui(ui, |ui| {
                        for p in ports {
                            ui.selectable_value(&mut self.gpio_lab.selected_port, p.clone(), p);
                        }
                    });
                egui::ComboBox::from_id_salt("serial_baud_combo")
                    .width(100.0)
                    .selected_text(mono(format!("{} baud", self.gpio_lab.baud_rate)))
                    .show_ui(ui, |ui| {
                        for b in [9600, 19200, 38400, 57600, 115200, 230400, 460800, 921600] {
                            ui.selectable_value(
                                &mut self.gpio_lab.baud_rate,
                                b,
                                format!("{b} baud"),
                            );
                        }
                    });
            });
            let (label, fill, border) = if connected {
                (g.disconnect, uc().btn_slate, uc().border_subtle)
            } else {
                (g.connect, uc().btn_green, uc().green)
            };
            if aorus_action_button(ui, label, fill, border).clicked() {
                self.gpio_lab.toggle_serial_connection();
            }
            ui.separator();
            for (fmt, lbl) in [
                (DataFormat::Ascii, "ASCII"),
                (DataFormat::Hex, "HEX"),
                (DataFormat::Binary, "BIN"),
            ] {
                if segmented_pill(
                    ui,
                    lbl,
                    self.gpio_lab.data_format == fmt,
                    uc().btn_slate,
                    uc().cyan,
                )
                .clicked()
                {
                    self.gpio_lab.data_format = fmt;
                }
            }
            ui.label(
                mono(format!(
                    "RX {} B · TX {} B",
                    self.gpio_lab.rx_bytes_total, self.gpio_lab.tx_bytes_total
                ))
                .color(uc().text_secondary),
            );
        });
        ui.add_space(6.0);
        log_view(ui, &self.gpio_lab, Some(LogSource::Serial), g.empty_output);
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            let edit = ui.add(
                egui::TextEdit::singleline(&mut self.gpio_lab.tx_input)
                    .hint_text(g.serial_hint)
                    .font(TextStyle::Monospace)
                    .desired_width((ui.available_width() - 220.0).max(140.0)),
            );
            let enter = edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            egui::ComboBox::from_id_salt("line_ending_combo")
                .width(110.0)
                .selected_text(self.gpio_lab.line_ending.label())
                .show_ui(ui, |ui| {
                    for le in [
                        LineEnding::Newline,
                        LineEnding::BothNlCr,
                        LineEnding::CarriageReturn,
                        LineEnding::None,
                    ] {
                        ui.selectable_value(&mut self.gpio_lab.line_ending, le, le.label());
                    }
                });
            if aorus_action_button(ui, g.send, uc().btn_orange, uc().orange).clicked() || enter {
                self.gpio_lab.send_serial();
                edit.request_focus();
            }
        });
    }

    fn i2c_tab(&mut self, ui: &mut Ui) {
        let g = texts(self.lang).gpio;
        let health = i2c_health();
        ui.horizontal_wrapped(|ui| {
            ui.add_enabled_ui(health != Health::Off, |ui| {
                let r = aorus_action_button(ui, g.i2c_scan, uc().btn_cyan, uc().cyan);
                if r.clicked() {
                    self.gpio_lab.probe_i2c_bus();
                }
                aorus_tooltip(r, g.i2c_scan, g.i2c_scan_tip, Some("i2cdetect -y 1"));
                let r = aorus_action_button(ui, g.i2c_mcu, uc().btn_slate, uc().border_subtle);
                if r.clicked() {
                    self.gpio_lab.probe_argon_mcu_registers();
                }
                aorus_tooltip(r, g.i2c_mcu, g.i2c_mcu_tip, Some("I2C 0x1a"));
            });
            if health == Health::Off {
                ui.label(
                    RichText::new(g.status_i2c_off_tip)
                        .size(12.0)
                        .color(uc().amber),
                );
                if ui.button(g.open_hardware).clicked() {
                    self.active_tab = ActiveTab::Features;
                }
            }
        });
        ui.add_space(6.0);
        log_view(ui, &self.gpio_lab, Some(LogSource::I2c), g.empty_output);
    }

    fn log_tab(&mut self, ui: &mut Ui) {
        let g = texts(self.lang).gpio;
        log_view(ui, &self.gpio_lab, None, g.empty_output);
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.label(mono(">").strong().color(uc().orange));
            let edit = ui.add(
                egui::TextEdit::singleline(&mut self.gpio_lab.cmd_input)
                    .hint_text(g.cmd_hint)
                    .font(TextStyle::Monospace)
                    .desired_width((ui.available_width() - 90.0).max(140.0)),
            );
            let enter = edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if aorus_action_button(ui, g.send, uc().btn_orange, uc().orange).clicked() || enter {
                self.gpio_lab.run_command();
                edit.request_focus();
            }
        });
    }
}

/// Adds or removes an analyzer channel (at most four; the oldest is replaced).
fn toggle_channel(lab: &mut GpioSerialLabState, bcm: u8, on: bool) {
    if on {
        if !lab.watched_bcm.contains(&bcm) {
            if lab.watched_bcm.len() >= ANALYZER_MAX_CHANNELS {
                lab.watched_bcm.remove(0);
            }
            lab.watched_bcm.push(bcm);
        }
    } else {
        lab.watched_bcm.retain(|&b| b != bcm);
    }
}

fn source_tag(source: LogSource) -> &'static str {
    match source {
        LogSource::System => "sys",
        LogSource::Gpio => "gpio",
        LogSource::Serial => "serial",
        LogSource::I2c => "i2c",
    }
}

/// Monospace output of the console entries, optionally limited to one source.
fn log_view(ui: &mut Ui, lab: &GpioSerialLabState, only: Option<LogSource>, empty: &str) {
    let fmt = if only == Some(LogSource::Serial) {
        lab.data_format
    } else {
        DataFormat::Ascii
    };
    egui::Frame::new()
        .fill(uc().bg_subgroup)
        .stroke(Stroke::new(1.0_f32, uc().border_subtle))
        .corner_radius(CornerRadius::same(4))
        .inner_margin(egui::Margin::same(8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            egui::ScrollArea::vertical()
                .id_salt(("gpio_log_view", only.map(source_tag)))
                .min_scrolled_height(OUTPUT_HEIGHT)
                .max_height(OUTPUT_HEIGHT)
                .stick_to_bottom(lab.auto_scroll)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    let entries: Vec<&ConsoleEntry> = lab
                        .entries
                        .iter()
                        .filter(|e| only.is_none_or(|s| e.source == s))
                        .collect();
                    if entries.is_empty() {
                        ui.label(RichText::new(empty).color(uc().text_secondary));
                        return;
                    }
                    let font = FontId::monospace(12.0);
                    for entry in entries {
                        let (badge, color) = match entry.direction {
                            "TX" => ("TX ", uc().amber),
                            "RX" => ("RX ", uc().green),
                            "EV" => ("EV ", uc().cyan),
                            "ERR" => ("ERR", uc().red),
                            _ => ("SYS", uc().text_secondary),
                        };
                        let mut job = LayoutJob::default();
                        let plain = TextFormat::simple(font.clone(), uc().text_secondary);
                        if lab.show_timestamps {
                            job.append(&format!("{} ", entry.timestamp), 0.0, plain.clone());
                        }
                        job.append(badge, 0.0, TextFormat::simple(font.clone(), color));
                        if only.is_none() {
                            job.append(&format!(" {:<6}", source_tag(entry.source)), 0.0, plain);
                        }
                        job.append(
                            &format!(" {}", format_console_entry_payload(entry, fmt)),
                            0.0,
                            TextFormat::simple(font.clone(), uc().text_primary),
                        );
                        ui.label(job);
                    }
                });
        });
}

/// Logic analyzer: one square-wave trace per watched GPIO.
fn draw_logic_analyzer_waveforms(ui: &mut Ui, lab: &GpioSerialLabState) {
    let channels = &lab.watched_bcm;
    let row_h = 40.0;
    let total_h = (channels.len().max(1) as f32 * row_h) + 12.0;
    let desired = Vec2::new(ui.available_width(), total_h);
    let (rect, _) = ui.allocate_exact_size(desired, Sense::hover());
    let painter = ui.painter_at(rect);

    painter.rect_filled(rect, CornerRadius::same(5), uc().bg_subgroup);
    painter.rect_stroke(
        rect,
        CornerRadius::same(5),
        Stroke::new(1.0_f32, uc().border_subtle),
        StrokeKind::Inside,
    );

    let label_col_w = 130.0;
    let colors = trace_colors();

    for (idx, &bcm) in channels.iter().enumerate() {
        let Some(pin) = lab.pins.get(bcm as usize) else {
            continue;
        };
        let y_top = rect.top() + 6.0 + (idx as f32) * row_h;
        let y_high = y_top + 7.0;
        let y_low = y_top + row_h - 9.0;
        let trace_color = colors[idx % colors.len()];

        if idx > 0 {
            painter.line_segment(
                [
                    Pos2::new(rect.left() + 4.0, y_top - 2.0),
                    Pos2::new(rect.right() - 4.0, y_top - 2.0),
                ],
                Stroke::new(1.0_f32, chart_grid()),
            );
        }

        painter.text(
            Pos2::new(rect.left() + 10.0, y_top + 10.0),
            Align2::LEFT_CENTER,
            format!("GPIO{:02} {}", pin.bcm, pin.direction.short_label()),
            FontId::new(11.5, FontFamily::Monospace),
            trace_color,
        );
        painter.text(
            Pos2::new(rect.left() + 10.0, y_top + 25.0),
            Align2::LEFT_CENTER,
            format!(
                "{} ⬆{} ⬇{}",
                if pin.level_high { "1" } else { "0" },
                pin.rising_edges,
                pin.falling_edges
            ),
            FontId::new(10.5, FontFamily::Monospace),
            uc().text_secondary,
        );

        let plot_left = rect.left() + label_col_w;
        let plot_right = rect.right() - 10.0;
        let plot_w = (plot_right - plot_left).max(20.0);

        let n = pin.history.len();
        if n >= 2 {
            let mut pts = Vec::with_capacity(n * 2);
            for (i, &val) in pin.history.iter().enumerate() {
                let x = plot_left + (i as f32 / (n - 1) as f32) * plot_w;
                let y = if val > 0 { y_high } else { y_low };
                if i > 0 {
                    let prev_val = pin.history[i - 1];
                    if prev_val != val {
                        // Vertical edge for a crisp digital trace.
                        let prev_y = if prev_val > 0 { y_high } else { y_low };
                        pts.push(Pos2::new(x, prev_y));
                    }
                }
                pts.push(Pos2::new(x, y));
            }
            painter.add(Shape::line(pts, Stroke::new(2.0_f32, trace_color)));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn def(phys: u8) -> &'static PhysicalPinDef {
        &RPI_40PIN_HEADER[phys as usize - 1]
    }

    #[test]
    fn search_understands_gpio_physical_and_names() {
        assert!(pin_matches("17", def(11)));
        assert!(pin_matches("gpio17", def(11)));
        assert!(!pin_matches("17", def(17)));
        assert!(pin_matches("p11", def(11)));
        assert!(pin_matches("sda", def(3)));
        assert!(pin_matches("", def(1)));
        assert_eq!(first_gpio_match("scl"), Some(3));
        assert_eq!(first_gpio_match("p8"), Some(14));
        assert_eq!(first_gpio_match("zzz"), None);
    }

    #[test]
    fn function_comes_from_the_label() {
        assert_eq!(pin_function(def(3)), "SDA1 I2C");
        assert_eq!(pin_function(def(1)), "");
    }

    #[test]
    fn commands_follow_the_pin_state() {
        let out = equivalent_commands(17, PinDirection::Output, PinPull::None, true);
        assert!(out.contains(&"pinctrl set 17 op dh".to_string()));
        assert!(out.contains(&"gpioset -t0 GPIO17=1".to_string()));
        let inp = equivalent_commands(4, PinDirection::Input, PinPull::Down, false);
        assert!(inp.contains(&"pinctrl set 4 ip pd".to_string()));
        assert!(inp.iter().any(|c| c.contains("pull_up=False")));
    }

    #[test]
    fn analyzer_keeps_at_most_four_channels() {
        let mut lab = GpioSerialLabState::new();
        lab.watched_bcm = vec![2, 3, 4, 23];
        toggle_channel(&mut lab, 17, true);
        assert_eq!(lab.watched_bcm, vec![3, 4, 23, 17]);
        toggle_channel(&mut lab, 4, false);
        assert_eq!(lab.watched_bcm, vec![3, 23, 17]);
    }
}
