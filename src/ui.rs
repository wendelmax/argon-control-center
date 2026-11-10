use crate::argon::{
    apply_argon_eeprom_psu_current, control_argononed_service, evaluate_fan_speed,
    is_oled_hardware_installed, load_fan_curve, load_oled_config, load_temp_unit,
    reset_argon_ir_power_button, save_fan_curve, save_oled_config, save_temp_unit,
    serialize_fan_curve, set_immediate_fan_speed, FanCurvePoint, FanPreset, OledConfig, TempUnit,
    CPU_FAN_CONF, HDD_FAN_CONF,
};
use crate::gpio_lab::{
    format_console_entry_payload, ConsoleMode, DataFormat, GpioSerialLabState, HeaderPinKind,
    LineEnding, PinPull, RPI_40PIN_HEADER,
};
use crate::i18n::{texts, Language};
use crate::overclock::{
    apply_runtime_cpufreq, get_boot_config_path, load_boot_config, save_boot_config,
    update_boot_config_str, BootOcConfig, OcPreset,
};
use crate::privileged::is_root;
use crate::telemetry::TelemetryState;
use crate::theme::{
    configure_kde_breeze_style, draw_kde_area_line_chart, draw_kde_donut_chart,
    draw_kde_donut_chart_sized, set_ui_colors, uc, KdeBreezePalette, ThemeModel, UiColors,
};
use egui::{
    Align2, Color32, CornerRadius, FontFamily, FontId, Pos2, Rect, Response, RichText, Sense,
    Shape, Stroke, StrokeKind, TextStyle, Ui, Vec2,
};
use std::collections::VecDeque;
use std::f32::consts::PI;
use std::fs;
use std::time::{Duration, Instant};

// High-Contrast Gigabyte AORUS Palette (WCAG AAA readable)

// Accent Colors

// High-Contrast Button Fills (paired with pure WHITE text)

// High-Contrast Typography Colors

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActiveTab {
    Dashboard,
    SmartFan,
    Overclocking,
    Features,
    GpioLab,
    ServicesAndConfig,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FanTargetMode {
    Cpu,
    Hdd,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FanControlSubTab {
    AutoCurve,
    ManualOverride,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigPreviewTab {
    ArgonFanConf,
    BootConfigTxt,
}

pub struct ArgonControlCenterApp {
    pub lang: Language,
    pub theme_model: ThemeModel,
    pub kde_palette: KdeBreezePalette,
    palette_rx: Option<std::sync::mpsc::Receiver<KdeBreezePalette>>,
    last_palette_check: Instant,
    pub active_tab: ActiveTab,
    pub telemetry: TelemetryState,
    pub last_tick: Instant,

    // Fan configuration state
    pub fan_target_mode: FanTargetMode,
    pub fan_control_subtab: FanControlSubTab,
    pub cpu_fan_curve: Vec<FanCurvePoint>,
    pub hdd_fan_curve: Vec<FanCurvePoint>,
    pub selected_fan_preset: FanPreset,
    pub dragging_point_idx: Option<usize>,
    pub manual_fan_slider: u8,

    // Overclocking & Runtime CPU state
    pub boot_oc: BootOcConfig,
    pub selected_oc_preset: OcPreset,
    pub runtime_governor: String,
    pub runtime_min_mhz: u32,
    pub runtime_max_mhz: u32,
    pub runtime_boost: bool,

    // Units & OLED
    pub temp_unit: TempUnit,
    pub oled_cfg: OledConfig,
    pub oled_hw_present: bool,

    // Services & Inspector
    pub config_preview_tab: ConfigPreviewTab,
    pub gpio_lab: GpioSerialLabState,

    // Status & Logs
    pub status_message: Option<String>,
    pub status_is_error: bool,
    pub action_logs: VecDeque<String>,
}

impl ArgonControlCenterApp {
    pub fn new(cc: &eframe::CreationContext<'_>, initial_lang: Language) -> Self {
        let kde_palette = KdeBreezePalette::load_from_system();
        configure_kde_breeze_style(&cc.egui_ctx, &kde_palette);

        let cpu_fan_curve = load_fan_curve(CPU_FAN_CONF, FanPreset::BalancedDefault.points());
        let hdd_fan_curve = load_fan_curve(
            HDD_FAN_CONF,
            vec![
                FanCurvePoint {
                    temp_c: 35.0,
                    speed_pct: 30,
                },
                FanCurvePoint {
                    temp_c: 40.0,
                    speed_pct: 55,
                },
                FanCurvePoint {
                    temp_c: 45.0,
                    speed_pct: 100,
                },
            ],
        );
        let mut telemetry = TelemetryState::new();
        telemetry.refresh(&cpu_fan_curve);

        let boot_oc = load_boot_config();
        let temp_unit = load_temp_unit();
        let oled_cfg = load_oled_config();
        let oled_hw_present = is_oled_hardware_installed();

        let runtime_governor = telemetry.cpu_governor.clone();
        let runtime_min_mhz = telemetry.cpu_min_freq_mhz;
        let runtime_max_mhz = telemetry.cpu_max_freq_mhz;
        let runtime_boost = telemetry.cpu_boost_enabled;

        let mut logs = VecDeque::new();
        logs.push_back(format!(
            "[INIT] Argon AORUS Control Center — {} ({})",
            telemetry.board_model, telemetry.os_name
        ));
        logs.push_back(format!(
            "[INIT] Fan Curve: {} ({} stages)",
            CPU_FAN_CONF,
            cpu_fan_curve.len()
        ));

        Self {
            lang: initial_lang,
            theme_model: ThemeModel::KdeCompactOverview,
            kde_palette,
            palette_rx: None,
            last_palette_check: Instant::now(),
            active_tab: ActiveTab::Dashboard,
            telemetry,
            last_tick: Instant::now(),
            fan_target_mode: FanTargetMode::Cpu,
            fan_control_subtab: FanControlSubTab::AutoCurve,
            cpu_fan_curve,
            hdd_fan_curve,
            selected_fan_preset: FanPreset::BalancedDefault,
            dragging_point_idx: None,
            manual_fan_slider: 55,
            boot_oc,
            selected_oc_preset: OcPreset::StockBoost,
            runtime_governor,
            runtime_min_mhz,
            runtime_max_mhz,
            runtime_boost,
            temp_unit,
            oled_cfg,
            oled_hw_present,
            config_preview_tab: ConfigPreviewTab::ArgonFanConf,
            gpio_lab: GpioSerialLabState::new(),
            status_message: None,
            status_is_error: false,
            action_logs: logs,
        }
    }

    fn log_result(&mut self, action: &str, res: Result<String, String>) {
        match res {
            Ok(msg) => {
                self.status_message = Some(format!("✔ {action}: {msg}"));
                self.status_is_error = false;
                if self.action_logs.len() >= 100 {
                    self.action_logs.pop_front();
                }
                self.action_logs
                    .push_back(format!("[OK] {action} -> {msg}"));
            }
            Err(err) => {
                self.status_message = Some(format!("✖ {action}: {err}"));
                self.status_is_error = true;
                if self.action_logs.len() >= 100 {
                    self.action_logs.pop_front();
                }
                self.action_logs
                    .push_back(format!("[ERR] {action} -> {err}"));
            }
        }
    }
}

pub fn configure_aorus_style(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    style.text_styles.insert(
        TextStyle::Heading,
        FontId::new(18.0, FontFamily::Proportional),
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
        FontId::new(12.0, FontFamily::Proportional),
    );
    style.text_styles.insert(
        TextStyle::Monospace,
        FontId::new(12.5, FontFamily::Monospace),
    );
    style.spacing.item_spacing = Vec2::new(8.0, 7.0);
    style.spacing.button_padding = Vec2::new(10.0, 6.0);
    style.interaction.tooltip_delay = 0.15;

    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = uc().bg_dark;
    visuals.window_fill = uc().bg_panel;
    visuals.window_stroke = Stroke::new(1.2_f32, uc().orange);
    visuals.extreme_bg_color = Color32::from_rgb(8, 12, 18);
    visuals.faint_bg_color = uc().bg_card;
    visuals.code_bg_color = Color32::from_rgb(9, 13, 21);

    visuals.selection.bg_fill = uc().btn_orange;
    visuals.selection.stroke = Stroke::new(1.5_f32, Color32::WHITE);

    visuals.widgets.noninteractive.bg_fill = uc().bg_card;
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, uc().border_subtle);
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.2_f32, uc().text_primary);
    visuals.widgets.noninteractive.corner_radius = CornerRadius::same(5);

    visuals.widgets.inactive.bg_fill = uc().btn_slate;
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, uc().border_subtle);
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.2_f32, Color32::WHITE);
    visuals.widgets.inactive.corner_radius = CornerRadius::same(5);

    visuals.widgets.hovered.bg_fill = uc().bg_card_hover;
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.5_f32, uc().orange);
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.5_f32, Color32::WHITE);
    visuals.widgets.hovered.corner_radius = CornerRadius::same(5);

    visuals.widgets.active.bg_fill = uc().btn_orange;
    visuals.widgets.active.bg_stroke = Stroke::new(1.5_f32, Color32::WHITE);
    visuals.widgets.active.fg_stroke = Stroke::new(1.5_f32, Color32::WHITE);
    visuals.widgets.active.corner_radius = CornerRadius::same(5);

    visuals.widgets.open.bg_fill = uc().bg_card_hover;
    visuals.widgets.open.bg_stroke = Stroke::new(1.5_f32, uc().cyan);
    visuals.widgets.open.fg_stroke = Stroke::new(1.5_f32, Color32::WHITE);
    visuals.widgets.open.corner_radius = CornerRadius::same(5);

    style.visuals = visuals;
    ctx.set_style(style);
}

impl eframe::App for ArgonControlCenterApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        set_ui_colors(if self.theme_model == ThemeModel::AorusGaming {
            UiColors::AORUS
        } else {
            UiColors::from_palette(&self.kde_palette)
        });
        // Poll serial RX continuously when on GPIO Lab tab or connected
        if self.gpio_lab.serial_connected {
            self.gpio_lab.poll_serial_rx();
        }

        let tick_interval = if self.active_tab == ActiveTab::GpioLab {
            Duration::from_millis(350)
        } else {
            Duration::from_millis(1000)
        };

        if self.last_tick.elapsed() >= tick_interval {
            self.telemetry.refresh(&self.cpu_fan_curve);
            if self.active_tab == ActiveTab::GpioLab {
                self.gpio_lab.poll_gpio_states();
            }
            self.last_tick = Instant::now();
        }
        ctx.request_repaint_after(tick_interval);

        self.poll_system_palette(ctx);

        match self.theme_model {
            ThemeModel::KdeCompactOverview | ThemeModel::KdeBreeze => {
                let pal = self.kde_palette;
                // 1. Kirigami Global Drawer Left Sidebar (on windows >= 680px wide)
                if ctx.screen_rect().width() >= 680.0 {
                    egui::SidePanel::left("kde_kirigami_sidebar")
                        .resizable(false)
                        .exact_width(220.0)
                        .frame(
                            egui::Frame::new()
                                .fill(pal.window_alt_bg)
                                .stroke(Stroke::new(1.0_f32, pal.border_subtle))
                                .inner_margin(egui::Margin::symmetric(10, 10)),
                        )
                        .show(ctx, |ui| {
                            self.draw_kde_sidebar(ui, ctx);
                        });
                }

                // 2. Contextual Kirigami Page Header Bar
                egui::TopBottomPanel::top("kde_page_header")
                    .frame(
                        egui::Frame::new()
                            .fill(pal.header_bg)
                            .stroke(Stroke::new(1.0_f32, pal.border_subtle))
                            .inner_margin(egui::Margin::symmetric(14, 8)),
                    )
                    .show(ctx, |ui| {
                        self.draw_kde_page_header(ui, ctx);
                    });

                // 3. Breeze Bottom Status Strip
                egui::TopBottomPanel::bottom("kde_bottom_bar")
                    .frame(
                        egui::Frame::new()
                            .fill(pal.window_alt_bg)
                            .stroke(Stroke::new(1.0_f32, pal.border_subtle))
                            .inner_margin(egui::Margin::symmetric(14, 5)),
                    )
                    .show(ctx, |ui| {
                        self.draw_bottom_bar(ui);
                    });

                // 4. Main Breeze Window Content Area
                egui::CentralPanel::default()
                    .frame(
                        egui::Frame::new()
                            .fill(pal.window_bg)
                            .inner_margin(egui::Margin::symmetric(14, 10)),
                    )
                    .show(ctx, |ui| {
                        egui::ScrollArea::vertical()
                            .auto_shrink([false, false])
                            .show(ui, |ui| match self.active_tab {
                                ActiveTab::Dashboard => {
                                    if self.theme_model == ThemeModel::KdeCompactOverview {
                                        self.draw_kde_compact_overview_tab(ui);
                                    } else {
                                        self.draw_kde_dashboard_tab(ui);
                                    }
                                }
                                ActiveTab::SmartFan => self.draw_smart_fan_tab(ui),
                                ActiveTab::Overclocking => self.draw_overclocking_tab(ui),
                                ActiveTab::Features => self.draw_features_tab(ui),
                                ActiveTab::GpioLab => self.draw_gpio_lab_tab(ui),
                                ActiveTab::ServicesAndConfig => self.draw_services_tab(ui),
                            });
                    });
            }
            ThemeModel::BreezeClassicLayout | ThemeModel::AorusGaming => {
                let is_breeze_pal = self.theme_model == ThemeModel::BreezeClassicLayout;
                let top_fill = if is_breeze_pal {
                    self.kde_palette.header_bg
                } else {
                    uc().bg_panel
                };
                let body_fill = if is_breeze_pal {
                    self.kde_palette.window_bg
                } else {
                    uc().bg_dark
                };
                let bdr = if is_breeze_pal {
                    self.kde_palette.border_subtle
                } else {
                    uc().border_subtle
                };

                egui::TopBottomPanel::top("aorus_top_bar")
                    .frame(
                        egui::Frame::new()
                            .fill(top_fill)
                            .stroke(Stroke::new(1.0_f32, bdr))
                            .inner_margin(egui::Margin::symmetric(14, 8)),
                    )
                    .show(ctx, |ui| {
                        self.draw_top_bar(ui, ctx);
                    });

                egui::TopBottomPanel::bottom("aorus_bottom_bar")
                    .frame(
                        egui::Frame::new()
                            .fill(top_fill)
                            .stroke(Stroke::new(1.0_f32, bdr))
                            .inner_margin(egui::Margin::symmetric(14, 6)),
                    )
                    .show(ctx, |ui| {
                        self.draw_bottom_bar(ui);
                    });

                egui::CentralPanel::default()
                    .frame(
                        egui::Frame::new()
                            .fill(body_fill)
                            .inner_margin(egui::Margin::symmetric(14, 12)),
                    )
                    .show(ctx, |ui| {
                        egui::ScrollArea::vertical()
                            .auto_shrink([false, false])
                            .show(ui, |ui| match self.active_tab {
                                ActiveTab::Dashboard => self.draw_dashboard_tab(ui),
                                ActiveTab::SmartFan => self.draw_smart_fan_tab(ui),
                                ActiveTab::Overclocking => self.draw_overclocking_tab(ui),
                                ActiveTab::Features => self.draw_features_tab(ui),
                                ActiveTab::GpioLab => self.draw_gpio_lab_tab(ui),
                                ActiveTab::ServicesAndConfig => self.draw_services_tab(ui),
                            });
                    });
            }
        }
    }
}

impl ArgonControlCenterApp {
    fn draw_theme_switcher_pills(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        let t = texts(self.lang);
        for (model, label, fill, border) in [
            (
                ThemeModel::AorusGaming,
                t.theme_aorus_label,
                uc().btn_orange,
                uc().orange,
            ),
            (
                ThemeModel::BreezeClassicLayout,
                t.theme_breeze_classic_label,
                Color32::from_rgb(38, 86, 128),
                self.kde_palette.focus_blue,
            ),
            (
                ThemeModel::KdeBreeze,
                t.theme_kde_label,
                Color32::from_rgb(24, 98, 145),
                self.kde_palette.focus_blue,
            ),
            (
                ThemeModel::KdeCompactOverview,
                t.theme_kde_compact_label,
                Color32::from_rgb(18, 115, 135),
                self.kde_palette.positive_green,
            ),
        ] {
            let is_sel = self.theme_model == model;
            let resp = segmented_pill(ui, label, is_sel, fill, border);
            if resp.clicked() && !is_sel {
                self.theme_model = model;
                match model {
                    ThemeModel::KdeCompactOverview
                    | ThemeModel::KdeBreeze
                    | ThemeModel::BreezeClassicLayout => {
                        self.kde_palette = KdeBreezePalette::load_from_system();
                        configure_kde_breeze_style(ctx, &self.kde_palette);
                    }
                    ThemeModel::AorusGaming => {
                        configure_aorus_style(ctx);
                    }
                }
            }
            aorus_tooltip(
                resp,
                "Theme Model / Modelo de Tema",
                t.theme_selector_tip,
                Some(&self.kde_palette.describe()),
            );
        }
    }

    /// Re-reads the desktop palette (KDE / GNOME / XFCE / portal…) every few seconds on a worker
    /// thread and restyles only when the colors actually changed, so theme switches in the
    /// system settings are picked up live.
    fn poll_system_palette(&mut self, ctx: &egui::Context) {
        if let Some(rx) = &self.palette_rx {
            match rx.try_recv() {
                Ok(pal) => {
                    self.palette_rx = None;
                    if !pal.same_colors(&self.kde_palette) {
                        self.kde_palette = pal;
                        if self.theme_model != ThemeModel::AorusGaming {
                            configure_kde_breeze_style(ctx, &self.kde_palette);
                        }
                        ctx.request_repaint();
                    }
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => self.palette_rx = None,
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
            }
            return;
        }
        if self.theme_model == ThemeModel::AorusGaming
            || self.last_palette_check.elapsed() < Duration::from_secs(4)
        {
            return;
        }
        self.last_palette_check = Instant::now();
        let (tx, rx) = std::sync::mpsc::channel();
        self.palette_rx = Some(rx);
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(KdeBreezePalette::load_from_system());
            ctx.request_repaint();
        });
    }

    fn draw_kde_sidebar(&mut self, ui: &mut Ui, _ctx: &egui::Context) {
        let t = texts(self.lang);
        let pal = self.kde_palette;

        // Sidebar App Branding Header
        ui.horizontal(|ui| {
            let (logo_rect, _) = ui.allocate_exact_size(Vec2::new(28.0, 28.0), Sense::hover());
            let painter = ui.painter();
            painter.rect_filled(logo_rect, CornerRadius::same(4), pal.selection_bg);
            painter.text(
                logo_rect.center(),
                Align2::CENTER_CENTER,
                "⚙",
                FontId::new(16.0, FontFamily::Proportional),
                Color32::WHITE,
            );
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 1.0;
                ui.label(
                    RichText::new("Argon Monitor")
                        .size(14.5)
                        .strong()
                        .color(pal.text_normal),
                );
                ui.label(
                    RichText::new("KDE Breeze Edition")
                        .size(11.0)
                        .color(pal.focus_blue),
                );
            });
        });

        ui.add_space(6.0);
        ui.separator();
        ui.add_space(4.0);

        // Kirigami Navigation List
        let tabs = [
            (ActiveTab::Dashboard, t.tab_dashboard, t.tab_dashboard_tip),
            (ActiveTab::SmartFan, t.tab_smart_fan, t.tab_smart_fan_tip),
            (
                ActiveTab::Overclocking,
                t.tab_overclock,
                t.tab_overclock_tip,
            ),
            (ActiveTab::Features, t.tab_features, t.tab_features_tip),
            (ActiveTab::GpioLab, t.tab_gpio_lab, t.tab_gpio_lab_tip),
            (
                ActiveTab::ServicesAndConfig,
                t.tab_services,
                t.tab_services_tip,
            ),
        ];

        for (tab, label, tip) in tabs {
            let selected = self.active_tab == tab;
            let desired = Vec2::new(ui.available_width(), 34.0);
            let (rect, resp) = ui.allocate_exact_size(desired, Sense::click());
            let painter = ui.painter_at(rect);

            if selected {
                painter.rect_filled(rect, CornerRadius::same(4), pal.selected_fill);
                painter.rect_stroke(
                    rect,
                    CornerRadius::same(4),
                    Stroke::new(1.0_f32, pal.focus_blue),
                    StrokeKind::Inside,
                );
                // Kirigami left active accent bar
                let bar = Rect::from_min_size(
                    Pos2::new(rect.left() + 2.0, rect.top() + 5.0),
                    Vec2::new(3.5, rect.height() - 10.0),
                );
                painter.rect_filled(bar, CornerRadius::same(2), pal.focus_blue);
            } else if resp.hovered() {
                painter.rect_filled(rect, CornerRadius::same(4), pal.hover_bg);
            }

            painter.text(
                Pos2::new(rect.left() + 12.0, rect.center().y),
                Align2::LEFT_CENTER,
                label,
                FontId::new(13.0, FontFamily::Proportional),
                if selected {
                    pal.on_selected
                } else {
                    pal.text_normal
                },
            );

            if resp.clicked() {
                self.active_tab = tab;
            }
            aorus_tooltip(resp, label, tip, None);
            ui.add_space(2.0);
        }

        ui.add_space(10.0);
        ui.separator();
        ui.add_space(6.0);

        // Sidebar Hardware Summary & Argononed Guardian Card
        egui::Frame::new()
            .fill(pal.view_bg)
            .stroke(Stroke::new(1.0_f32, pal.border_subtle))
            .corner_radius(CornerRadius::same(4))
            .inner_margin(egui::Margin::symmetric(8, 8))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                let (daemon_col, daemon_txt) = if self.telemetry.argon_service_active {
                    (pal.positive_green, t.daemon_online)
                } else {
                    (pal.negative_red, t.daemon_offline)
                };
                let badge_resp = ui.add(
                    egui::Button::new(
                        RichText::new(daemon_txt)
                            .size(11.5)
                            .strong()
                            .color(daemon_col),
                    )
                    .frame(false),
                );
                if badge_resp.clicked() {
                    self.active_tab = ActiveTab::ServicesAndConfig;
                }
                aorus_tooltip(
                    badge_resp,
                    "Systemd Daemon (argononed.service)",
                    t.daemon_badge_tip,
                    Some("/lib/systemd/system/argononed.service"),
                );

                ui.add_space(4.0);
                let (v_col, v_txt) = if self.telemetry.undervoltage_alarm {
                    (pal.negative_red, t.voltage_alert)
                } else {
                    (pal.positive_green, t.voltage_stable)
                };
                let v_resp = ui.label(RichText::new(v_txt).size(11.5).strong().color(v_col));
                aorus_tooltip(
                    v_resp,
                    "5V Power Rail",
                    t.voltage_tip,
                    Some("/sys/class/hwmon/hwmon1/in0_lcrit_alarm"),
                );

                ui.add_space(4.0);
                let board = crate::board::current();
                ui.label(
                    RichText::new(board.summary())
                        .size(11.0)
                        .color(pal.text_inactive),
                )
                .on_hover_text(format!(
                    "{}\n\n{}",
                    t.board_detect_tip,
                    board.details()
                ));
                ui.label(
                    RichText::new(format!("IP: {}", self.telemetry.ip_address))
                        .size(11.0)
                        .color(pal.text_inactive),
                );
            });
    }

    fn draw_kde_page_header(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        let t = texts(self.lang);
        let pal = self.kde_palette;

        let active_title = match self.active_tab {
            ActiveTab::Dashboard => t.tab_dashboard,
            ActiveTab::SmartFan => t.tab_smart_fan,
            ActiveTab::Overclocking => t.tab_overclock,
            ActiveTab::Features => t.tab_features,
            ActiveTab::GpioLab => t.tab_gpio_lab,
            ActiveTab::ServicesAndConfig => t.tab_services,
        };

        ui.horizontal_wrapped(|ui| {
            ui.label(
                RichText::new(active_title)
                    .size(17.0)
                    .strong()
                    .color(pal.text_normal),
            );

            // Fallback compact tab selector when window < 680px (sidebar hidden)
            if ctx.screen_rect().width() < 680.0 {
                ui.separator();
                for (tab, short) in [
                    (ActiveTab::Dashboard, "📊"),
                    (ActiveTab::SmartFan, "🌀"),
                    (ActiveTab::Overclocking, "🔥"),
                    (ActiveTab::Features, "🛠"),
                    (ActiveTab::GpioLab, "🔬"),
                    (ActiveTab::ServicesAndConfig, "⚙"),
                ] {
                    let is_sel = self.active_tab == tab;
                    if segmented_pill(
                        ui,
                        short,
                        is_sel,
                        Color32::from_rgb(28, 108, 156),
                        pal.focus_blue,
                    )
                    .clicked()
                    {
                        self.active_tab = tab;
                    }
                }
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                egui::Frame::new()
                    .fill(pal.view_alt_bg)
                    .stroke(Stroke::new(1.0_f32, pal.border_subtle))
                    .corner_radius(CornerRadius::same(4))
                    .inner_margin(egui::Margin::symmetric(8, 3))
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;

                        // 1. Temp Unit (°C | °F)
                        for (u_opt, u_lbl) in
                            [(TempUnit::Fahrenheit, "°F"), (TempUnit::Celsius, "°C")]
                        {
                            let is_sel = self.temp_unit == u_opt;
                            let resp = segmented_pill(
                                ui,
                                u_lbl,
                                is_sel,
                                Color32::from_rgb(28, 108, 156),
                                pal.focus_blue,
                            );
                            if resp.clicked() && !is_sel {
                                self.temp_unit = u_opt;
                                let res = save_temp_unit(u_opt);
                                self.log_result("Temp Unit", res);
                            }
                            aorus_tooltip(
                                resp,
                                "Temperature Unit",
                                t.temp_unit_tip,
                                Some("/etc/argonunits.conf"),
                            );
                        }

                        ui.separator();

                        // 2. Language (PT | EN)
                        for (lang_opt, short_lbl) in
                            [(Language::EnUs, "🇺🇸 EN"), (Language::PtBr, "🇧🇷 PT")]
                        {
                            let is_sel = self.lang == lang_opt;
                            let resp = segmented_pill(
                                ui,
                                short_lbl,
                                is_sel,
                                Color32::from_rgb(28, 108, 156),
                                pal.focus_blue,
                            );
                            if resp.clicked() && !is_sel {
                                self.lang = lang_opt;
                            }
                            aorus_tooltip(resp, "Language / Idioma", t.lang_selector_tip, None);
                        }

                        ui.separator();

                        // 3. Theme Model Switcher (KDE Breeze | AORUS v1)
                        self.draw_theme_switcher_pills(ui, ctx);
                    });
            });
        });
    }

    fn draw_top_bar(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        let t = texts(self.lang);

        // Top Row: Brand Title + Unified Header Utility Strip (Temp Unit | Language | Daemon Badge)
        ui.horizontal_wrapped(|ui| {
            let (logo_rect, _) = ui.allocate_exact_size(Vec2::new(32.0, 32.0), Sense::hover());
            let painter = ui.painter();
            painter.rect_filled(logo_rect, CornerRadius::same(6), uc().btn_orange);
            painter.rect_stroke(
                logo_rect,
                CornerRadius::same(6),
                Stroke::new(1.5_f32, uc().orange),
                StrokeKind::Inside,
            );
            painter.text(
                logo_rect.center(),
                Align2::CENTER_CENTER,
                "⚡",
                FontId::new(18.0, FontFamily::Proportional),
                Color32::WHITE,
            );

            ui.label(
                RichText::new("ARGON")
                    .size(18.5)
                    .strong()
                    .color(Color32::WHITE),
            );
            ui.label(
                RichText::new("CONTROL CENTER")
                    .size(18.5)
                    .strong()
                    .color(uc().orange),
            );
            let board = crate::board::current();
            ui.label(
                RichText::new(format!("• {}", board.summary()))
                    .size(12.5)
                    .color(uc().text_secondary),
            )
            .on_hover_text(format!("{}\n\n{}", t.board_detect_tip, board.details()));

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // Unified Utility Pill Container
                egui::Frame::new()
                    .fill(uc().bg_subgroup)
                    .stroke(Stroke::new(1.0_f32, uc().border_subtle))
                    .corner_radius(CornerRadius::same(6))
                    .inner_margin(egui::Margin::symmetric(8, 4))
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;

                        // 1. Daemon Status Badge with Click-to-Manage & Tooltip
                        let (daemon_col, daemon_txt) = if self.telemetry.argon_service_active {
                            (uc().green, t.daemon_online)
                        } else {
                            (uc().red, t.daemon_offline)
                        };
                        let badge_resp = ui.add(
                            egui::Button::new(
                                RichText::new(daemon_txt)
                                    .size(11.5)
                                    .strong()
                                    .color(daemon_col),
                            )
                            .frame(false),
                        );
                        if badge_resp.clicked() {
                            self.active_tab = ActiveTab::ServicesAndConfig;
                        }
                        aorus_tooltip(
                            badge_resp,
                            "Systemd Daemon (argononed.service)",
                            t.daemon_badge_tip,
                            Some("/lib/systemd/system/argononed.service"),
                        );

                        ui.separator();

                        // 2. Compact Language Segmented Switch (PT | EN)
                        for (lang_opt, short_lbl) in
                            [(Language::EnUs, "🇺🇸 EN"), (Language::PtBr, "🇧🇷 PT")]
                        {
                            let is_sel = self.lang == lang_opt;
                            let resp =
                                segmented_pill(ui, short_lbl, is_sel, uc().btn_cyan, uc().cyan);
                            if resp.clicked() && !is_sel {
                                self.lang = lang_opt;
                            }
                            aorus_tooltip(resp, "Language / Idioma", t.lang_selector_tip, None);
                        }

                        ui.separator();

                        // 3. Compact Temp Unit Segmented Switch (°C | °F)
                        for (u_opt, u_lbl) in
                            [(TempUnit::Fahrenheit, "°F"), (TempUnit::Celsius, "°C")]
                        {
                            let is_sel = self.temp_unit == u_opt;
                            let resp =
                                segmented_pill(ui, u_lbl, is_sel, uc().btn_orange, uc().orange);
                            if resp.clicked() && !is_sel {
                                self.temp_unit = u_opt;
                                let res = save_temp_unit(u_opt);
                                self.log_result("Temp Unit", res);
                            }
                            aorus_tooltip(
                                resp,
                                "Temperature Unit",
                                t.temp_unit_tip,
                                Some("/etc/argonunits.conf"),
                            );
                        }

                        ui.separator();

                        // 4. Theme Model Switcher (KDE Breeze | AORUS v1)
                        self.draw_theme_switcher_pills(ui, ctx);
                    });
            });
        });

        ui.add_space(6.0);

        // Second Row: Unified Segmented Navigation Strip
        egui::Frame::new()
            .fill(uc().bg_subgroup)
            .stroke(Stroke::new(1.0_f32, uc().border_subtle))
            .corner_radius(CornerRadius::same(6))
            .inner_margin(egui::Margin::symmetric(6, 4))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    let tabs = [
                        (ActiveTab::Dashboard, t.tab_dashboard, t.tab_dashboard_tip),
                        (ActiveTab::SmartFan, t.tab_smart_fan, t.tab_smart_fan_tip),
                        (
                            ActiveTab::Overclocking,
                            t.tab_overclock,
                            t.tab_overclock_tip,
                        ),
                        (ActiveTab::Features, t.tab_features, t.tab_features_tip),
                        (ActiveTab::GpioLab, t.tab_gpio_lab, t.tab_gpio_lab_tip),
                        (
                            ActiveTab::ServicesAndConfig,
                            t.tab_services,
                            t.tab_services_tip,
                        ),
                    ];

                    for (tab, title, tip) in tabs {
                        let is_active = self.active_tab == tab;
                        let fill = if is_active {
                            uc().btn_orange
                        } else {
                            Color32::TRANSPARENT
                        };
                        let stroke = if is_active {
                            Stroke::new(1.5_f32, uc().orange)
                        } else {
                            Stroke::NONE
                        };

                        let btn =
                            egui::Button::new(RichText::new(title).size(13.0).strong().color(
                                if is_active {
                                    Color32::WHITE
                                } else {
                                    uc().text_secondary
                                },
                            ))
                            .fill(fill)
                            .stroke(stroke)
                            .min_size(Vec2::new(135.0, 30.0));

                        let resp = ui.add(btn);
                        if resp.clicked() {
                            self.active_tab = tab;
                        }
                        aorus_tooltip(resp, title, tip, None);
                    }
                });
            });
    }

    fn draw_bottom_bar(&mut self, ui: &mut Ui) {
        let t = texts(self.lang);
        ui.horizontal_wrapped(|ui| {
            let msg = self.status_message.as_deref().unwrap_or(t.ready_status);
            let status_color = if self.status_is_error {
                uc().red
            } else {
                uc().cyan
            };
            ui.label(RichText::new(msg).size(12.5).strong().color(status_color));

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let priv_label = if is_root() {
                    t.mode_root
                } else {
                    t.mode_polkit
                };
                let resp = ui.add(
                    egui::Label::new(
                        RichText::new(format!(
                            "IP: {}  |  {}: {}  |  {}: {} ⓘ",
                            self.telemetry.ip_address,
                            t.uptime_label,
                            self.telemetry.format_uptime(),
                            t.mode_label,
                            priv_label
                        ))
                        .size(12.0)
                        .color(uc().text_secondary),
                    )
                    .sense(Sense::hover()),
                );
                aorus_tooltip(resp, t.mode_label, t.mode_tip, Some("/usr/bin/pkexec"));
            });
        });
    }

    // =========================================================================
    // TAB 1A: KDE COMPACT ALL-IN-ONE OVERVIEW (NO-SCROLL TELEMETRY + CONTROLS)
    // =========================================================================
    fn draw_kde_compact_overview_tab(&mut self, ui: &mut Ui) {
        let t = texts(self.lang);
        let lang = self.lang;
        let pal = self.kde_palette;

        // 1. Compact Donut Ring Strip (CPU | Temp | Cooler | RAM)
        aorus_card_with_tip(
            ui,
            t.kde_overview_sensors_title,
            t.tab_dashboard_tip,
            pal.focus_blue,
            |ui| {
                let cpu_frac = (self.telemetry.cpu_total_usage_pct / 100.0).clamp(0.0, 1.0);
                let cpu_val = format!("{:.1}%", self.telemetry.cpu_total_usage_pct);
                let cpu_sub = format!(
                    "{} MHz • {}",
                    self.telemetry.cpu_cur_freq_mhz, self.telemetry.cpu_governor
                );

                let temp_c = self.telemetry.cpu_temp_c;
                let temp_frac = ((temp_c - 25.0) / 60.0).clamp(0.0, 1.0);
                let temp_col = if temp_c >= 70.0 {
                    pal.negative_red
                } else if temp_c >= 58.0 {
                    pal.neutral_orange
                } else {
                    pal.positive_green
                };
                let temp_val = self.temp_unit.format_temp(temp_c);
                let temp_sub = format!(
                    "{}: {}",
                    t.peak_max,
                    self.temp_unit.format_temp(self.telemetry.max_temp_c)
                );

                let fan_pct = self.telemetry.target_fan_speed_pct;
                let fan_frac = (fan_pct as f32 / 100.0).clamp(0.0, 1.0);
                let fan_val = format!("{fan_pct}%");
                let fan_sub = if self.telemetry.manual_fan_override.is_some() {
                    t.fan_mode_manual
                } else {
                    t.fan_mode_auto
                };

                let ram_frac = (self.telemetry.ram_usage_pct / 100.0).clamp(0.0, 1.0);
                let ram_val = format!(
                    "{:.1} / {:.1} GB",
                    self.telemetry.ram_used_mb as f32 / 1024.0,
                    self.telemetry.ram_total_mb as f32 / 1024.0
                );
                let ram_sub = format!(
                    "↓{:.0} ↑{:.0} KB/s",
                    self.telemetry.net_rx_kbps, self.telemetry.net_tx_kbps
                );

                ui.columns(4, |cols| {
                    draw_kde_donut_chart_sized(
                        &mut cols[0],
                        t.gauge_cpu_clock,
                        &cpu_val,
                        &cpu_sub,
                        cpu_frac,
                        pal.focus_blue,
                        &pal,
                        true,
                    );
                    draw_kde_donut_chart_sized(
                        &mut cols[1],
                        t.gauge_cpu_temp,
                        &temp_val,
                        &temp_sub,
                        temp_frac,
                        temp_col,
                        &pal,
                        true,
                    );
                    draw_kde_donut_chart_sized(
                        &mut cols[2],
                        t.gauge_fan_speed,
                        &fan_val,
                        fan_sub,
                        fan_frac,
                        Color32::from_rgb(26, 188, 156),
                        &pal,
                        true,
                    );
                    draw_kde_donut_chart_sized(
                        &mut cols[3],
                        t.gauge_memory,
                        &ram_val,
                        &ram_sub,
                        ram_frac,
                        pal.visited_purple,
                        &pal,
                        true,
                    );
                });
            },
        );

        ui.add_space(8.0);

        // 2. Integrated Quick Control Deck (Fan + Overclock + Argononed Daemon + Hardware)
        aorus_card_with_tip(
            ui,
            t.kde_compact_controls_title,
            t.kde_compact_controls_tip,
            pal.positive_green,
            |ui| {
                ui.columns(3, |cols| {
                    // Column 1: Smart Fan Direct Controls
                    cols[0].label(
                        RichText::new("🌀 Smart Fan & I2C PWM")
                            .size(12.5)
                            .strong()
                            .color(pal.focus_blue),
                    );
                    cols[0].add_space(4.0);
                    cols[0].horizontal_wrapped(|ui| {
                        for (lbl, spd) in [
                            (t.quick_auto, None),
                            ("0%", Some(0u8)),
                            ("35%", Some(35)),
                            ("65%", Some(65)),
                            ("100%", Some(100)),
                        ] {
                            let is_sel = self.telemetry.manual_fan_override == spd;
                            if segmented_pill(
                                ui,
                                lbl,
                                is_sel,
                                Color32::from_rgb(24, 98, 145),
                                pal.focus_blue,
                            )
                            .clicked()
                            {
                                self.telemetry.manual_fan_override = spd;
                                match spd {
                                    Some(pct) => {
                                        self.manual_fan_slider = pct;
                                        let res = set_immediate_fan_speed(pct);
                                        self.log_result(&format!("Fan PWM {pct}%"), res);
                                    }
                                    None => {
                                        let res = control_argononed_service("restart");
                                        self.log_result("Fan Auto Curve", res);
                                    }
                                }
                            }
                        }
                    });
                    cols[0].add_space(4.0);
                    cols[0].horizontal_wrapped(|ui| {
                        egui::ComboBox::from_id_salt("compact_fan_preset")
                            .width(145.0)
                            .selected_text(self.selected_fan_preset.label(lang))
                            .show_ui(ui, |ui| {
                                for p in [
                                    FanPreset::Silent,
                                    FanPreset::BalancedDefault,
                                    FanPreset::Performance,
                                    FanPreset::AlwaysOnTurbo,
                                ] {
                                    if ui
                                        .selectable_label(
                                            self.selected_fan_preset == p,
                                            p.label(lang),
                                        )
                                        .clicked()
                                    {
                                        self.selected_fan_preset = p;
                                        self.cpu_fan_curve = p.points();
                                    }
                                }
                            });
                        if aorus_action_button(
                            ui,
                            "💾",
                            Color32::from_rgb(18, 115, 72),
                            pal.positive_green,
                        )
                        .on_hover_text(t.btn_save_fan_curve_tip)
                        .clicked()
                        {
                            let res = save_fan_curve(CPU_FAN_CONF, &self.cpu_fan_curve, "CPU");
                            self.log_result("Save CPU Fan Curve", res);
                        }
                    });

                    // Column 2: Overclocking & Live CPUFreq Controls
                    cols[1].label(
                        RichText::new("🔥 Overclock & CPUFreq")
                            .size(12.5)
                            .strong()
                            .color(pal.neutral_orange),
                    );
                    cols[1].add_space(4.0);
                    cols[1].horizontal_wrapped(|ui| {
                        for preset in [
                            OcPreset::EcoPowerSave,
                            OcPreset::StockBoost,
                            OcPreset::OcGaming,
                            OcPreset::OcExtreme,
                        ] {
                            let is_sel = self.selected_oc_preset == preset;
                            let short_lbl = preset.short_label();
                            if segmented_pill(
                                ui,
                                &short_lbl,
                                is_sel,
                                Color32::from_rgb(155, 72, 12),
                                pal.neutral_orange,
                            )
                            .clicked()
                            {
                                self.selected_oc_preset = preset;
                                self.boot_oc.apply_preset(preset);
                                self.runtime_max_mhz =
                                    self.boot_oc.arm_freq_mhz.min(self.telemetry.cpu_hw_max_mhz);
                                self.runtime_governor = if preset == OcPreset::EcoPowerSave {
                                    "powersave".to_string()
                                } else if preset == OcPreset::OcExtreme {
                                    "performance".to_string()
                                } else {
                                    "schedutil".to_string()
                                };
                                let res = apply_runtime_cpufreq(
                                    &self.runtime_governor,
                                    self.runtime_min_mhz,
                                    self.runtime_max_mhz,
                                    self.runtime_boost,
                                );
                                self.log_result("1-Click CPUFreq", res);
                            }
                        }
                    });
                    cols[1].add_space(4.0);
                    let govs = self.telemetry.available_governors.clone();
                    cols[1].horizontal_wrapped(|ui| {
                        ui.label(RichText::new("Gov:").size(11.5).color(pal.text_inactive));
                        egui::ComboBox::from_id_salt("compact_gov_combo")
                            .width(120.0)
                            .selected_text(&self.telemetry.cpu_governor)
                            .show_ui(ui, |ui| {
                                for g in &govs {
                                    if ui
                                        .selectable_label(&self.telemetry.cpu_governor == g, g)
                                        .clicked()
                                    {
                                        self.runtime_governor = g.clone();
                                        let res = apply_runtime_cpufreq(
                                            g,
                                            self.telemetry.cpu_min_freq_mhz,
                                            self.telemetry.cpu_max_freq_mhz,
                                            self.telemetry.cpu_boost_enabled,
                                        );
                                        self.log_result(&format!("Governor -> {g}"), res);
                                    }
                                }
                            });
                    });

                    // Column 3: Argononed Guardian & Quick Lab Access
                    cols[2].label(
                        RichText::new("🛡 Guardião argononed & Lab")
                            .size(12.5)
                            .strong()
                            .color(pal.positive_green),
                    );
                    cols[2].add_space(4.0);
                    cols[2].horizontal_wrapped(|ui| {
                        if aorus_action_button(
                            ui,
                            t.btn_start,
                            Color32::from_rgb(18, 115, 72),
                            pal.positive_green,
                        )
                        .on_hover_text(t.btn_start_tip)
                        .clicked()
                        {
                            let res = control_argononed_service("start");
                            self.log_result("Start argononed", res);
                            self.telemetry.last_slow_refresh = None;
                        }
                        if aorus_action_button(
                            ui,
                            t.btn_restart,
                            Color32::from_rgb(24, 98, 145),
                            pal.focus_blue,
                        )
                        .on_hover_text(t.btn_restart_tip)
                        .clicked()
                        {
                            let res = control_argononed_service("restart");
                            self.log_result("Restart argononed", res);
                            self.telemetry.last_slow_refresh = None;
                        }
                        if ui
                            .button(t.btn_stop)
                            .on_hover_text(t.btn_stop_tip)
                            .clicked()
                        {
                            let res = control_argononed_service("stop");
                            self.log_result("Stop argononed", res);
                            self.telemetry.last_slow_refresh = None;
                        }
                    });
                    cols[2].add_space(4.0);
                    cols[2].horizontal_wrapped(|ui| {
                        if ui.small_button("🔬 GPIO & Serial Lab").clicked() {
                            self.active_tab = ActiveTab::GpioLab;
                        }
                        if ui.small_button("🛠 I2C/OLED/IR").clicked() {
                            self.active_tab = ActiveTab::Features;
                        }
                    });
                });
            },
        );

        ui.add_space(8.0);

        // 3. Compact Dual Area Charts (Thermal/CPU/Fan + Memory/Network) & Per-Core/Disk Bars
        responsive_two_columns(
            ui,
            self,
            |app, left_ui| {
                aorus_card_with_tip(
                    left_ui,
                    t.kde_thermal_history_title,
                    t.card_telemetry_tip,
                    pal.focus_blue,
                    |ui| {
                        ui.horizontal_wrapped(|ui| {
                            legend_badge(ui, t.legend_temp, pal.neutral_orange);
                            ui.add_space(6.0);
                            legend_badge(ui, t.legend_fan_pwm, pal.positive_green);
                            ui.add_space(6.0);
                            legend_badge(ui, t.legend_cpu_load, pal.focus_blue);
                        });
                        ui.add_space(4.0);
                        draw_kde_area_line_chart(
                            ui,
                            135.0,
                            &[
                                (&app.telemetry.cpu_history, pal.focus_blue, 100.0),
                                (&app.telemetry.fan_history, pal.positive_green, 100.0),
                                (&app.telemetry.temp_history, pal.neutral_orange, 100.0),
                            ],
                            &pal,
                        );
                    },
                );
            },
            |app, right_ui| {
                aorus_card(
                    right_ui,
                    t.kde_mem_net_history_title,
                    pal.visited_purple,
                    |ui| {
                        let max_net = app
                            .telemetry
                            .net_rx_history
                            .iter()
                            .chain(app.telemetry.net_tx_history.iter())
                            .copied()
                            .fold(100.0_f32, f32::max);
                        ui.horizontal_wrapped(|ui| {
                            legend_badge(ui, "RAM (%)", pal.visited_purple);
                            ui.add_space(6.0);
                            legend_badge(
                                ui,
                                &format!("↓ RX ({:.0} KB/s)", app.telemetry.net_rx_kbps),
                                pal.focus_blue,
                            );
                            ui.add_space(6.0);
                            legend_badge(
                                ui,
                                &format!("↑ TX ({:.0} KB/s)", app.telemetry.net_tx_kbps),
                                pal.positive_green,
                            );
                        });
                        ui.add_space(4.0);
                        draw_kde_area_line_chart(
                            ui,
                            135.0,
                            &[
                                (&app.telemetry.ram_history, pal.visited_purple, 100.0),
                                (&app.telemetry.net_rx_history, pal.focus_blue, max_net),
                                (&app.telemetry.net_tx_history, pal.positive_green, max_net),
                            ],
                            &pal,
                        );
                    },
                );
            },
        );
    }

    // =========================================================================
    // TAB 1B: KDE SYSTEM MONITOR FULL DASHBOARD (overview.page + history.page)
    // =========================================================================
    fn draw_kde_dashboard_tab(&mut self, ui: &mut Ui) {
        let t = texts(self.lang);
        let pal = self.kde_palette;

        // 1. Unified KDE overview.page Donut Ring Strip (4 piechart sensors with subtle dividers)
        aorus_card_with_tip(
            ui,
            t.kde_overview_sensors_title,
            t.tab_dashboard_tip,
            pal.focus_blue,
            |ui| {
                let cpu_frac = (self.telemetry.cpu_total_usage_pct / 100.0).clamp(0.0, 1.0);
                let cpu_val = format!("{:.1}%", self.telemetry.cpu_total_usage_pct);
                let cpu_sub = format!(
                    "{} MHz • {}",
                    self.telemetry.cpu_cur_freq_mhz, self.telemetry.cpu_governor
                );

                let temp_c = self.telemetry.cpu_temp_c;
                let temp_frac = ((temp_c - 25.0) / 60.0).clamp(0.0, 1.0);
                let temp_col = if temp_c >= 70.0 {
                    pal.negative_red
                } else if temp_c >= 58.0 {
                    pal.neutral_orange
                } else {
                    pal.positive_green
                };
                let temp_val = self.temp_unit.format_temp(temp_c);
                let temp_sub = format!(
                    "{}: {}",
                    t.peak_max,
                    self.temp_unit.format_temp(self.telemetry.max_temp_c)
                );

                let fan_pct = self.telemetry.target_fan_speed_pct;
                let fan_frac = (fan_pct as f32 / 100.0).clamp(0.0, 1.0);
                let fan_val = format!("{fan_pct}%");
                let fan_sub = if self.telemetry.manual_fan_override.is_some() {
                    t.fan_mode_manual
                } else {
                    t.fan_mode_auto
                };

                let ram_frac = (self.telemetry.ram_usage_pct / 100.0).clamp(0.0, 1.0);
                let ram_val = format!(
                    "{:.1} / {:.1} GB",
                    self.telemetry.ram_used_mb as f32 / 1024.0,
                    self.telemetry.ram_total_mb as f32 / 1024.0
                );
                let ram_sub = format!("Swap: {} MB", self.telemetry.swap_used_mb);

                if ui.available_width() >= 760.0 {
                    ui.columns(4, |cols| {
                        let r0 = draw_kde_donut_chart(
                            &mut cols[0],
                            t.gauge_cpu_clock,
                            &cpu_val,
                            &cpu_sub,
                            cpu_frac,
                            pal.focus_blue,
                            &pal,
                        );
                        aorus_tooltip(
                            r0,
                            t.gauge_cpu_clock,
                            t.gauge_cpu_clock_tip,
                            Some("/proc/stat & cpufreq"),
                        );

                        let r1 = draw_kde_donut_chart(
                            &mut cols[1],
                            t.gauge_cpu_temp,
                            &temp_val,
                            &temp_sub,
                            temp_frac,
                            temp_col,
                            &pal,
                        );
                        aorus_tooltip(
                            r1,
                            t.gauge_cpu_temp,
                            t.gauge_cpu_temp_tip,
                            Some("/sys/class/thermal/thermal_zone0/temp"),
                        );

                        let r2 = draw_kde_donut_chart(
                            &mut cols[2],
                            t.gauge_fan_speed,
                            &fan_val,
                            fan_sub,
                            fan_frac,
                            Color32::from_rgb(26, 188, 156),
                            &pal,
                        );
                        aorus_tooltip(
                            r2,
                            t.gauge_fan_speed,
                            t.gauge_fan_speed_tip,
                            Some("/dev/i2c-1 @ 0x1a"),
                        );

                        let r3 = draw_kde_donut_chart(
                            &mut cols[3],
                            t.gauge_memory,
                            &ram_val,
                            &ram_sub,
                            ram_frac,
                            pal.visited_purple,
                            &pal,
                        );
                        aorus_tooltip(
                            r3,
                            t.gauge_memory,
                            t.gauge_memory_tip,
                            Some("/proc/meminfo"),
                        );
                    });
                } else {
                    ui.columns(2, |cols| {
                        draw_kde_donut_chart(
                            &mut cols[0],
                            t.gauge_cpu_clock,
                            &cpu_val,
                            &cpu_sub,
                            cpu_frac,
                            pal.focus_blue,
                            &pal,
                        );
                        draw_kde_donut_chart(
                            &mut cols[1],
                            t.gauge_cpu_temp,
                            &temp_val,
                            &temp_sub,
                            temp_frac,
                            temp_col,
                            &pal,
                        );
                    });
                    ui.separator();
                    ui.columns(2, |cols| {
                        draw_kde_donut_chart(
                            &mut cols[0],
                            t.gauge_fan_speed,
                            &fan_val,
                            fan_sub,
                            fan_frac,
                            Color32::from_rgb(26, 188, 156),
                            &pal,
                        );
                        draw_kde_donut_chart(
                            &mut cols[1],
                            t.gauge_memory,
                            &ram_val,
                            &ram_sub,
                            ram_frac,
                            pal.visited_purple,
                            &pal,
                        );
                    });
                }
            },
        );

        ui.add_space(12.0);

        // 2. KDE history.page Translucent Area Line Charts + Per-Core Bars
        responsive_two_columns(
            ui,
            self,
            |app, left_ui| {
                aorus_card_with_tip(
                    left_ui,
                    t.kde_thermal_history_title,
                    t.card_telemetry_tip,
                    pal.focus_blue,
                    |ui| {
                        ui.horizontal_wrapped(|ui| {
                            legend_badge(ui, t.legend_temp, pal.neutral_orange);
                            ui.add_space(8.0);
                            legend_badge(ui, t.legend_fan_pwm, pal.positive_green);
                            ui.add_space(8.0);
                            legend_badge(ui, t.legend_cpu_load, pal.focus_blue);
                        });
                        ui.add_space(6.0);
                        draw_kde_area_line_chart(
                            ui,
                            185.0,
                            &[
                                (&app.telemetry.cpu_history, pal.focus_blue, 100.0),
                                (&app.telemetry.fan_history, pal.positive_green, 100.0),
                                (&app.telemetry.temp_history, pal.neutral_orange, 100.0),
                            ],
                            &pal,
                        );
                    },
                );
            },
            |app, right_ui| {
                app.draw_core_load_card(right_ui);
            },
        );

        ui.add_space(12.0);

        // 3. KDE history.page Memory (%) & Network Rate (KB/s) Area Chart + Storage/Specs
        aorus_card(ui, t.kde_mem_net_history_title, pal.visited_purple, |ui| {
            let max_net = self
                .telemetry
                .net_rx_history
                .iter()
                .chain(self.telemetry.net_tx_history.iter())
                .copied()
                .fold(100.0_f32, f32::max);
            ui.horizontal_wrapped(|ui| {
                legend_badge(ui, "RAM (%)", pal.visited_purple);
                ui.add_space(8.0);
                legend_badge(
                    ui,
                    &format!("↓ Download ({:.1} KB/s)", self.telemetry.net_rx_kbps),
                    pal.focus_blue,
                );
                ui.add_space(8.0);
                legend_badge(
                    ui,
                    &format!("↑ Upload ({:.1} KB/s)", self.telemetry.net_tx_kbps),
                    pal.positive_green,
                );
            });
            ui.add_space(6.0);
            draw_kde_area_line_chart(
                ui,
                145.0,
                &[
                    (&self.telemetry.ram_history, pal.visited_purple, 100.0),
                    (&self.telemetry.net_rx_history, pal.focus_blue, max_net),
                    (&self.telemetry.net_tx_history, pal.positive_green, max_net),
                ],
                &pal,
            );
        });

        ui.add_space(12.0);

        // 4. Storage Horizontal Bars + Hardware & Network Specs
        responsive_two_columns(
            ui,
            self,
            |app, left_ui| {
                app.draw_storage_card(left_ui);
            },
            |app, right_ui| {
                app.draw_specs_card(right_ui);
            },
        );
    }

    // =========================================================================
    // TAB 1B: DASHBOARD (SAVED AORUS v1 MONITOR)
    // =========================================================================
    fn draw_dashboard_tab(&mut self, ui: &mut Ui) {
        let t = texts(self.lang);
        let avail_w = ui.available_width();

        if avail_w >= 960.0 {
            ui.columns(4, |cols| {
                self.draw_gauge_cpu_clock(&mut cols[0]);
                self.draw_gauge_cpu_temp(&mut cols[1]);
                self.draw_gauge_fan(&mut cols[2]);
                self.draw_gauge_memory(&mut cols[3]);
            });
        } else if avail_w >= 500.0 {
            ui.columns(2, |cols| {
                self.draw_gauge_cpu_clock(&mut cols[0]);
                self.draw_gauge_cpu_temp(&mut cols[1]);
            });
            ui.add_space(10.0);
            ui.columns(2, |cols| {
                self.draw_gauge_fan(&mut cols[0]);
                self.draw_gauge_memory(&mut cols[1]);
            });
        } else {
            self.draw_gauge_cpu_clock(ui);
            ui.add_space(8.0);
            self.draw_gauge_cpu_temp(ui);
            ui.add_space(8.0);
            self.draw_gauge_fan(ui);
            ui.add_space(8.0);
            self.draw_gauge_memory(ui);
        }

        ui.add_space(12.0);

        responsive_two_columns(
            ui,
            self,
            |app, left_ui| {
                aorus_card_with_tip(
                    left_ui,
                    t.card_telemetry_60s,
                    t.card_telemetry_tip,
                    uc().cyan,
                    |ui| {
                        ui.horizontal_wrapped(|ui| {
                            legend_badge(ui, t.legend_temp, uc().cyan);
                            ui.add_space(8.0);
                            legend_badge(ui, t.legend_cpu_load, uc().orange);
                            ui.add_space(8.0);
                            legend_badge(ui, t.legend_fan_pwm, uc().green);
                        });
                        ui.add_space(6.0);
                        draw_multi_telemetry_chart(
                            ui,
                            &app.telemetry.temp_history,
                            &app.telemetry.cpu_history,
                            &app.telemetry.fan_history,
                        );
                    },
                );
            },
            |app, right_ui| {
                app.draw_core_load_card(right_ui);
            },
        );

        ui.add_space(12.0);

        responsive_two_columns(
            ui,
            self,
            |app, left_ui| {
                app.draw_storage_card(left_ui);
            },
            |app, right_ui| {
                app.draw_specs_card(right_ui);
            },
        );
    }

    fn draw_gauge_cpu_clock(&self, ui: &mut Ui) {
        let t = texts(self.lang);
        aorus_card_with_tip(
            ui,
            t.gauge_cpu_clock,
            t.gauge_cpu_clock_tip,
            uc().orange,
            |ui| {
                let max_scale = self.telemetry.cpu_hw_max_mhz.max(2200) as f32;
                let ratio = (self.telemetry.cpu_cur_freq_mhz as f32 / max_scale).clamp(0.0, 1.0);
                let resp = draw_radial_gauge(
                    ui,
                    ratio,
                    &format!("{}", self.telemetry.cpu_cur_freq_mhz),
                    "MHz",
                    &format!("GOV: {}", self.telemetry.cpu_governor.to_uppercase()),
                    uc().orange,
                );
                aorus_tooltip(
                    resp,
                    t.gauge_cpu_clock,
                    t.gauge_cpu_clock_tip,
                    Some("/sys/devices/system/cpu/cpu0/cpufreq/scaling_cur_freq"),
                );
            },
        );
    }

    fn draw_gauge_cpu_temp(&self, ui: &mut Ui) {
        let t = texts(self.lang);
        let temp_color = if self.telemetry.cpu_temp_c >= 75.0 {
            uc().red
        } else if self.telemetry.cpu_temp_c >= 60.0 {
            uc().amber
        } else {
            uc().cyan
        };
        aorus_card_with_tip(
            ui,
            t.gauge_cpu_temp,
            t.gauge_cpu_temp_tip,
            temp_color,
            |ui| {
                let ratio = ((self.telemetry.cpu_temp_c - 25.0) / 65.0).clamp(0.0, 1.0);
                let display_val = match self.temp_unit {
                    TempUnit::Celsius => format!("{:.1}", self.telemetry.cpu_temp_c),
                    TempUnit::Fahrenheit => {
                        format!("{:.1}", self.telemetry.cpu_temp_c * 9.0 / 5.0 + 32.0)
                    }
                };
                let resp = draw_radial_gauge(
                    ui,
                    ratio,
                    &display_val,
                    self.temp_unit.symbol(),
                    &format!(
                        "{}: {}",
                        t.peak_max,
                        self.temp_unit.format_temp(self.telemetry.max_temp_c)
                    ),
                    temp_color,
                );
                aorus_tooltip(
                    resp,
                    t.gauge_cpu_temp,
                    t.gauge_cpu_temp_tip,
                    Some("/sys/class/thermal/thermal_zone0/temp"),
                );
            },
        );
    }

    fn draw_gauge_fan(&self, ui: &mut Ui) {
        let t = texts(self.lang);
        aorus_card_with_tip(
            ui,
            t.gauge_fan_speed,
            t.gauge_fan_speed_tip,
            uc().green,
            |ui| {
                let ratio = (self.telemetry.target_fan_speed_pct as f32 / 100.0).clamp(0.0, 1.0);
                let mode_str = if self.telemetry.manual_fan_override.is_some() {
                    t.fan_mode_manual
                } else {
                    t.fan_mode_auto
                };
                let resp = draw_radial_gauge(
                    ui,
                    ratio,
                    &format!("{}", self.telemetry.target_fan_speed_pct),
                    "% PWM",
                    mode_str,
                    uc().green,
                );
                aorus_tooltip(
                    resp,
                    t.gauge_fan_speed,
                    t.gauge_fan_speed_tip,
                    Some("/dev/i2c-1 @ 0x1a (reg 0x80)"),
                );
            },
        );
    }

    fn draw_gauge_memory(&self, ui: &mut Ui) {
        let t = texts(self.lang);
        aorus_card_with_tip(ui, t.gauge_memory, t.gauge_memory_tip, uc().amber, |ui| {
            let ratio = (self.telemetry.ram_usage_pct / 100.0).clamp(0.0, 1.0);
            let resp = draw_radial_gauge(
                ui,
                ratio,
                &format!("{:.0}", self.telemetry.ram_usage_pct),
                "% RAM",
                &format!(
                    "{} / {} MB",
                    self.telemetry.ram_used_mb, self.telemetry.ram_total_mb
                ),
                uc().amber,
            );
            aorus_tooltip(
                resp,
                t.gauge_memory,
                t.gauge_memory_tip,
                Some("/proc/meminfo"),
            );
        });
    }

    fn draw_core_load_card(&mut self, ui: &mut Ui) {
        let t = texts(self.lang);
        aorus_card_with_tip(
            ui,
            t.card_core_load,
            t.card_core_load_tip,
            uc().orange,
            |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(
                        RichText::new(format!(
                            "{}: {:.1}%",
                            t.total_cpu_load, self.telemetry.cpu_total_usage_pct
                        ))
                        .strong()
                        .color(uc().text_primary),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let (v_txt, v_col) = if self.telemetry.undervoltage_alarm {
                            (t.voltage_alert, uc().red)
                        } else {
                            (t.voltage_stable, uc().green)
                        };
                        let v_resp = ui.add(
                            egui::Label::new(
                                RichText::new(format!("{v_txt} ⓘ"))
                                    .size(12.5)
                                    .strong()
                                    .color(v_col),
                            )
                            .sense(Sense::hover()),
                        );
                        aorus_tooltip(
                            v_resp,
                            "5V Power Rail Sensor",
                            t.voltage_tip,
                            Some("/sys/class/hwmon/hwmon1/in0_lcrit_alarm"),
                        );
                    });
                });
                ui.add_space(4.0);

                for (core_name, pct) in &self.telemetry.cpu_core_usage_pct {
                    draw_progress_bar_row(
                        ui,
                        core_name,
                        *pct / 100.0,
                        &format!("{pct:.1}%  ({} MHz)", self.telemetry.cpu_cur_freq_mhz),
                        uc().orange,
                    );
                    ui.add_space(3.0);
                }

                ui.add_space(4.0);
                ui.horizontal_wrapped(|ui| {
                    ui.label(
                        RichText::new(format!(
                            "Load Avg: {:.2} / {:.2} / {:.2}",
                            self.telemetry.load_avg.0,
                            self.telemetry.load_avg.1,
                            self.telemetry.load_avg.2
                        ))
                        .size(12.0)
                        .color(uc().text_secondary),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            RichText::new(format!(
                                "{}: ⬇ {:.1} KB/s   ⬆ {:.1} KB/s",
                                t.net_label, self.telemetry.net_rx_kbps, self.telemetry.net_tx_kbps
                            ))
                            .size(12.0)
                            .strong()
                            .color(uc().cyan),
                        );
                    });
                });

                ui.add_space(6.0);

                // Grouped Quick Control Strip (Segmented Fan Mode + Governor Selector)
                egui::Frame::new()
                    .fill(uc().bg_subgroup)
                    .stroke(Stroke::new(1.0_f32, uc().border_subtle))
                    .corner_radius(CornerRadius::same(5))
                    .inner_margin(egui::Margin::symmetric(10, 6))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal_wrapped(|ui| {
                            ui.spacing_mut().item_spacing.x = 5.0;
                            ui.label(
                                RichText::new(t.quick_cooling_label)
                                    .size(12.0)
                                    .strong()
                                    .color(uc().text_secondary),
                            );

                            let is_auto = self.telemetry.manual_fan_override.is_none();
                            let r_auto =
                                segmented_pill(ui, t.quick_auto, is_auto, uc().btn_cyan, uc().cyan);
                            if r_auto.clicked() && !is_auto {
                                self.telemetry.manual_fan_override = None;
                                let res = control_argononed_service("restart");
                                self.log_result("Auto Fan Curve", res);
                            }
                            aorus_tooltip(r_auto, t.quick_auto, t.quick_auto_tip, None);

                            let is_off = self.telemetry.manual_fan_override == Some(0);
                            let r_off = segmented_pill(
                                ui,
                                t.quick_silent,
                                is_off,
                                uc().btn_slate,
                                uc().cyan,
                            );
                            if r_off.clicked() {
                                self.telemetry.manual_fan_override = Some(0);
                                let res = set_immediate_fan_speed(0);
                                self.log_result("Fan Silent 0%", res);
                            }
                            aorus_tooltip(r_off, t.quick_silent, t.quick_silent_tip, None);

                            let is_max = self.telemetry.manual_fan_override == Some(100);
                            let r_max = segmented_pill(
                                ui,
                                t.quick_turbo,
                                is_max,
                                uc().btn_green,
                                uc().green,
                            );
                            if r_max.clicked() {
                                self.telemetry.manual_fan_override = Some(100);
                                let res = set_immediate_fan_speed(100);
                                self.log_result("Fan Turbo 100%", res);
                            }
                            aorus_tooltip(r_max, t.quick_turbo, t.quick_turbo_tip, None);

                            ui.separator();

                            let gov_lbl = ui.add(
                                egui::Label::new(
                                    RichText::new(format!("{} ⓘ", t.quick_gov_label))
                                        .size(12.0)
                                        .strong()
                                        .color(uc().text_secondary),
                                )
                                .sense(Sense::hover()),
                            );
                            aorus_tooltip(gov_lbl, t.quick_gov_label, t.quick_gov_tip, None);

                            let mut selected_gov = self.telemetry.cpu_governor.clone();
                            let gov_options = self.telemetry.available_governors.clone();
                            egui::ComboBox::from_id_salt("quick_gov_combo")
                                .width(125.0)
                                .selected_text(
                                    RichText::new(selected_gov.to_uppercase())
                                        .size(12.0)
                                        .strong()
                                        .color(uc().text_primary),
                                )
                                .show_ui(ui, |ui| {
                                    for g in &gov_options {
                                        if ui
                                            .selectable_value(
                                                &mut selected_gov,
                                                g.clone(),
                                                g.to_uppercase(),
                                            )
                                            .clicked()
                                        {
                                            self.runtime_governor = g.clone();
                                            let res = apply_runtime_cpufreq(
                                                g,
                                                self.telemetry.cpu_min_freq_mhz,
                                                self.telemetry.cpu_hw_max_mhz,
                                                self.runtime_boost,
                                            );
                                            self.log_result(&format!("Governor -> {g}"), res);
                                            self.telemetry.refresh(&self.cpu_fan_curve);
                                        }
                                    }
                                });
                        });
                    });
            },
        );
    }

    fn draw_storage_card(&self, ui: &mut Ui) {
        let t = texts(self.lang);
        aorus_card(ui, t.card_storage, uc().cyan, |ui| {
            if self.telemetry.disks.is_empty() {
                ui.label(RichText::new(t.no_disks).color(uc().text_secondary));
            } else {
                for d in &self.telemetry.disks {
                    let color = if d.usage_pct > 85.0 {
                        uc().red
                    } else {
                        uc().cyan
                    };
                    draw_progress_bar_row(
                        ui,
                        &format!("{} ({})", d.mount_point, d.device),
                        d.usage_pct / 100.0,
                        &format!(
                            "{:.1} / {:.1} GB ({:.0}%)",
                            d.used_gb, d.total_gb, d.usage_pct
                        ),
                        color,
                    );
                    ui.add_space(4.0);
                }
            }

            if !self.telemetry.raid_arrays.is_empty() {
                ui.separator();
                ui.label(RichText::new(t.raid_arrays_title).strong());
                for r in &self.telemetry.raid_arrays {
                    ui.label(format!(
                        "• {} [{}] — {} ({})",
                        r.name, r.level, r.state, r.disks
                    ));
                }
            }
        });
    }

    fn draw_specs_card(&self, ui: &mut Ui) {
        let t = texts(self.lang);
        aorus_card(ui, t.card_specs, uc().amber, |ui| {
            egui::Grid::new("sys_specs_grid")
                .num_columns(2)
                .spacing([16.0, 7.0])
                .show(ui, |ui| {
                    spec_row(ui, t.spec_board, &self.telemetry.board_model);
                    let board = crate::board::current();
                    spec_row(
                        ui,
                        t.spec_soc,
                        &format!(
                            "{} · {}",
                            board.soc.name(),
                            board
                                .ram_mb
                                .map(|m| format!("{} GB", m / 1024))
                                .unwrap_or_else(|| "—".into())
                        ),
                    );
                    spec_row(ui, t.spec_case, board.case.name());
                    spec_row(ui, t.spec_os, &self.telemetry.os_name);
                    spec_row(ui, t.spec_kernel, &self.telemetry.kernel_version);
                    spec_row(
                        ui,
                        t.spec_cpu_scale,
                        &format!(
                            "{} – {} MHz (HW Max: {} MHz)",
                            self.telemetry.cpu_min_freq_mhz,
                            self.telemetry.cpu_max_freq_mhz,
                            self.telemetry.cpu_hw_max_mhz
                        ),
                    );
                    spec_row(
                        ui,
                        t.spec_swap,
                        &format!(
                            "{} / {} MB",
                            self.telemetry.swap_used_mb, self.telemetry.swap_total_mb
                        ),
                    );
                    spec_row(
                        ui,
                        t.spec_boot_oc,
                        &format!(
                            "ARM {} MHz | GPU {} MHz | OV {}",
                            self.boot_oc.arm_freq_mhz,
                            self.boot_oc.gpu_freq_mhz,
                            self.boot_oc.over_voltage
                        ),
                    );
                });
        });
    }

    // =========================================================================
    // TAB 2: SMART FAN 5 (CLEAN GROUPED UX + RICH TOOLTIPS)
    // =========================================================================
    fn draw_smart_fan_tab(&mut self, ui: &mut Ui) {
        let t = texts(self.lang);
        responsive_two_columns(
            ui,
            self,
            |app, left_ui| {
                aorus_card(left_ui, t.card_fan_curve_editor, uc().orange, |ui| {
                    // Grouped Sensor Selector Strip
                    egui::Frame::new()
                        .fill(uc().bg_subgroup)
                        .stroke(Stroke::new(1.0_f32, uc().border_subtle))
                        .corner_radius(CornerRadius::same(5))
                        .inner_margin(egui::Margin::symmetric(8, 5))
                        .show(ui, |ui| {
                            ui.horizontal_wrapped(|ui| {
                                ui.label(
                                    RichText::new(t.target_sensor)
                                        .size(12.5)
                                        .strong()
                                        .color(uc().text_secondary),
                                );
                                let r_cpu = segmented_pill(
                                    ui,
                                    t.cpu_fan_target,
                                    app.fan_target_mode == FanTargetMode::Cpu,
                                    uc().btn_orange,
                                    uc().orange,
                                );
                                if r_cpu.clicked() {
                                    app.fan_target_mode = FanTargetMode::Cpu;
                                }
                                aorus_tooltip(
                                    r_cpu,
                                    t.cpu_fan_target,
                                    t.cpu_fan_target_tip,
                                    Some(CPU_FAN_CONF),
                                );

                                let r_hdd = segmented_pill(
                                    ui,
                                    t.hdd_fan_target,
                                    app.fan_target_mode == FanTargetMode::Hdd,
                                    uc().btn_orange,
                                    uc().orange,
                                );
                                if r_hdd.clicked() {
                                    app.fan_target_mode = FanTargetMode::Hdd;
                                }
                                aorus_tooltip(
                                    r_hdd,
                                    t.hdd_fan_target,
                                    t.hdd_fan_target_tip,
                                    Some(HDD_FAN_CONF),
                                );
                            });
                        });

                    ui.add_space(6.0);
                    ui.label(
                        RichText::new(t.fan_curve_hint)
                            .size(12.0)
                            .color(uc().text_secondary),
                    );
                    ui.add_space(6.0);

                    let active_curve = match app.fan_target_mode {
                        FanTargetMode::Cpu => &mut app.cpu_fan_curve,
                        FanTargetMode::Hdd => &mut app.hdd_fan_curve,
                    };

                    draw_interactive_fan_curve(
                        ui,
                        active_curve,
                        &mut app.dragging_point_idx,
                        app.telemetry.cpu_temp_c,
                    );

                    ui.add_space(8.0);
                    let current_eval = evaluate_fan_speed(app.telemetry.cpu_temp_c, active_curve);
                    ui.label(
                        RichText::new(format!(
                            "🌡 {:.1} °C  ➡  {}: {}% PWM",
                            app.telemetry.cpu_temp_c, t.cur_temp_to_curve, current_eval
                        ))
                        .size(13.5)
                        .strong()
                        .color(uc().cyan),
                    );
                });
            },
            |app, right_ui| {
                app.draw_fan_controls_card(right_ui);
            },
        );
    }

    fn draw_fan_controls_card(&mut self, ui: &mut Ui) {
        let t = texts(self.lang);
        let lang = self.lang;
        aorus_card(ui, t.card_fan_profiles, uc().cyan, |ui| {
            // Segmented Operating Mode Switch (Auto Curve vs Fixed Manual Override)
            egui::Frame::new()
                .fill(uc().bg_subgroup)
                .stroke(Stroke::new(1.0_f32, uc().border_subtle))
                .corner_radius(CornerRadius::same(5))
                .inner_margin(egui::Margin::symmetric(8, 5))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal_wrapped(|ui| {
                        ui.label(
                            RichText::new(t.fan_control_mode_label)
                                .size(12.5)
                                .strong()
                                .color(uc().text_secondary),
                        );
                        let r_auto = segmented_pill(
                            ui,
                            t.fan_tab_auto_curve,
                            self.fan_control_subtab == FanControlSubTab::AutoCurve,
                            uc().btn_cyan,
                            uc().cyan,
                        );
                        if r_auto.clicked() {
                            self.fan_control_subtab = FanControlSubTab::AutoCurve;
                        }
                        aorus_tooltip(
                            r_auto,
                            t.fan_tab_auto_curve,
                            t.fan_tab_auto_curve_tip,
                            Some("argononed.service"),
                        );

                        let r_man = segmented_pill(
                            ui,
                            t.fan_tab_manual_override,
                            self.fan_control_subtab == FanControlSubTab::ManualOverride,
                            uc().btn_orange,
                            uc().orange,
                        );
                        if r_man.clicked() {
                            self.fan_control_subtab = FanControlSubTab::ManualOverride;
                        }
                        aorus_tooltip(
                            r_man,
                            t.fan_tab_manual_override,
                            t.fan_tab_manual_override_tip,
                            Some("/dev/i2c-1 @ 0x1a"),
                        );
                    });
                });

            ui.add_space(10.0);
            adapt_slider_width(ui);

            match self.fan_control_subtab {
                FanControlSubTab::AutoCurve => {
                    // Alert Banner if argononed is currently stopped
                    if !self.telemetry.argon_service_active {
                        egui::Frame::new()
                            .fill(tint(uc().red))
                            .stroke(Stroke::new(1.5_f32, uc().red))
                            .corner_radius(CornerRadius::same(5))
                            .inner_margin(egui::Margin::symmetric(10, 8))
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                ui.label(
                                    RichText::new(t.smart_fan_daemon_off_warn)
                                        .size(12.5)
                                        .strong()
                                        .color(uc().amber),
                                );
                                ui.add_space(4.0);
                                let r_quick_start = aorus_action_button(
                                    ui,
                                    t.btn_quick_start_daemon,
                                    uc().btn_green,
                                    uc().green,
                                );
                                if r_quick_start.clicked() {
                                    let res = control_argononed_service("start");
                                    self.log_result("Start argononed.service", res);
                                    self.telemetry.last_slow_refresh = None;
                                }
                                aorus_tooltip(
                                    r_quick_start,
                                    t.btn_quick_start_daemon,
                                    t.btn_start_tip,
                                    Some("systemctl start argononed.service"),
                                );
                            });
                        ui.add_space(10.0);
                    }

                    // 1. Curve Preset Dropdown (replaces 4 loose buttons)
                    ui.horizontal_wrapped(|ui| {
                        label_with_tip(
                            ui,
                            t.fan_preset_label,
                            t.fan_preset_tip,
                            Some(CPU_FAN_CONF),
                        );
                        egui::ComboBox::from_id_salt("fan_preset_combo")
                            .width(230.0)
                            .selected_text(
                                RichText::new(self.selected_fan_preset.label(lang))
                                    .strong()
                                    .color(uc().text_primary),
                            )
                            .show_ui(ui, |ui| {
                                for preset in [
                                    FanPreset::Silent,
                                    FanPreset::BalancedDefault,
                                    FanPreset::Performance,
                                    FanPreset::AlwaysOnTurbo,
                                    FanPreset::Custom,
                                ] {
                                    if ui
                                        .selectable_value(
                                            &mut self.selected_fan_preset,
                                            preset,
                                            preset.label(lang),
                                        )
                                        .clicked()
                                        && preset != FanPreset::Custom
                                    {
                                        match self.fan_target_mode {
                                            FanTargetMode::Cpu => {
                                                self.cpu_fan_curve = preset.points()
                                            }
                                            FanTargetMode::Hdd => {
                                                self.hdd_fan_curve = preset.points()
                                            }
                                        }
                                    }
                                }
                            });
                    });

                    ui.separator();

                    // 2. Clean Aligned Grid Table of Trigger Stages
                    label_with_tip(ui, t.fan_step2_points, t.fan_points_tip, None);
                    ui.add_space(4.0);

                    let active_curve = match self.fan_target_mode {
                        FanTargetMode::Cpu => &mut self.cpu_fan_curve,
                        FanTargetMode::Hdd => &mut self.hdd_fan_curve,
                    };

                    let mut remove_idx: Option<usize> = None;
                    let can_remove = active_curve.len() > 1;

                    egui::Frame::new()
                        .fill(uc().bg_subgroup)
                        .stroke(Stroke::new(1.0_f32, uc().border_subtle))
                        .corner_radius(CornerRadius::same(5))
                        .inner_margin(egui::Margin::same(8))
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            egui::Grid::new("fan_stages_grid")
                                .num_columns(4)
                                .spacing([10.0, 6.0])
                                .striped(true)
                                .show(ui, |ui| {
                                    for (idx, pt) in active_curve.iter_mut().enumerate() {
                                        ui.label(
                                            RichText::new(format!(
                                                "{} #{}",
                                                t.stage_label,
                                                idx + 1
                                            ))
                                            .strong()
                                            .color(uc().amber),
                                        );
                                        if ui
                                            .add(
                                                egui::DragValue::new(&mut pt.temp_c)
                                                    .speed(0.5)
                                                    .range(1.0..=95.0)
                                                    .suffix(" °C"),
                                            )
                                            .changed()
                                        {
                                            self.selected_fan_preset = FanPreset::Custom;
                                        }
                                        if ui
                                            .add(
                                                egui::Slider::new(&mut pt.speed_pct, 0..=100)
                                                    .suffix(" % PWM"),
                                            )
                                            .changed()
                                        {
                                            self.selected_fan_preset = FanPreset::Custom;
                                        }
                                        if can_remove
                                            && ui
                                                .small_button("🗑")
                                                .on_hover_text(t.remove_point_hint)
                                                .clicked()
                                        {
                                            remove_idx = Some(idx);
                                        }
                                        ui.end_row();
                                    }
                                });
                        });

                    if let Some(idx) = remove_idx {
                        active_curve.remove(idx);
                        self.selected_fan_preset = FanPreset::Custom;
                    }

                    ui.add_space(6.0);
                    let add_resp = ui.button(t.btn_add_stage);
                    if add_resp.clicked() {
                        let (last_t, last_s) = active_curve
                            .last()
                            .map(|p| (p.temp_c, p.speed_pct))
                            .unwrap_or((50.0, 30));
                        active_curve.push(FanCurvePoint {
                            temp_c: (last_t + 5.0).min(90.0),
                            speed_pct: (last_s + 20).min(100),
                        });
                        self.selected_fan_preset = FanPreset::Custom;
                    }
                    aorus_tooltip(add_resp, t.btn_add_stage, t.btn_add_stage_tip, None);

                    ui.add_space(12.0);
                    let save_resp = aorus_full_cta_button(
                        ui,
                        t.btn_save_fan_curve,
                        uc().btn_orange,
                        uc().orange,
                    );
                    if save_resp.clicked() {
                        self.telemetry.manual_fan_override = None;
                        let (path, label, pts) = match self.fan_target_mode {
                            FanTargetMode::Cpu => (CPU_FAN_CONF, "CPU", self.cpu_fan_curve.clone()),
                            FanTargetMode::Hdd => (HDD_FAN_CONF, "HDD", self.hdd_fan_curve.clone()),
                        };
                        let res = save_fan_curve(path, &pts, label);
                        self.log_result(&format!("Save Fan Curve ({label})"), res);
                    }
                    aorus_tooltip(
                        save_resp,
                        t.btn_save_fan_curve,
                        t.btn_save_fan_curve_tip,
                        Some(CPU_FAN_CONF),
                    );
                }

                FanControlSubTab::ManualOverride => {
                    ui.label(
                        RichText::new(t.fan_manual_desc)
                            .size(12.5)
                            .color(uc().text_secondary),
                    );
                    ui.add_space(10.0);

                    // Segmented Quick Duty Bar
                    egui::Frame::new()
                        .fill(uc().bg_subgroup)
                        .stroke(Stroke::new(1.0_f32, uc().border_subtle))
                        .corner_radius(CornerRadius::same(5))
                        .inner_margin(egui::Margin::symmetric(10, 8))
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.horizontal_wrapped(|ui| {
                                ui.label(
                                    RichText::new(t.manual_presets_label)
                                        .size(12.5)
                                        .strong()
                                        .color(uc().text_secondary),
                                );
                                for (lbl, pct) in [
                                    ("0% Off", 0u8),
                                    ("30% Quiet", 30),
                                    ("55% Med", 55),
                                    ("75% High", 75),
                                    ("100% Max", 100),
                                ] {
                                    let is_sel = self.manual_fan_slider == pct;
                                    if segmented_pill(ui, lbl, is_sel, uc().btn_orange, uc().orange)
                                        .clicked()
                                    {
                                        self.manual_fan_slider = pct;
                                    }
                                }
                            });

                            ui.add_space(8.0);
                            ui.horizontal_wrapped(|ui| {
                                ui.label(RichText::new(t.manual_speed_slider).strong());
                                ui.add(
                                    egui::Slider::new(&mut self.manual_fan_slider, 0..=100)
                                        .suffix(" % PWM"),
                                );
                            });
                        });

                    ui.add_space(12.0);
                    let apply_resp = aorus_full_cta_button(
                        ui,
                        t.btn_apply_manual_fan,
                        uc().btn_green,
                        uc().green,
                    );
                    if apply_resp.clicked() {
                        let spd = self.manual_fan_slider;
                        self.telemetry.manual_fan_override = Some(spd);
                        let res = set_immediate_fan_speed(spd);
                        self.log_result(&format!("Manual Fan I2C ({spd}%)"), res);
                    }
                    aorus_tooltip(
                        apply_resp,
                        t.btn_apply_manual_fan,
                        t.btn_apply_manual_fan_tip,
                        Some("/dev/i2c-1 @ 0x1a (reg 0x80)"),
                    );

                    if self.telemetry.manual_fan_override.is_some() {
                        ui.add_space(6.0);
                        if aorus_full_cta_button(ui, t.btn_return_auto, uc().btn_cyan, uc().cyan)
                            .clicked()
                        {
                            self.telemetry.manual_fan_override = None;
                            let res = control_argononed_service("restart");
                            self.log_result("Restore Auto Fan Daemon", res);
                        }
                    }
                }
            }
        });
    }

    // =========================================================================
    // TAB 3: OVERCLOCKING (AORUS ENGINE — ALIGNED GRID + TOOLTIPS)
    // =========================================================================
    fn draw_overclocking_tab(&mut self, ui: &mut Ui) {
        let t = texts(self.lang);
        let lang = self.lang;

        // Grouped 1-Click Performance Profile Bar
        aorus_card(ui, t.card_oc_presets, uc().orange, |ui| {
            egui::Frame::new()
                .fill(uc().bg_subgroup)
                .stroke(Stroke::new(1.0_f32, uc().border_subtle))
                .corner_radius(CornerRadius::same(5))
                .inner_margin(egui::Margin::symmetric(8, 6))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal_wrapped(|ui| {
                        let presets = [
                            (OcPreset::EcoPowerSave, t.oc_preset_eco_tip),
                            (OcPreset::StockBoost, t.oc_preset_stock_tip),
                            (OcPreset::OcGaming, t.oc_preset_gaming_tip),
                            (OcPreset::OcExtreme, t.oc_preset_extreme_tip),
                            (OcPreset::Custom, t.oc_preset_custom_tip),
                        ];
                        for (p, tip) in presets {
                            let is_sel = self.selected_oc_preset == p;
                            let resp = segmented_pill(
                                ui,
                                &p.label(lang),
                                is_sel,
                                uc().btn_orange,
                                uc().orange,
                            );
                            if resp.clicked() {
                                self.selected_oc_preset = p;
                                self.boot_oc.apply_preset(p);
                                match p {
                                    OcPreset::EcoPowerSave => {
                                        self.runtime_governor = "powersave".to_string();
                                        self.runtime_max_mhz = crate::board::current()
                                            .oc
                                            .eco_mhz
                                            .min(self.telemetry.cpu_hw_max_mhz);
                                    }
                                    OcPreset::StockBoost => {
                                        self.runtime_governor = "ondemand".to_string();
                                        self.runtime_max_mhz = self.telemetry.cpu_hw_max_mhz;
                                    }
                                    OcPreset::OcGaming | OcPreset::OcExtreme => {
                                        self.runtime_governor = "performance".to_string();
                                        self.runtime_max_mhz = self.telemetry.cpu_hw_max_mhz;
                                    }
                                    OcPreset::Custom => {}
                                }
                            }
                            aorus_tooltip(
                                resp,
                                &p.label(lang),
                                tip,
                                Some("/boot/firmware/config.txt"),
                            );
                        }
                    });
                });
        });

        ui.add_space(12.0);

        responsive_two_columns(
            ui,
            self,
            |app, left_ui| {
                app.draw_runtime_cpu_card(left_ui);
            },
            |app, right_ui| {
                app.draw_firmware_oc_card(right_ui);
            },
        );
    }

    fn draw_runtime_cpu_card(&mut self, ui: &mut Ui) {
        let t = texts(self.lang);
        aorus_card(ui, t.card_runtime_cpu, uc().green, |ui| {
            adapt_slider_width(ui);
            ui.label(
                RichText::new(t.runtime_cpu_desc)
                    .size(12.5)
                    .color(uc().text_secondary),
            );
            ui.add_space(8.0);

            let hw_min = self.telemetry.cpu_hw_min_mhz.max(600);
            let hw_max = self.telemetry.cpu_hw_max_mhz.max(1800);

            egui::Frame::new()
                .fill(uc().bg_subgroup)
                .stroke(Stroke::new(1.0_f32, uc().border_subtle))
                .corner_radius(CornerRadius::same(5))
                .inner_margin(egui::Margin::same(10))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    egui::Grid::new("runtime_cpu_grid")
                        .num_columns(2)
                        .spacing([14.0, 10.0])
                        .show(ui, |ui| {
                            label_with_tip(
                                ui,
                                t.gov_label,
                                t.gov_tip,
                                Some("/sys/devices/system/cpu/cpu*/cpufreq/scaling_governor"),
                            );
                            egui::ComboBox::from_id_salt("gov_combo")
                                .width(175.0)
                                .selected_text(
                                    RichText::new(self.runtime_governor.to_uppercase())
                                        .strong()
                                        .color(uc().text_primary),
                                )
                                .show_ui(ui, |ui| {
                                    for g in &self.telemetry.available_governors {
                                        ui.selectable_value(
                                            &mut self.runtime_governor,
                                            g.clone(),
                                            g.to_uppercase(),
                                        );
                                    }
                                });
                            ui.end_row();

                            label_with_tip(
                                ui,
                                t.min_freq_label,
                                t.min_freq_tip,
                                Some("/sys/devices/system/cpu/cpu*/cpufreq/scaling_min_freq"),
                            );
                            ui.add(
                                egui::Slider::new(&mut self.runtime_min_mhz, hw_min..=hw_max)
                                    .step_by(100.0)
                                    .suffix(" MHz"),
                            );
                            ui.end_row();

                            label_with_tip(
                                ui,
                                t.max_freq_label,
                                t.max_freq_tip,
                                Some("/sys/devices/system/cpu/cpu*/cpufreq/scaling_max_freq"),
                            );
                            ui.add(
                                egui::Slider::new(&mut self.runtime_max_mhz, hw_min..=hw_max)
                                    .step_by(100.0)
                                    .suffix(" MHz"),
                            );
                            ui.end_row();
                        });

                    ui.add_space(6.0);
                    let boost_resp = ui.checkbox(&mut self.runtime_boost, t.boost_checkbox);
                    aorus_tooltip(
                        boost_resp,
                        t.boost_checkbox,
                        t.boost_tip,
                        Some("/sys/devices/system/cpu/cpufreq/boost"),
                    );
                });

            ui.add_space(10.0);
            let apply_resp =
                aorus_full_cta_button(ui, t.btn_apply_runtime_cpu, uc().btn_green, uc().green);
            if apply_resp.clicked() {
                let res = apply_runtime_cpufreq(
                    &self.runtime_governor,
                    self.runtime_min_mhz,
                    self.runtime_max_mhz,
                    self.runtime_boost,
                );
                self.log_result("Runtime CPUFreq", res);
                self.telemetry.refresh(&self.cpu_fan_curve);
            }
            aorus_tooltip(
                apply_resp,
                t.btn_apply_runtime_cpu,
                t.btn_apply_runtime_cpu_tip,
                Some("sysfs cpufreq"),
            );

            ui.separator();
            ui.label(
                RichText::new(format!(
                    "{}: {} MHz | Governor '{}' | {}-{} MHz",
                    t.runtime_status_prefix,
                    self.telemetry.cpu_cur_freq_mhz,
                    self.telemetry.cpu_governor,
                    self.telemetry.cpu_min_freq_mhz,
                    self.telemetry.cpu_max_freq_mhz
                ))
                .size(12.5)
                .strong()
                .color(uc().cyan),
            );
        });
    }

    fn draw_firmware_oc_card(&mut self, ui: &mut Ui) {
        let t = texts(self.lang);
        aorus_card(ui, t.card_firmware_oc, uc().orange, |ui| {
            adapt_slider_width(ui);
            ui.label(
                RichText::new(t.firmware_oc_desc)
                    .size(12.5)
                    .color(uc().text_secondary),
            );
            ui.add_space(8.0);

            egui::Frame::new()
                .fill(uc().bg_subgroup)
                .stroke(Stroke::new(1.0_f32, uc().border_subtle))
                .corner_radius(CornerRadius::same(5))
                .inner_margin(egui::Margin::same(10))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    let oc_prof = crate::board::current().oc;
                    egui::Grid::new("firmware_oc_grid")
                        .num_columns(2)
                        .spacing([14.0, 10.0])
                        .show(ui, |ui| {
                            label_with_tip(ui, t.arm_freq_label, t.arm_freq_tip, Some("arm_freq"));
                            if ui
                                .add(
                                    egui::Slider::new(
                                        &mut self.boot_oc.arm_freq_mhz,
                                        oc_prof.arm_range.0..=oc_prof.arm_range.1,
                                    )
                                    .step_by(25.0)
                                    .suffix(" MHz"),
                                )
                                .changed()
                            {
                                self.selected_oc_preset = OcPreset::Custom;
                            }
                            ui.end_row();

                            label_with_tip(ui, t.gpu_freq_label, t.gpu_freq_tip, Some("gpu_freq"));
                            if ui
                                .add(
                                    egui::Slider::new(
                                        &mut self.boot_oc.gpu_freq_mhz,
                                        oc_prof.gpu_range.0..=oc_prof.gpu_range.1,
                                    )
                                    .step_by(25.0)
                                    .suffix(" MHz"),
                                )
                                .changed()
                            {
                                self.selected_oc_preset = OcPreset::Custom;
                            }
                            ui.end_row();

                            if oc_prof.uses_ov_delta {
                                label_with_tip(
                                    ui,
                                    "over_voltage_delta",
                                    t.over_voltage_delta_tip,
                                    Some("over_voltage_delta (µV)"),
                                );
                                if ui
                                    .add(
                                        egui::Slider::new(
                                            &mut self.boot_oc.over_voltage_delta_uv,
                                            0..=150_000,
                                        )
                                        .step_by(5_000.0)
                                        .suffix(" µV"),
                                    )
                                    .changed()
                                {
                                    self.selected_oc_preset = OcPreset::Custom;
                                }
                                ui.end_row();
                            } else {
                                label_with_tip(
                                    ui,
                                    t.over_voltage_label,
                                    t.over_voltage_tip,
                                    Some("over_voltage"),
                                );
                                ui.horizontal(|ui| {
                                    if ui
                                        .add(
                                            egui::Slider::new(
                                                &mut self.boot_oc.over_voltage,
                                                -4..=8,
                                            )
                                            .suffix(" st"),
                                        )
                                        .changed()
                                    {
                                        self.selected_oc_preset = OcPreset::Custom;
                                    }
                                    ui.label(
                                        RichText::new(format!(
                                            "~{:.2}V",
                                            self.boot_oc.estimated_core_voltage()
                                        ))
                                        .size(12.0)
                                        .strong()
                                        .color(uc().amber),
                                    );
                                });
                                ui.end_row();
                            }

                            label_with_tip(
                                ui,
                                t.temp_limit_label,
                                t.temp_limit_tip,
                                Some("temp_limit"),
                            );
                            ui.add(
                                egui::Slider::new(&mut self.boot_oc.temp_limit_c, 70..=85)
                                    .suffix(" °C"),
                            );
                            ui.end_row();
                        });

                    ui.add_space(6.0);
                    let r_boost = ui.checkbox(&mut self.boot_oc.arm_boost, t.arm_boost_check);
                    aorus_tooltip(
                        r_boost,
                        t.arm_boost_check,
                        t.arm_boost_tip,
                        Some("arm_boost=1"),
                    );

                    let r_turbo = ui.checkbox(&mut self.boot_oc.force_turbo, t.force_turbo_check);
                    aorus_tooltip(
                        r_turbo,
                        t.force_turbo_check,
                        t.force_turbo_tip,
                        Some("force_turbo=1"),
                    );
                });

            if self.boot_oc.over_voltage > 6 || self.boot_oc.force_turbo {
                ui.add_space(4.0);
                ui.label(
                    RichText::new(t.oc_warning)
                        .size(12.0)
                        .strong()
                        .color(uc().red),
                );
            }

            ui.add_space(10.0);
            let save_resp =
                aorus_full_cta_button(ui, t.btn_save_firmware_oc, uc().btn_orange, uc().orange);
            if save_resp.clicked() {
                let res = save_boot_config(&self.boot_oc);
                self.log_result("Firmware OC (config.txt)", res);
            }
            aorus_tooltip(
                save_resp,
                t.btn_save_firmware_oc,
                t.btn_save_firmware_oc_tip,
                Some(get_boot_config_path()),
            );
        });
    }

    // =========================================================================
    // TAB 4: RECURSOS & HARDWARE (CATEGORIZED GROUPS + RICH TOOLTIPS)
    // =========================================================================
    fn draw_features_tab(&mut self, ui: &mut Ui) {
        responsive_two_columns(
            ui,
            self,
            |app, left_ui| {
                app.draw_hw_interfaces_card(left_ui);
            },
            |app, right_ui| {
                app.draw_argon_peripherals_card(right_ui);
            },
        );
    }

    fn draw_hw_interfaces_card(&mut self, ui: &mut Ui) {
        let t = texts(self.lang);
        aorus_card(ui, t.card_hw_interfaces, uc().orange, |ui| {
            ui.label(
                RichText::new(t.hw_interfaces_desc)
                    .size(12.5)
                    .color(uc().text_secondary),
            );
            ui.add_space(6.0);

            // Group 1: GPIO Buses
            section_group_header(ui, t.group_buses, uc().amber);
            feature_toggle_row(
                ui,
                t.feat_i2c_title,
                t.feat_i2c_desc,
                "dtparam=i2c_arm=on",
                &mut self.boot_oc.i2c_arm,
            );
            feature_toggle_row(
                ui,
                t.feat_spi_title,
                t.feat_spi_desc,
                "dtparam=spi=on",
                &mut self.boot_oc.spi,
            );
            feature_toggle_row(
                ui,
                t.feat_i2s_title,
                t.feat_i2s_desc,
                "dtparam=i2s=on",
                &mut self.boot_oc.i2s,
            );
            feature_toggle_row(
                ui,
                t.feat_uart_title,
                t.feat_uart_desc,
                "enable_uart=1",
                &mut self.boot_oc.enable_uart,
            );

            ui.add_space(4.0);

            // Group 2: Graphics, Audio & Camera/Display
            section_group_header(ui, t.group_multimedia, uc().cyan);
            feature_toggle_row(
                ui,
                t.feat_vc4_title,
                t.feat_vc4_desc,
                "dtoverlay=vc4-kms-v3d",
                &mut self.boot_oc.vc4_kms_v3d,
            );
            feature_toggle_row(
                ui,
                t.feat_audio_title,
                t.feat_audio_desc,
                "dtparam=audio=on",
                &mut self.boot_oc.onboard_audio,
            );
            feature_toggle_row(
                ui,
                t.feat_cam_title,
                t.feat_cam_desc,
                "camera_auto_detect=1",
                &mut self.boot_oc.camera_auto_detect,
            );
            feature_toggle_row(
                ui,
                t.feat_disp_title,
                t.feat_disp_desc,
                "display_auto_detect=1",
                &mut self.boot_oc.display_auto_detect,
            );

            ui.add_space(4.0);

            // Group 3: Argon40 Expansion Modules
            section_group_header(ui, t.group_argon_addons, uc().green);
            feature_toggle_row(
                ui,
                t.feat_dac_title,
                t.feat_dac_desc,
                "dtoverlay=hifiberry-dacplus,slave",
                &mut self.boot_oc.argon_blstr_dac,
            );
            feature_toggle_row(
                ui,
                t.feat_ir_title,
                t.feat_ir_desc,
                "dtoverlay=gpio-ir,gpio_pin=23",
                &mut self.boot_oc.argon_ir_receiver,
            );

            ui.add_space(10.0);
            let save_resp =
                aorus_full_cta_button(ui, t.btn_apply_hw_features, uc().btn_orange, uc().orange);
            if save_resp.clicked() {
                let res = save_boot_config(&self.boot_oc);
                self.log_result("Hardware Features (config.txt)", res);
            }
            aorus_tooltip(
                save_resp,
                t.btn_apply_hw_features,
                t.btn_apply_hw_features_tip,
                Some(get_boot_config_path()),
            );
        });
    }

    fn draw_argon_peripherals_card(&mut self, ui: &mut Ui) {
        let t = texts(self.lang);
        aorus_card(ui, t.card_argon_peripherals, uc().cyan, |ui| {
            adapt_slider_width(ui);

            // Group 1: OLED Configuration
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(t.oled_section).strong().color(uc().amber));
                let badge = if self.oled_hw_present {
                    (t.oled_installed_badge, uc().green)
                } else {
                    (t.oled_preview_badge, uc().text_secondary)
                };
                ui.label(
                    RichText::new(format!("({})", badge.0))
                        .size(11.5)
                        .color(badge.1),
                );
            });
            ui.add_space(4.0);

            egui::Frame::new()
                .fill(uc().bg_subgroup)
                .stroke(Stroke::new(1.0_f32, uc().border_subtle))
                .corner_radius(CornerRadius::same(5))
                .inner_margin(egui::Margin::same(10))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    let r_en = ui.checkbox(&mut self.oled_cfg.enabled, t.oled_enable_check);
                    aorus_tooltip(
                        r_en,
                        t.oled_enable_check,
                        t.oled_enable_tip,
                        Some("/etc/argoneonoled.conf"),
                    );

                    ui.add_space(4.0);
                    egui::Grid::new("oled_sliders_grid")
                        .num_columns(2)
                        .spacing([12.0, 8.0])
                        .show(ui, |ui| {
                            label_with_tip(
                                ui,
                                t.oled_switch_duration,
                                t.oled_switch_tip,
                                Some("switchduration"),
                            );
                            ui.add(
                                egui::Slider::new(&mut self.oled_cfg.switch_duration_sec, 5..=120)
                                    .suffix(" s"),
                            );
                            ui.end_row();

                            label_with_tip(
                                ui,
                                t.oled_screensaver,
                                t.oled_screensaver_tip,
                                Some("screensaver"),
                            );
                            ui.add(
                                egui::Slider::new(&mut self.oled_cfg.screensaver_sec, 30..=600)
                                    .suffix(" s"),
                            );
                            ui.end_row();
                        });

                    ui.add_space(6.0);
                    ui.label(
                        RichText::new(t.oled_pages_label)
                            .size(12.0)
                            .color(uc().text_secondary),
                    );
                    ui.horizontal_wrapped(|ui| {
                        for page in [
                            "clock", "cpu", "storage", "raid", "ram", "temp", "ip", "logo1v5",
                        ] {
                            let mut has = self.oled_cfg.screens.iter().any(|s| s == page);
                            if segmented_pill(
                                ui,
                                &page.to_uppercase(),
                                has,
                                uc().btn_cyan,
                                uc().cyan,
                            )
                            .clicked()
                            {
                                has = !has;
                                if has {
                                    self.oled_cfg.screens.push(page.to_string());
                                } else {
                                    self.oled_cfg.screens.retain(|s| s != page);
                                }
                            }
                        }
                    });

                    ui.add_space(8.0);
                    let r_oled = aorus_action_button(ui, t.btn_save_oled, uc().btn_cyan, uc().cyan);
                    if r_oled.clicked() {
                        let res = save_oled_config(&self.oled_cfg);
                        self.log_result("Save OLED Config", res);
                    }
                    aorus_tooltip(
                        r_oled,
                        t.btn_save_oled,
                        t.btn_save_oled_tip,
                        Some("/etc/argoneonoled.conf"),
                    );
                });

            ui.add_space(12.0);

            // Group 2: Argon Firmware & IR Utilities (Structured Action Rows)
            section_group_header(ui, t.ir_eeprom_section, uc().green);

            utility_action_row(
                ui,
                t.ir_row_label,
                t.ir_row_desc,
                t.btn_prog_ir,
                t.btn_prog_ir_tip,
                "/etc/argon/argonone-ir",
                || {
                    let res = reset_argon_ir_power_button();
                    self.log_result("Argon IR Power Button", res);
                },
            );

            utility_action_row(
                ui,
                t.eeprom_row_label,
                t.eeprom_row_desc,
                t.btn_opt_eeprom,
                t.btn_opt_eeprom_tip,
                "/etc/argon/argonone-eepromconfig.py",
                || {
                    let res = apply_argon_eeprom_psu_current();
                    self.log_result("EEPROM PSU_MAX_CURRENT=5000", res);
                },
            );
        });
    }

    // =========================================================================
    // TAB 5: SERVIÇOS & LOGS (SEGMENTED TOOLBAR + TABBED FILE INSPECTOR)
    // =========================================================================
    fn draw_services_tab(&mut self, ui: &mut Ui) {
        let t = texts(self.lang);
        responsive_two_columns(
            ui,
            self,
            |app, left_ui| {
                aorus_card(left_ui, t.card_systemd_service, uc().green, |ui| {
                    // 1. Visual Explainer Box: What does argononed do?
                    egui::Frame::new()
                        .fill(tint(uc().cyan))
                        .stroke(Stroke::new(1.0_f32, uc().cyan.gamma_multiply(0.55)))
                        .corner_radius(CornerRadius::same(5))
                        .inner_margin(egui::Margin::symmetric(10, 8))
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.label(
                                RichText::new(t.argononed_explainer_title)
                                    .size(12.5)
                                    .strong()
                                    .color(uc().cyan),
                            );
                            ui.add_space(4.0);
                            for role_txt in [
                                t.argononed_role_fan,
                                t.argononed_role_power,
                                t.argononed_role_oled,
                            ] {
                                ui.label(
                                    RichText::new(role_txt).size(12.0).color(uc().text_primary),
                                );
                                ui.add_space(2.0);
                            }
                        });

                    ui.add_space(8.0);

                    // 2. Impact Warning Before Stopping/Disabling
                    egui::Frame::new()
                        .fill(tint(uc().amber))
                        .stroke(Stroke::new(1.0_f32, uc().amber.gamma_multiply(0.65)))
                        .corner_radius(CornerRadius::same(5))
                        .inner_margin(egui::Margin::symmetric(10, 7))
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.label(
                                RichText::new(t.argononed_stop_warning)
                                    .size(11.8)
                                    .color(uc().amber),
                            );
                        });

                    ui.add_space(8.0);

                    // 3. Unified Service Control Strip (Runtime + Boot Startup)
                    egui::Frame::new()
                        .fill(uc().bg_subgroup)
                        .stroke(Stroke::new(1.0_f32, uc().border_subtle))
                        .corner_radius(CornerRadius::same(5))
                        .inner_margin(egui::Margin::symmetric(10, 8))
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.horizontal_wrapped(|ui| {
                                ui.label(RichText::new(t.daemon_state_label).strong());
                                let col = if app.telemetry.argon_service_active {
                                    uc().green
                                } else {
                                    uc().red
                                };
                                ui.label(
                                    RichText::new(
                                        app.telemetry.argon_service_status.to_uppercase(),
                                    )
                                    .strong()
                                    .color(col),
                                );

                                ui.separator();

                                ui.label(RichText::new(t.daemon_boot_label).strong());
                                let (boot_col, boot_txt) =
                                    if app.telemetry.argon_service_enabled_boot {
                                        (uc().green, t.daemon_boot_enabled)
                                    } else {
                                        (uc().amber, t.daemon_boot_disabled)
                                    };
                                ui.label(
                                    RichText::new(boot_txt).size(12.0).strong().color(boot_col),
                                );
                            });

                            ui.add_space(6.0);
                            ui.horizontal_wrapped(|ui| {
                                let r_start = aorus_action_button(
                                    ui,
                                    t.btn_start,
                                    uc().btn_green,
                                    uc().green,
                                );
                                if r_start.clicked() {
                                    let res = control_argononed_service("start");
                                    app.log_result("Start argononed.service", res);
                                    app.telemetry.last_slow_refresh = None;
                                }
                                aorus_tooltip(
                                    r_start,
                                    t.btn_start,
                                    t.btn_start_tip,
                                    Some("systemctl start argononed.service"),
                                );

                                let r_rest = aorus_action_button(
                                    ui,
                                    t.btn_restart,
                                    uc().btn_cyan,
                                    uc().cyan,
                                );
                                if r_rest.clicked() {
                                    let res = control_argononed_service("restart");
                                    app.log_result("Restart argononed.service", res);
                                    app.telemetry.last_slow_refresh = None;
                                }
                                aorus_tooltip(
                                    r_rest,
                                    t.btn_restart,
                                    t.btn_restart_tip,
                                    Some("systemctl restart argononed.service"),
                                );

                                let r_stop = ui.button(t.btn_stop);
                                if r_stop.clicked() {
                                    let res = control_argononed_service("stop");
                                    app.log_result("Stop argononed.service", res);
                                    app.telemetry.last_slow_refresh = None;
                                }
                                aorus_tooltip(
                                    r_stop,
                                    t.btn_stop,
                                    t.btn_stop_tip,
                                    Some("systemctl stop argononed.service"),
                                );

                                ui.separator();

                                let r_en = ui.button(t.btn_enable_boot);
                                if r_en.clicked() {
                                    let res = control_argononed_service("enable");
                                    app.log_result("Enable argononed.service", res);
                                    app.telemetry.last_slow_refresh = None;
                                }
                                aorus_tooltip(
                                    r_en,
                                    t.btn_enable_boot,
                                    t.btn_enable_boot_tip,
                                    Some("systemctl enable argononed.service"),
                                );

                                let r_dis = ui.button(t.btn_disable_boot);
                                if r_dis.clicked() {
                                    let res = control_argononed_service("disable");
                                    app.log_result("Disable argononed.service", res);
                                    app.telemetry.last_slow_refresh = None;
                                }
                                aorus_tooltip(
                                    r_dis,
                                    t.btn_disable_boot,
                                    t.btn_disable_boot_tip,
                                    Some("systemctl disable argononed.service"),
                                );
                            });
                        });

                    ui.add_space(12.0);

                    // Tabbed Configuration File Inspector (Replaces 2 cramped boxes)
                    ui.horizontal_wrapped(|ui| {
                        ui.label(
                            RichText::new(t.config_inspector_label)
                                .size(12.5)
                                .strong()
                                .color(uc().text_secondary),
                        );
                        if segmented_pill(
                            ui,
                            "📄 /etc/argononed.conf",
                            app.config_preview_tab == ConfigPreviewTab::ArgonFanConf,
                            uc().btn_orange,
                            uc().orange,
                        )
                        .clicked()
                        {
                            app.config_preview_tab = ConfigPreviewTab::ArgonFanConf;
                        }
                        if segmented_pill(
                            ui,
                            &format!("⚙ {}", get_boot_config_path()),
                            app.config_preview_tab == ConfigPreviewTab::BootConfigTxt,
                            uc().btn_orange,
                            uc().orange,
                        )
                        .clicked()
                        {
                            app.config_preview_tab = ConfigPreviewTab::BootConfigTxt;
                        }
                    });

                    ui.add_space(6.0);
                    match app.config_preview_tab {
                        ConfigPreviewTab::ArgonFanConf => {
                            let fan_preview = serialize_fan_curve(&app.cpu_fan_curve, "CPU");
                            high_contrast_code_box(ui, "fan_conf_preview", &fan_preview, 280.0);
                        }
                        ConfigPreviewTab::BootConfigTxt => {
                            let orig =
                                fs::read_to_string(get_boot_config_path()).unwrap_or_default();
                            let boot_preview = update_boot_config_str(&orig, &app.boot_oc);
                            high_contrast_code_box(ui, "boot_conf_preview", &boot_preview, 280.0);
                        }
                    }
                });
            },
            |app, right_ui| {
                aorus_card(right_ui, t.card_audit_logs, uc().cyan, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(
                            RichText::new(t.audit_logs_desc)
                                .size(12.5)
                                .color(uc().text_secondary),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button(t.btn_clear_logs).clicked() {
                                app.action_logs.clear();
                            }
                        });
                    });
                    ui.add_space(6.0);

                    egui::Frame::new()
                        .fill(uc().bg_subgroup)
                        .stroke(Stroke::new(1.0_f32, uc().border_subtle))
                        .corner_radius(CornerRadius::same(4))
                        .inner_margin(egui::Margin::same(10))
                        .show(ui, |ui| {
                            egui::ScrollArea::vertical()
                                .id_salt("audit_logs_scroll")
                                .max_height(355.0)
                                .stick_to_bottom(true)
                                .show(ui, |ui| {
                                    ui.set_width(ui.available_width());
                                    for entry in &app.action_logs {
                                        let color = if entry.starts_with("[ERR") {
                                            uc().red
                                        } else if entry.starts_with("[OK]") {
                                            uc().green
                                        } else {
                                            uc().text_primary
                                        };
                                        ui.label(
                                            RichText::new(entry)
                                                .monospace()
                                                .size(12.5)
                                                .color(color),
                                        );
                                    }
                                });
                        });
                });
            },
        );
    }

    // =========================================================================
    // TAB 6: GPIO LOGIC ANALYZER & ARDUINO-STYLE SERIAL/I2C DEV CONSOLE
    // =========================================================================
    fn draw_gpio_lab_tab(&mut self, ui: &mut Ui) {
        responsive_two_columns(
            ui,
            self,
            |app, left_ui| {
                app.draw_gpio_header_card(left_ui);
            },
            |app, right_ui| {
                app.draw_logic_analyzer_and_console(right_ui);
            },
        );
    }

    fn draw_gpio_header_card(&mut self, ui: &mut Ui) {
        let t = texts(self.lang);
        aorus_card_with_tip(
            ui,
            t.card_gpio_header,
            t.card_gpio_header_tip,
            uc().orange,
            |ui| {
                // Top Control Strip: Hardware Unlock Badge/Button + Pulse Generator Toggle
                egui::Frame::new()
                    .fill(uc().bg_subgroup)
                    .stroke(Stroke::new(1.0_f32, uc().border_subtle))
                    .corner_radius(CornerRadius::same(5))
                    .inner_margin(egui::Margin::symmetric(8, 5))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal_wrapped(|ui| {
                            if self.gpio_lab.hw_access_unlocked {
                                ui.label(
                                    RichText::new(t.gpio_unlocked_badge)
                                        .size(11.5)
                                        .strong()
                                        .color(uc().green),
                                );
                            } else {
                                let r_un = aorus_action_button(
                                    ui,
                                    t.btn_unlock_gpio,
                                    uc().btn_orange,
                                    uc().orange,
                                );
                                if r_un.clicked() {
                                    let res = self.gpio_lab.unlock_hardware_access();
                                    self.log_result("Unlock GPIO/I2C Hardware", res);
                                }
                                aorus_tooltip(
                                    r_un,
                                    t.btn_unlock_gpio,
                                    t.btn_unlock_gpio_tip,
                                    Some("/usr/bin/pinctrl & /dev/gpiochip0"),
                                );
                            }

                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    let r_sim = ui.checkbox(
                                        &mut self.gpio_lab.simulate_pulses,
                                        t.chk_sim_pulses,
                                    );
                                    aorus_tooltip(
                                        r_sim,
                                        t.chk_sim_pulses,
                                        t.chk_sim_pulses_tip,
                                        None,
                                    );
                                },
                            );
                        });
                    });

                ui.add_space(6.0);

                // Interactive 40-Pin Raspberry Pi Physical Header (20 rows x 2 columns)
                egui::Frame::new()
                    .fill(uc().bg_subgroup)
                    .stroke(Stroke::new(1.0_f32, uc().border_subtle))
                    .corner_radius(CornerRadius::same(6))
                    .inner_margin(egui::Margin::symmetric(8, 6))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        egui::ScrollArea::vertical()
                            .id_salt("rpi_40pin_scroll")
                            .max_height(315.0)
                            .show(ui, |ui| {
                                egui::Grid::new("rpi_40pin_grid")
                                    .num_columns(2)
                                    .spacing([10.0, 4.0])
                                    .striped(true)
                                    .show(ui, |ui| {
                                        for row in 0..20 {
                                            let odd_pin = &RPI_40PIN_HEADER[row * 2];
                                            let even_pin = &RPI_40PIN_HEADER[row * 2 + 1];

                                            self.draw_single_header_pin(ui, odd_pin, false);
                                            self.draw_single_header_pin(ui, even_pin, true);
                                            ui.end_row();
                                        }
                                    });
                            });
                    });

                ui.add_space(8.0);

                // Selected GPIO Pin Inspector & Signal Generator Strip
                let sel_bcm = self.gpio_lab.selected_bcm;
                if let Some(pin) = self.gpio_lab.pins.get(sel_bcm as usize).cloned() {
                    egui::Frame::new()
                        .fill(uc().bg_subgroup)
                        .stroke(Stroke::new(1.2_f32, uc().orange))
                        .corner_radius(CornerRadius::same(5))
                        .inner_margin(egui::Margin::symmetric(10, 8))
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.horizontal_wrapped(|ui| {
                                ui.label(
                                    RichText::new(format!(
                                        "{} GPIO{} ({})",
                                        t.pin_inspector_title, pin.bcm, pin.role_note
                                    ))
                                    .size(12.5)
                                    .strong()
                                    .color(uc().amber),
                                );
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        let lvl_col = if pin.level_high {
                                            uc().green
                                        } else {
                                            uc().cyan
                                        };
                                        let lvl_str = if pin.level_high {
                                            "HIGH (1 / 3.3V)"
                                        } else {
                                            "LOW (0 / 0.0V)"
                                        };
                                        ui.label(
                                            RichText::new(format!(
                                                "[{} | {}] {}  (↑{} ↓{})",
                                                pin.direction.short_label(),
                                                pin.pull.short_label(),
                                                lvl_str,
                                                pin.rising_edges,
                                                pin.falling_edges
                                            ))
                                            .monospace()
                                            .size(11.5)
                                            .strong()
                                            .color(lvl_col),
                                        );
                                    },
                                );
                            });

                            ui.add_space(6.0);
                            ui.horizontal_wrapped(|ui| {
                                if aorus_action_button(
                                    ui,
                                    t.btn_set_high,
                                    uc().btn_green,
                                    uc().green,
                                )
                                .clicked()
                                {
                                    let res = self.gpio_lab.set_gpio_output(sel_bcm, true);
                                    self.log_result(&format!("GPIO{sel_bcm} -> HIGH"), res);
                                }
                                if aorus_action_button(ui, t.btn_set_low, uc().btn_cyan, uc().cyan)
                                    .clicked()
                                {
                                    let res = self.gpio_lab.set_gpio_output(sel_bcm, false);
                                    self.log_result(&format!("GPIO{sel_bcm} -> LOW"), res);
                                }
                                if ui.button(t.btn_set_in_pu).clicked() {
                                    let res = self.gpio_lab.set_gpio_input(sel_bcm, PinPull::Up);
                                    self.log_result(&format!("GPIO{sel_bcm} -> IN (PU)"), res);
                                }
                                if ui.button(t.btn_set_in_pd).clicked() {
                                    let res = self.gpio_lab.set_gpio_input(sel_bcm, PinPull::Down);
                                    self.log_result(&format!("GPIO{sel_bcm} -> IN (PD)"), res);
                                }

                                let mut is_watched = self.gpio_lab.watched_bcm.contains(&sel_bcm);
                                if ui.checkbox(&mut is_watched, t.chk_watch_waveform).changed() {
                                    if is_watched {
                                        if self.gpio_lab.watched_bcm.len() >= 4 {
                                            self.gpio_lab.watched_bcm.remove(0);
                                        }
                                        self.gpio_lab.watched_bcm.push(sel_bcm);
                                    } else if self.gpio_lab.watched_bcm.len() > 1 {
                                        self.gpio_lab.watched_bcm.retain(|&b| b != sel_bcm);
                                    }
                                }
                            });
                        });
                }
            },
        );
    }

    fn draw_single_header_pin(
        &mut self,
        ui: &mut Ui,
        def: &crate::gpio_lab::PhysicalPinDef,
        right_side: bool,
    ) {
        match def.kind {
            HeaderPinKind::Power3v3 => {
                let txt = format!("#{:02}  ⚡ {}", def.phys_num, def.label);
                ui.label(
                    RichText::new(txt)
                        .monospace()
                        .size(11.5)
                        .strong()
                        .color(uc().amber),
                );
            }
            HeaderPinKind::Power5v => {
                let txt = format!("#{:02}  ⚡ {}", def.phys_num, def.label);
                ui.label(
                    RichText::new(txt)
                        .monospace()
                        .size(11.5)
                        .strong()
                        .color(uc().red),
                );
            }
            HeaderPinKind::Ground => {
                let txt = format!("#{:02}  ⏚ {}", def.phys_num, def.label);
                ui.label(
                    RichText::new(txt)
                        .monospace()
                        .size(11.5)
                        .color(uc().text_secondary),
                );
            }
            HeaderPinKind::IdEeprom => {
                let txt = format!("#{:02}  🏷 {}", def.phys_num, def.label);
                ui.label(RichText::new(txt).monospace().size(11.5).color(uc().cyan));
            }
            HeaderPinKind::Gpio(bcm) => {
                let is_sel = self.gpio_lab.selected_bcm == bcm;
                let (dir_str, pull_str, high, role, rising, falling) = self
                    .gpio_lab
                    .pins
                    .get(bcm as usize)
                    .map(|p| {
                        (
                            p.direction.short_label(),
                            p.pull.short_label(),
                            p.level_high,
                            p.role_note,
                            p.rising_edges,
                            p.falling_edges,
                        )
                    })
                    .unwrap_or(("IN", "NP", false, "", 0, 0));

                let led = if high { "🟢 1" } else { "⚫ 0" };
                let btn_txt = if right_side {
                    // Mirrored so the physical pin number sits next to the header, like the board.
                    format!(
                        "({}) {} [{}] {} | # {:02}",
                        pull_str, def.label, dir_str, led, def.phys_num
                    )
                } else {
                    format!(
                        "# {:02} | {} [{}] {} ({})",
                        def.phys_num, led, dir_str, def.label, pull_str
                    )
                };

                let fill = if is_sel {
                    uc().btn_orange
                } else if high {
                    Color32::from_rgb(14, 52, 38)
                } else {
                    Color32::from_rgb(20, 28, 44)
                };
                let border = if is_sel {
                    uc().orange
                } else if high {
                    uc().green
                } else {
                    uc().border_subtle
                };

                let btn = egui::Button::new(
                    RichText::new(btn_txt)
                        .monospace()
                        .size(11.0)
                        .strong()
                        .color(Color32::WHITE),
                )
                .fill(fill)
                .stroke(Stroke::new(1.0_f32, border))
                .min_size(Vec2::new(210.0, 22.0));

                let resp = ui.add(btn);
                if resp.clicked() {
                    self.gpio_lab.selected_bcm = bcm;
                }
                let tip_desc = format!(
                    "Função: {}\nDireção: {} | Pull: {}\nNível Lógico: {}\nBordas Capturadas: ↑ {} Subidas | ↓ {} Descidas",
                    role,
                    dir_str,
                    pull_str,
                    if high { "HIGH (3.3V)" } else { "LOW (0.0V)" },
                    rising,
                    falling
                );
                aorus_tooltip(
                    resp,
                    &format!("Pino Físico #{} — GPIO {}", def.phys_num, bcm),
                    &tip_desc,
                    Some(&format!("pinctrl get {bcm}")),
                );
            }
        }
    }

    fn draw_logic_analyzer_and_console(&mut self, ui: &mut Ui) {
        let t = texts(self.lang);

        // Top Card: Multi-Channel Digital Logic Analyzer (Waveform Plotter)
        aorus_card_with_tip(
            ui,
            t.card_logic_analyzer,
            t.card_logic_analyzer_tip,
            uc().green,
            |ui| {
                draw_logic_analyzer_waveforms(ui, &self.gpio_lab);
            },
        );

        ui.add_space(10.0);

        // Bottom Card: Professional Arduino-Style Serial / I2C / GPIO Developer Console
        aorus_card_with_tip(
            ui,
            t.card_arduino_console,
            t.card_arduino_console_tip,
            uc().cyan,
            |ui| {
                // Toolbar Row 1: Mode Switch + Connection / Bus Controls
                egui::Frame::new()
                    .fill(uc().bg_subgroup)
                    .stroke(Stroke::new(1.0_f32, uc().border_subtle))
                    .corner_radius(CornerRadius::same(5))
                    .inner_margin(egui::Margin::symmetric(8, 5))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal_wrapped(|ui| {
                            let r_uart = segmented_pill(
                                ui,
                                t.mode_uart,
                                self.gpio_lab.console_mode == ConsoleMode::SerialUart,
                                uc().btn_orange,
                                uc().orange,
                            );
                            if r_uart.clicked() {
                                self.gpio_lab.console_mode = ConsoleMode::SerialUart;
                            }
                            aorus_tooltip(
                                r_uart,
                                t.mode_uart,
                                t.mode_uart_tip,
                                Some("/dev/ttyS0 (GPIO 14/15)"),
                            );

                            let r_gmon = segmented_pill(
                                ui,
                                t.mode_gpio_mon,
                                self.gpio_lab.console_mode == ConsoleMode::GpioEdgeMonitor,
                                uc().btn_green,
                                uc().green,
                            );
                            if r_gmon.clicked() {
                                self.gpio_lab.console_mode = ConsoleMode::GpioEdgeMonitor;
                            }
                            aorus_tooltip(
                                r_gmon,
                                t.mode_gpio_mon,
                                t.mode_gpio_mon_tip,
                                Some("/dev/gpiochip0"),
                            );

                            let r_i2c = segmented_pill(
                                ui,
                                t.mode_i2c_insp,
                                self.gpio_lab.console_mode == ConsoleMode::I2cArgonInspector,
                                uc().btn_cyan,
                                uc().cyan,
                            );
                            if r_i2c.clicked() {
                                self.gpio_lab.console_mode = ConsoleMode::I2cArgonInspector;
                            }
                            aorus_tooltip(
                                r_i2c,
                                t.mode_i2c_insp,
                                t.mode_i2c_insp_tip,
                                Some("/dev/i2c-1 @ 0x1a"),
                            );
                        });

                        ui.add_space(5.0);
                        ui.horizontal_wrapped(|ui| {
                            match self.gpio_lab.console_mode {
                                ConsoleMode::SerialUart => {
                                    let ports = self.gpio_lab.available_ports.clone();
                                    egui::ComboBox::from_id_salt("serial_port_combo")
                                        .width(120.0)
                                        .selected_text(
                                            RichText::new(&self.gpio_lab.selected_port)
                                                .monospace()
                                                .color(uc().text_primary),
                                        )
                                        .show_ui(ui, |ui| {
                                            for p in ports {
                                                ui.selectable_value(
                                                    &mut self.gpio_lab.selected_port,
                                                    p.clone(),
                                                    p,
                                                );
                                            }
                                        });

                                    egui::ComboBox::from_id_salt("serial_baud_combo")
                                        .width(105.0)
                                        .selected_text(
                                            RichText::new(format!(
                                                "{} baud",
                                                self.gpio_lab.baud_rate
                                            ))
                                            .monospace()
                                            .color(uc().text_primary),
                                        )
                                        .show_ui(ui, |ui| {
                                            for b in [
                                                9600, 19200, 38400, 57600, 115200, 230400, 460800,
                                                921600,
                                            ] {
                                                ui.selectable_value(
                                                    &mut self.gpio_lab.baud_rate,
                                                    b,
                                                    format!("{b} baud"),
                                                );
                                            }
                                        });

                                    let (conn_lbl, conn_fill, conn_brd) =
                                        if self.gpio_lab.serial_connected {
                                            (t.btn_disconnect_serial, uc().red, Color32::WHITE)
                                        } else {
                                            (t.btn_connect_serial, uc().btn_green, uc().green)
                                        };
                                    if aorus_action_button(ui, conn_lbl, conn_fill, conn_brd)
                                        .clicked()
                                    {
                                        self.gpio_lab.toggle_serial_connection();
                                    }
                                }
                                ConsoleMode::GpioEdgeMonitor | ConsoleMode::I2cArgonInspector => {
                                    if aorus_action_button(
                                        ui,
                                        t.btn_i2c_scan,
                                        uc().btn_cyan,
                                        uc().cyan,
                                    )
                                    .clicked()
                                    {
                                        self.gpio_lab.probe_i2c_bus();
                                    }
                                    if aorus_action_button(
                                        ui,
                                        t.btn_mcu_dump,
                                        uc().btn_orange,
                                        uc().orange,
                                    )
                                    .clicked()
                                    {
                                        self.gpio_lab.probe_argon_mcu_registers();
                                    }
                                }
                            }

                            ui.separator();

                            // Format Switch: ASCII | HEX | BIN
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

                            ui.checkbox(&mut self.gpio_lab.show_timestamps, "🕒 Time");
                            ui.checkbox(&mut self.gpio_lab.auto_scroll, "⬇ Scroll");
                            ui.checkbox(&mut self.gpio_lab.paused, "⏸ Pause");
                            if ui
                                .small_button("🗑")
                                .on_hover_text(t.btn_clear_logs)
                                .clicked()
                            {
                                self.gpio_lab.entries.clear();
                            }
                        });
                    });

                ui.add_space(6.0);

                // Monospace Serial / Hardware Terminal Output Box
                egui::Frame::new()
                    .fill(uc().bg_subgroup)
                    .stroke(Stroke::new(1.0_f32, uc().border_subtle))
                    .corner_radius(CornerRadius::same(4))
                    .inner_margin(egui::Margin::same(8))
                    .show(ui, |ui| {
                        let fmt = self.gpio_lab.data_format;
                        let show_ts = self.gpio_lab.show_timestamps;
                        egui::ScrollArea::vertical()
                            .id_salt("arduino_serial_monitor_scroll")
                            .max_height(175.0)
                            .stick_to_bottom(self.gpio_lab.auto_scroll)
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                for entry in &self.gpio_lab.entries {
                                    let (prefix_col, dir_badge) = match entry.direction {
                                        "TX" => (uc().amber, "TX >>"),
                                        "RX" => (uc().green, "RX <<"),
                                        "EV" => (uc().cyan, "EV ⚡"),
                                        "ERR" => (uc().red, "ERR ✖"),
                                        _ => (uc().text_secondary, "SYS ℹ"),
                                    };
                                    let payload = format_console_entry_payload(entry, fmt);
                                    let line = if show_ts {
                                        format!("[{}] {} {}", entry.timestamp, dir_badge, payload)
                                    } else {
                                        format!("{} {}", dir_badge, payload)
                                    };
                                    ui.label(
                                        RichText::new(line)
                                            .monospace()
                                            .size(12.0)
                                            .color(prefix_col),
                                    );
                                }
                            });
                    });

                ui.add_space(6.0);

                // Bottom TX Command Input Bar (Arduino Serial Monitor Style)
                ui.horizontal_wrapped(|ui| {
                    let text_edit = egui::TextEdit::singleline(&mut self.gpio_lab.tx_input)
                        .hint_text(t.tx_placeholder)
                        .font(TextStyle::Monospace)
                        .desired_width((ui.available_width() - 235.0).max(140.0));

                    let edit_resp = ui.add(text_edit);
                    let enter_pressed =
                        edit_resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));

                    egui::ComboBox::from_id_salt("line_ending_combo")
                        .width(115.0)
                        .selected_text(
                            RichText::new(match self.gpio_lab.line_ending {
                                LineEnding::None => "No EOL",
                                LineEnding::Newline => "LF (\\n)",
                                LineEnding::CarriageReturn => "CR (\\r)",
                                LineEnding::BothNlCr => "CRLF (\\r\\n)",
                            })
                            .size(11.5)
                            .color(uc().text_primary),
                        )
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

                    if aorus_action_button(ui, t.btn_send_tx, uc().btn_orange, uc().orange)
                        .clicked()
                        || enter_pressed
                    {
                        self.gpio_lab.send_console_command();
                    }
                });
            },
        );
    }
}

/// Renders a 4-Channel Digital Logic Analyzer / Waveform Plotter for the watched GPIO pins.
fn draw_logic_analyzer_waveforms(ui: &mut Ui, lab: &GpioSerialLabState) {
    let channels = &lab.watched_bcm;
    let row_h = 38.0;
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

    let label_col_w = 135.0;
    let colors = [uc().green, uc().cyan, uc().orange, uc().amber];

    for (idx, &bcm) in channels.iter().enumerate() {
        let Some(pin) = lab.pins.get(bcm as usize) else {
            continue;
        };
        let y_top = rect.top() + 6.0 + (idx as f32) * row_h;
        let y_high = y_top + 7.0;
        let y_low = y_top + row_h - 9.0;
        let trace_color = colors[idx % colors.len()];

        // Channel separator line
        if idx > 0 {
            painter.line_segment(
                [
                    Pos2::new(rect.left() + 4.0, y_top - 2.0),
                    Pos2::new(rect.right() - 4.0, y_top - 2.0),
                ],
                Stroke::new(1.0_f32, chart_grid()),
            );
        }

        // Left channel badge: GPIO name + Direction + HIGH/LOW + Edge count
        let state_txt = if pin.level_high { "HIGH 1" } else { "LOW  0" };
        painter.text(
            Pos2::new(rect.left() + 8.0, y_top + 10.0),
            Align2::LEFT_CENTER,
            format!("GPIO{:02} [{}]", pin.bcm, pin.direction.short_label()),
            FontId::new(11.5, FontFamily::Monospace),
            uc().text_primary,
        );
        painter.text(
            Pos2::new(rect.left() + 8.0, y_top + 24.0),
            Align2::LEFT_CENTER,
            format!("{} ↑{} ↓{}", state_txt, pin.rising_edges, pin.falling_edges),
            FontId::new(10.5, FontFamily::Monospace),
            trace_color,
        );

        // Digital Square-Wave Trace
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
                        // Vertical edge step for crisp digital logic analyzer look
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

// =============================================================================
// RESPONSIVE LAYOUT, RICH TOOLTIPS & SEGMENTED WIDGET HELPERS
// =============================================================================

/// Attaches a styled Gigabyte AORUS rich tooltip card with title, explanation, and optional Linux/hardware path.
fn aorus_tooltip(
    resp: Response,
    title: &str,
    description: &str,
    tech_path: Option<&str>,
) -> Response {
    resp.on_hover_ui(|ui| {
        ui.set_max_width(340.0);
        ui.horizontal(|ui| {
            let (bar, _) = ui.allocate_exact_size(Vec2::new(3.0, 14.0), Sense::hover());
            ui.painter()
                .rect_filled(bar, CornerRadius::same(1), uc().orange);
            ui.label(RichText::new(title).size(13.0).strong().color(uc().amber));
        });
        ui.add_space(3.0);
        ui.label(
            RichText::new(description)
                .size(12.5)
                .color(uc().text_primary),
        );
        if let Some(path) = tech_path {
            ui.add_space(4.0);
            egui::Frame::new()
                .fill(uc().bg_subgroup)
                .stroke(Stroke::new(1.0_f32, uc().border_subtle))
                .corner_radius(CornerRadius::same(3))
                .inner_margin(egui::Margin::symmetric(6, 3))
                .show(ui, |ui| {
                    ui.label(RichText::new(path).monospace().size(11.0).color(uc().cyan));
                });
        }
    })
}

fn label_with_tip(ui: &mut Ui, label: &str, tip: &str, tech_path: Option<&str>) {
    let resp = ui.add(
        egui::Label::new(
            RichText::new(format!("{label} ⓘ"))
                .size(13.0)
                .strong()
                .color(uc().text_primary),
        )
        .sense(Sense::hover()),
    );
    aorus_tooltip(resp, label, tip, tech_path);
}

fn segmented_pill(
    ui: &mut Ui,
    label: &str,
    selected: bool,
    active_fill: Color32,
    active_border: Color32,
) -> Response {
    let btn = egui::Button::new(RichText::new(label).size(12.0).strong().color(if selected {
        Color32::WHITE
    } else {
        uc().text_secondary
    }))
    .fill(if selected {
        active_fill
    } else {
        uc().btn_slate
    })
    .stroke(Stroke::new(
        if selected { 1.4_f32 } else { 1.0_f32 },
        if selected {
            active_border
        } else {
            uc().border_subtle
        },
    ))
    .corner_radius(CornerRadius::same(4));
    ui.add(btn)
}

fn section_group_header(ui: &mut Ui, title: &str, color: Color32) {
    ui.add_space(2.0);
    ui.label(RichText::new(title).size(12.5).strong().color(color));
    ui.add_space(2.0);
}

fn utility_action_row(
    ui: &mut Ui,
    title: &str,
    desc: &str,
    btn_label: &str,
    tip: &str,
    tech_path: &str,
    on_click: impl FnOnce(),
) {
    egui::Frame::new()
        .fill(uc().bg_subgroup)
        .stroke(Stroke::new(1.0_f32, uc().border_subtle))
        .corner_radius(CornerRadius::same(5))
        .inner_margin(egui::Margin::symmetric(10, 8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                ui.vertical(|ui| {
                    ui.label(
                        RichText::new(title)
                            .size(13.0)
                            .strong()
                            .color(Color32::WHITE),
                    );
                    ui.label(RichText::new(desc).size(11.5).color(uc().text_secondary));
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let resp = aorus_action_button(ui, btn_label, uc().btn_slate, uc().orange);
                    if resp.clicked() {
                        on_click();
                    }
                    aorus_tooltip(resp, title, tip, Some(tech_path));
                });
            });
        });
    ui.add_space(4.0);
}

/// Renders 2 columns side-by-side when available width >= 780px, or stacks them vertically on smaller windows.
fn responsive_two_columns<S>(
    ui: &mut Ui,
    state: &mut S,
    left: impl FnOnce(&mut S, &mut Ui),
    right: impl FnOnce(&mut S, &mut Ui),
) {
    if ui.available_width() >= 780.0 {
        ui.columns(2, |cols| {
            let (left_slice, right_slice) = cols.split_at_mut(1);
            left(state, &mut left_slice[0]);
            right(state, &mut right_slice[0]);
        });
    } else {
        left(state, ui);
        ui.add_space(12.0);
        right(state, ui);
    }
}

fn adapt_slider_width(ui: &mut Ui) {
    let w = (ui.available_width() * 0.36).clamp(85.0, 200.0);
    ui.spacing_mut().slider_width = w;
}

fn aorus_action_button(ui: &mut Ui, label: &str, fill: Color32, border: Color32) -> Response {
    let btn = egui::Button::new(
        RichText::new(label)
            .size(12.5)
            .strong()
            .color(Color32::WHITE),
    )
    .fill(fill)
    .stroke(Stroke::new(1.2_f32, border));
    ui.add(btn)
}

fn aorus_full_cta_button(ui: &mut Ui, label: &str, fill: Color32, border: Color32) -> Response {
    let btn = egui::Button::new(
        RichText::new(label)
            .size(13.5)
            .strong()
            .color(Color32::WHITE),
    )
    .fill(fill)
    .stroke(Stroke::new(1.5_f32, border))
    .min_size(Vec2::new(ui.available_width(), 36.0));
    ui.add(btn)
}

fn high_contrast_code_box(ui: &mut Ui, id: &str, content: &str, max_h: f32) {
    egui::Frame::new()
        .fill(Color32::from_rgb(9, 13, 21))
        .stroke(Stroke::new(1.0_f32, uc().border_subtle))
        .corner_radius(CornerRadius::same(4))
        .inner_margin(egui::Margin::same(8))
        .show(ui, |ui| {
            egui::ScrollArea::vertical()
                .id_salt(id)
                .max_height(max_h)
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(
                        RichText::new(content)
                            .monospace()
                            .size(12.0)
                            .color(Color32::from_rgb(226, 232, 240)),
                    );
                });
        });
}

fn aorus_card(ui: &mut Ui, title: &str, accent: Color32, add_contents: impl FnOnce(&mut Ui)) {
    let card_fill = ui.visuals().faint_bg_color;
    let card_stroke = ui.visuals().widgets.noninteractive.bg_stroke;
    let card_radius = ui.visuals().widgets.noninteractive.corner_radius;
    let is_kde = card_fill != uc().bg_card;
    let bar_col = if is_kde { uc().cyan } else { accent };

    egui::Frame::new()
        .fill(card_fill)
        .stroke(card_stroke)
        .corner_radius(card_radius)
        .inner_margin(egui::Margin::same(12))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                let (bar_rect, _) = ui.allocate_exact_size(Vec2::new(4.0, 16.0), Sense::hover());
                ui.painter()
                    .rect_filled(bar_rect, CornerRadius::same(2), bar_col);
                ui.label(
                    RichText::new(title)
                        .size(13.5)
                        .strong()
                        .color(uc().text_primary),
                );
            });
            if is_kde {
                ui.add_space(4.0);
                ui.separator();
                ui.add_space(4.0);
            } else {
                ui.add_space(8.0);
            }
            add_contents(ui);
        });
}

fn aorus_card_with_tip(
    ui: &mut Ui,
    title: &str,
    tip: &str,
    accent: Color32,
    add_contents: impl FnOnce(&mut Ui),
) {
    let card_fill = ui.visuals().faint_bg_color;
    let card_stroke = ui.visuals().widgets.noninteractive.bg_stroke;
    let card_radius = ui.visuals().widgets.noninteractive.corner_radius;
    let is_kde = card_fill != uc().bg_card;
    let bar_col = if is_kde { uc().cyan } else { accent };

    egui::Frame::new()
        .fill(card_fill)
        .stroke(card_stroke)
        .corner_radius(card_radius)
        .inner_margin(egui::Margin::same(12))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                let (bar_rect, _) = ui.allocate_exact_size(Vec2::new(4.0, 16.0), Sense::hover());
                ui.painter()
                    .rect_filled(bar_rect, CornerRadius::same(2), bar_col);
                let resp = ui.add(
                    egui::Label::new(
                        RichText::new(format!("{title} ⓘ"))
                            .size(13.5)
                            .strong()
                            .color(uc().text_primary),
                    )
                    .sense(Sense::hover()),
                );
                aorus_tooltip(resp, title, tip, None);
            });
            if is_kde {
                ui.add_space(4.0);
                ui.separator();
                ui.add_space(4.0);
            } else {
                ui.add_space(8.0);
            }
            add_contents(ui);
        });
}

fn draw_radial_gauge(
    ui: &mut Ui,
    ratio: f32,
    main_value: &str,
    unit_label: &str,
    sub_label: &str,
    accent: Color32,
) -> Response {
    let desired = Vec2::new(ui.available_width(), 160.0);
    let (rect, resp) = ui.allocate_exact_size(desired, Sense::hover());
    let painter = ui.painter_at(rect);

    let center = rect.center() + Vec2::new(0.0, 4.0);
    let radius = (rect.height().min(rect.width()) * 0.39).clamp(38.0, 58.0);

    let start_angle = 0.75 * PI;
    let sweep = 1.5 * PI;

    draw_arc(
        &painter,
        center,
        radius,
        start_angle,
        sweep,
        Stroke::new(9.0_f32, gauge_track()),
    );

    let ticks = 18;
    for i in 0..=ticks {
        let t = i as f32 / ticks as f32;
        let ang = start_angle + t * sweep;
        let dir = Vec2::new(ang.cos(), ang.sin());
        let p1 = center + dir * (radius + 8.0);
        let p2 = center + dir * (radius + 12.0);
        let tick_col = if t <= ratio { accent } else { gauge_tick() };
        painter.line_segment([p1, p2], Stroke::new(1.5_f32, tick_col));
    }

    if ratio > 0.005 {
        draw_arc(
            &painter,
            center,
            radius,
            start_angle,
            sweep * ratio.clamp(0.0, 1.0),
            Stroke::new(9.0_f32, accent),
        );
    }

    painter.text(
        center + Vec2::new(0.0, -6.0),
        Align2::CENTER_CENTER,
        main_value,
        FontId::new(24.0, FontFamily::Proportional),
        uc().text_primary,
    );
    painter.text(
        center + Vec2::new(0.0, 15.0),
        Align2::CENTER_CENTER,
        unit_label,
        FontId::new(12.5, FontFamily::Proportional),
        accent,
    );

    painter.text(
        Pos2::new(center.x, rect.bottom() - 7.0),
        Align2::CENTER_CENTER,
        sub_label,
        FontId::new(12.0, FontFamily::Proportional),
        uc().text_secondary,
    );

    resp
}

fn draw_arc(
    painter: &egui::Painter,
    center: Pos2,
    radius: f32,
    start_angle: f32,
    sweep_angle: f32,
    stroke: Stroke,
) {
    let steps = 40;
    let mut pts = Vec::with_capacity(steps + 1);
    for i in 0..=steps {
        let a = start_angle + (i as f32 / steps as f32) * sweep_angle;
        pts.push(center + Vec2::new(a.cos() * radius, a.sin() * radius));
    }
    painter.add(Shape::line(pts, stroke));
}

fn draw_multi_telemetry_chart(
    ui: &mut Ui,
    temp_hist: &VecDeque<f32>,
    cpu_hist: &VecDeque<f32>,
    fan_hist: &VecDeque<f32>,
) {
    let desired = Vec2::new(ui.available_width(), 195.0);
    let (rect, _) = ui.allocate_exact_size(desired, Sense::hover());
    let painter = ui.painter_at(rect);

    painter.rect_filled(rect, CornerRadius::same(4), uc().bg_subgroup);
    painter.rect_stroke(
        rect,
        CornerRadius::same(4),
        Stroke::new(1.0_f32, uc().border_subtle),
        StrokeKind::Inside,
    );

    let plot_rect = rect.shrink2(Vec2::new(36.0, 14.0));

    for i in 0..=4 {
        let frac = i as f32 / 4.0;
        let y = plot_rect.bottom() - frac * plot_rect.height();
        painter.line_segment(
            [
                Pos2::new(plot_rect.left(), y),
                Pos2::new(plot_rect.right(), y),
            ],
            Stroke::new(1.0_f32, chart_grid()),
        );
        painter.text(
            Pos2::new(plot_rect.left() - 6.0, y),
            Align2::RIGHT_CENTER,
            format!("{}", i * 25),
            FontId::new(11.0, FontFamily::Monospace),
            uc().text_secondary,
        );
    }

    let plot_series = |hist: &VecDeque<f32>, color: Color32| {
        if hist.len() < 2 {
            return;
        }
        let n = hist.len();
        let pts: Vec<Pos2> = hist
            .iter()
            .enumerate()
            .map(|(i, &val)| {
                let x = plot_rect.left() + (i as f32 / (n - 1) as f32) * plot_rect.width();
                let y = plot_rect.bottom() - (val.clamp(0.0, 100.0) / 100.0) * plot_rect.height();
                Pos2::new(x, y)
            })
            .collect();
        painter.add(Shape::line(pts, Stroke::new(2.2_f32, color)));
    };

    plot_series(fan_hist, uc().green);
    plot_series(cpu_hist, uc().orange);
    plot_series(temp_hist, uc().cyan);
}

fn draw_interactive_fan_curve(
    ui: &mut Ui,
    points: &mut [FanCurvePoint],
    dragging_idx: &mut Option<usize>,
    cur_temp_c: f32,
) {
    let desired = Vec2::new(ui.available_width(), 275.0);
    let (rect, response) = ui.allocate_exact_size(desired, Sense::click_and_drag());
    let painter = ui.painter_at(rect);

    painter.rect_filled(rect, CornerRadius::same(6), uc().bg_subgroup);
    painter.rect_stroke(
        rect,
        CornerRadius::same(6),
        Stroke::new(1.0_f32, uc().border_subtle),
        StrokeKind::Inside,
    );

    let pad_left = 46.0;
    let pad_right = 18.0;
    let pad_top = 20.0;
    let pad_bottom = 28.0;
    let plot = Rect::from_min_max(
        Pos2::new(rect.left() + pad_left, rect.top() + pad_top),
        Pos2::new(rect.right() - pad_right, rect.bottom() - pad_bottom),
    );

    let min_t = 25.0f32;
    let max_t = 90.0f32;

    let to_screen = |temp: f32, speed: f32| -> Pos2 {
        let tx = ((temp - min_t) / (max_t - min_t)).clamp(0.0, 1.0);
        let sy = (speed / 100.0).clamp(0.0, 1.0);
        Pos2::new(
            plot.left() + tx * plot.width(),
            plot.bottom() - sy * plot.height(),
        )
    };

    let from_screen = |pos: Pos2| -> (f32, u8) {
        let tx = ((pos.x - plot.left()) / plot.width()).clamp(0.0, 1.0);
        let sy = ((plot.bottom() - pos.y) / plot.height()).clamp(0.0, 1.0);
        let temp = (min_t + tx * (max_t - min_t)).round();
        let speed = (sy * 100.0).round() as u8;
        (temp, speed)
    };

    for s in [0, 25, 50, 75, 100] {
        let p = to_screen(min_t, s as f32);
        painter.line_segment(
            [Pos2::new(plot.left(), p.y), Pos2::new(plot.right(), p.y)],
            Stroke::new(1.0_f32, chart_grid()),
        );
        painter.text(
            Pos2::new(plot.left() - 6.0, p.y),
            Align2::RIGHT_CENTER,
            format!("{s}%"),
            FontId::new(11.0, FontFamily::Monospace),
            uc().text_secondary,
        );
    }

    for t in [30, 40, 50, 60, 70, 80, 90] {
        let p = to_screen(t as f32, 0.0);
        painter.line_segment(
            [Pos2::new(p.x, plot.top()), Pos2::new(p.x, plot.bottom())],
            Stroke::new(1.0_f32, chart_grid()),
        );
        painter.text(
            Pos2::new(p.x, plot.bottom() + 13.0),
            Align2::CENTER_CENTER,
            format!("{t}°C"),
            FontId::new(11.0, FontFamily::Monospace),
            uc().text_secondary,
        );
    }

    let cur_x = to_screen(cur_temp_c, 0.0).x;
    painter.line_segment(
        [
            Pos2::new(cur_x, plot.top()),
            Pos2::new(cur_x, plot.bottom()),
        ],
        Stroke::new(1.5_f32, uc().cyan),
    );

    points.sort_by(|a, b| {
        a.temp_c
            .partial_cmp(&b.temp_c)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    if let Some(pointer_pos) = response.interact_pointer_pos() {
        if response.drag_started() {
            let mut closest = None;
            let mut min_dist = 20.0f32;
            for (i, pt) in points.iter().enumerate() {
                let sp = to_screen(pt.temp_c, pt.speed_pct as f32);
                let d = sp.distance(pointer_pos);
                if d < min_dist {
                    min_dist = d;
                    closest = Some(i);
                }
            }
            *dragging_idx = closest;
        }
        if response.dragged() {
            if let Some(idx) = *dragging_idx {
                if idx < points.len() {
                    let (new_t, new_s) = from_screen(pointer_pos);
                    points[idx].temp_c = new_t;
                    points[idx].speed_pct = if new_s < 15 { 0 } else { new_s.max(25) };
                }
            }
        }
    }
    if response.drag_stopped() {
        *dragging_idx = None;
    }

    if !points.is_empty() {
        let mut line_pts = Vec::new();
        line_pts.push(to_screen(min_t, 0.0));
        let first_x = to_screen(points[0].temp_c, 0.0);
        line_pts.push(first_x);
        for pt in points.iter() {
            line_pts.push(to_screen(pt.temp_c, pt.speed_pct as f32));
        }
        let last_s = points.last().map(|p| p.speed_pct as f32).unwrap_or(100.0);
        line_pts.push(to_screen(max_t, last_s));

        painter.add(Shape::line(line_pts, Stroke::new(2.5_f32, uc().orange)));
    }

    for (i, pt) in points.iter().enumerate() {
        let sp = to_screen(pt.temp_c, pt.speed_pct as f32);
        let is_dragged = *dragging_idx == Some(i);
        let r = if is_dragged { 8.5 } else { 7.0 };
        painter.circle_filled(sp, r, uc().orange);
        painter.circle_stroke(sp, r, Stroke::new(2.0_f32, uc().text_primary));
        painter.text(
            sp + Vec2::new(0.0, -14.0),
            Align2::CENTER_CENTER,
            format!("{:.0}°C/{}%", pt.temp_c, pt.speed_pct),
            FontId::new(11.5, FontFamily::Proportional),
            uc().text_primary,
        );
    }
}

fn draw_progress_bar_row(
    ui: &mut Ui,
    label: &str,
    fraction: f32,
    right_text: &str,
    color: Color32,
) {
    ui.horizontal_wrapped(|ui| {
        ui.label(
            RichText::new(label)
                .size(12.5)
                .strong()
                .color(uc().text_primary),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                RichText::new(right_text)
                    .size(12.0)
                    .strong()
                    .color(uc().text_secondary),
            );
        });
    });
    let desired = Vec2::new(ui.available_width(), 9.0);
    let (rect, _) = ui.allocate_exact_size(desired, Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, CornerRadius::same(3), uc().bg_subgroup);
    let fill_w = (rect.width() * fraction.clamp(0.0, 1.0)).max(2.0);
    let fill_rect = Rect::from_min_size(rect.min, Vec2::new(fill_w, rect.height()));
    painter.rect_filled(fill_rect, CornerRadius::same(3), color);
}

fn legend_badge(ui: &mut Ui, text: &str, color: Color32) {
    ui.horizontal(|ui| {
        let (r, _) = ui.allocate_exact_size(Vec2::new(11.0, 11.0), Sense::hover());
        ui.painter().rect_filled(r, CornerRadius::same(2), color);
        ui.label(
            RichText::new(text)
                .size(12.0)
                .strong()
                .color(uc().text_primary),
        );
    });
}

fn spec_row(ui: &mut Ui, key: &str, val: &str) {
    ui.label(RichText::new(key).size(12.5).color(uc().text_secondary));
    ui.label(
        RichText::new(val)
            .size(12.5)
            .strong()
            .color(uc().text_primary),
    );
    ui.end_row();
}

fn feature_toggle_row(
    ui: &mut Ui,
    title: &str,
    description: &str,
    config_key: &str,
    state: &mut bool,
) {
    let resp = egui::Frame::new()
        .fill(if *state {
            uc().bg_card_hover
        } else {
            uc().bg_subgroup
        })
        .stroke(Stroke::new(
            1.1_f32,
            if *state {
                uc().orange
            } else {
                uc().border_subtle
            },
        ))
        .corner_radius(CornerRadius::same(5))
        .inner_margin(egui::Margin::symmetric(10, 6))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.checkbox(state, "");
                ui.vertical(|ui| {
                    ui.label(
                        RichText::new(format!("{title} ⓘ"))
                            .size(12.5)
                            .strong()
                            .color(uc().text_primary),
                    );
                    ui.label(
                        RichText::new(description)
                            .size(11.5)
                            .color(uc().text_secondary),
                    );
                });
            });
        })
        .response;
    aorus_tooltip(resp, title, description, Some(config_key));
    ui.add_space(3.0);
}

fn gauge_track() -> Color32 {
    let c = uc();
    crate::system_colors::mix(c.bg_card, c.text_primary, 0.16)
}

fn gauge_tick() -> Color32 {
    let c = uc();
    crate::system_colors::mix(c.bg_card, c.text_primary, 0.32)
}

fn chart_grid() -> Color32 {
    let c = uc();
    crate::system_colors::mix(c.bg_subgroup, c.text_primary, 0.12)
}

/// Subtle accent-tinted surface for banners (works on dark and light palettes).
fn tint(accent: Color32) -> Color32 {
    crate::system_colors::mix(uc().bg_card, accent, 0.14)
}
