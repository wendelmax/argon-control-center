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
    apply_runtime_cpufreq, get_boot_config_path, load_boot_config, save_boot_config_scoped,
    update_boot_config_str, BootOcConfig, ConfigScope, OcPreset,
};
use crate::privileged::is_root;
use crate::settings::{ColorScheme, DashboardStyle, Layout};
use crate::telemetry::TelemetryState;
use crate::theme::{
    configure_kde_breeze_style, draw_kde_area_line_chart, draw_kde_donut_chart, set_ui_colors, uc,
    KdeBreezePalette, UiColors,
};
use egui::{
    Align2, Color32, CornerRadius, FontFamily, FontId, Pos2, Rect, Response, RichText, Sense,
    Shape, Stroke, StrokeKind, TextStyle, Ui, Vec2,
};
use std::collections::VecDeque;
use std::f32::consts::PI;
use std::fs;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ActiveTab {
    Dashboard,
    SmartFan,
    Overclocking,
    Features,
    GpioLab,
    ServicesAndConfig,
    Settings,
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
    pub layout: Layout,
    pub colors: ColorScheme,
    pub dashboard: DashboardStyle,
    pub ui_scale: f32,
    pub sidebar_collapsed: bool,
    pub kde_palette: KdeBreezePalette,
    palette_rx: Option<std::sync::mpsc::Receiver<KdeBreezePalette>>,
    last_palette_check: Instant,
    pub active_tab: ActiveTab,
    saved_prefs: crate::settings::Settings,
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
    trial_status: crate::tryboot::Status,
    power_override: bool,
    accept_oc_risk: bool,
    stability: Option<crate::stability::StabilityTest>,
    stability_minutes: u32,
    last_stability: Option<crate::stability::Report>,
    pub runtime_governor: String,
    pub runtime_min_mhz: u32,
    pub runtime_max_mhz: u32,
    pub runtime_boost: bool,

    // Units & OLED
    /// Display unit in the app (saved preference).
    pub temp_unit: TempUnit,
    /// Unit configured for the Argon daemon / OLED (`/etc/argonunits.conf`).
    pub argon_unit: TempUnit,
    pub oled_cfg: OledConfig,
    pub oled_hw_present: bool,

    // Services & Inspector
    pub config_preview_tab: ConfigPreviewTab,
    pub gpio_lab: GpioSerialLabState,
    gpio_view_diagram: bool,
    pinout: Option<PinoutTexture>,
    pinout_rx: Option<std::sync::mpsc::Receiver<Option<crate::pinout::Pinout>>>,

    // Status & Logs
    pub status_message: Option<String>,
    pub status_is_error: bool,
    pub action_logs: VecDeque<String>,
}

impl ArgonControlCenterApp {
    pub fn new(cc: &eframe::CreationContext<'_>, lang_override: Option<Language>) -> Self {
        let prefs = crate::settings::load();
        let initial_lang = lang_override.unwrap_or(prefs.language);
        let kde_palette = KdeBreezePalette::load_from_system();
        cc.egui_ctx.set_zoom_factor(prefs.ui_scale);

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
        let argon_unit = load_temp_unit();
        let oled_cfg = load_oled_config();
        let oled_hw_present = is_oled_hardware_installed();

        let runtime_governor = telemetry.cpu_governor.clone();
        let runtime_min_mhz = telemetry.cpu_min_freq_mhz;
        let runtime_max_mhz = telemetry.cpu_max_freq_mhz;
        let runtime_boost = telemetry.cpu_boost_enabled;

        let mut logs = VecDeque::new();
        logs.push_back(format!(
            "[INIT] Argon Control Center — {} ({})",
            telemetry.board_model, telemetry.os_name
        ));
        logs.push_back(format!(
            "[INIT] Fan Curve: {} ({} stages)",
            CPU_FAN_CONF,
            cpu_fan_curve.len()
        ));

        let app = Self {
            lang: initial_lang,
            layout: prefs.layout,
            colors: prefs.colors,
            dashboard: prefs.dashboard,
            ui_scale: prefs.ui_scale,
            sidebar_collapsed: prefs.sidebar_collapsed,
            kde_palette,
            palette_rx: None,
            last_palette_check: Instant::now(),
            active_tab: prefs.tab,
            saved_prefs: prefs,
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
            trial_status: crate::tryboot::status(),
            power_override: false,
            accept_oc_risk: false,
            stability: None,
            stability_minutes: 3,
            last_stability: crate::stability::last_report(),
            runtime_governor,
            runtime_min_mhz,
            runtime_max_mhz,
            runtime_boost,
            temp_unit: prefs.temp_unit,
            argon_unit,
            oled_cfg,
            oled_hw_present,
            config_preview_tab: ConfigPreviewTab::ArgonFanConf,
            gpio_lab: GpioSerialLabState::new(),
            gpio_view_diagram: true,
            pinout: None,
            pinout_rx: {
                // Pre-render the GPIO pin-out diagram while the app starts.
                let (tx, rx) = std::sync::mpsc::channel();
                let ctx = cc.egui_ctx.clone();
                std::thread::spawn(move || {
                    let _ = tx.send(crate::pinout::render(900));
                    ctx.request_repaint();
                });
                Some(rx)
            },
            status_message: None,
            status_is_error: false,
            action_logs: logs,
        };
        app.apply_style(&cc.egui_ctx);
        app
    }

    /// Palette used for drawing: the desktop palette, or the AORUS palette expressed in the
    /// same shape so every layout and dashboard style supports both color schemes.
    pub fn active_palette(&self) -> KdeBreezePalette {
        match self.colors {
            ColorScheme::System => self.kde_palette,
            ColorScheme::Aorus => KdeBreezePalette::aorus(),
        }
    }

    /// Applies the egui style for the current color scheme.
    pub fn apply_style(&self, ctx: &egui::Context) {
        match self.colors {
            ColorScheme::System => configure_kde_breeze_style(ctx, &self.kde_palette),
            ColorScheme::Aorus => configure_aorus_style(ctx),
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
        set_ui_colors(match self.colors {
            ColorScheme::Aorus => UiColors::AORUS,
            ColorScheme::System => UiColors::from_palette(&self.kde_palette),
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
        if let Some(rx) = &self.pinout_rx {
            match rx.try_recv() {
                Ok(rendered) => {
                    self.pinout = Some(PinoutTexture::from_render(ctx, rendered));
                    self.pinout_rx = None;
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => self.pinout_rx = None,
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
            }
        }
        self.persist_prefs_if_changed();

        let pal = self.active_palette();
        let wide = ctx.screen_rect().width() >= SIDEBAR_MIN_WIDTH;

        // Footer first: panels added earlier take the full window width.
        egui::TopBottomPanel::bottom("status_bar")
            .frame(
                egui::Frame::new()
                    .fill(pal.window_alt_bg)
                    .stroke(Stroke::new(1.0_f32, pal.border_subtle))
                    .inner_margin(egui::Margin::symmetric(14, 5)),
            )
            .show(ctx, |ui| {
                self.draw_bottom_bar(ui);
            });

        match self.layout {
            Layout::Sidebar => {
                // Kirigami-style left navigation; on narrow windows the header shows a tab strip.
                if wide {
                    egui::SidePanel::left("kde_kirigami_sidebar")
                        .resizable(false)
                        .exact_width(if self.sidebar_collapsed { 52.0 } else { 176.0 })
                        .frame(
                            egui::Frame::new()
                                .fill(pal.window_alt_bg)
                                .stroke(Stroke::new(1.0_f32, pal.border_subtle))
                                .inner_margin(egui::Margin::symmetric(6, 8)),
                        )
                        .show(ctx, |ui| {
                            self.draw_kde_sidebar(ui, ctx);
                        });
                }
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
            }
            Layout::TopTabs => {
                egui::TopBottomPanel::top("aorus_top_bar")
                    .frame(
                        egui::Frame::new()
                            .fill(pal.header_bg)
                            .stroke(Stroke::new(1.0_f32, pal.border_subtle))
                            .inner_margin(egui::Margin::symmetric(14, 8)),
                    )
                    .show(ctx, |ui| {
                        self.draw_top_bar(ui, ctx);
                    });
            }
        }

        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(pal.window_bg)
                    .inner_margin(egui::Margin::symmetric(14, 10)),
            )
            .show(ctx, |ui| {
                // Vertical scrolling only: every page adapts its width to the window, so a
                // horizontal scrollbar would just expose rounding overflow.
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| match self.active_tab {
                        ActiveTab::Dashboard => match self.dashboard {
                            DashboardStyle::Compact => self.draw_kde_compact_overview_tab(ui),
                            DashboardStyle::Charts => self.draw_kde_dashboard_tab(ui),
                            DashboardStyle::Gauges => self.draw_dashboard_tab(ui),
                        },
                        ActiveTab::SmartFan => self.draw_smart_fan_tab(ui),
                        ActiveTab::Overclocking => self.draw_overclocking_tab(ui),
                        ActiveTab::Features => self.draw_features_tab(ui),
                        ActiveTab::GpioLab => self.draw_gpio_lab_tab(ui),
                        ActiveTab::ServicesAndConfig => self.draw_services_tab(ui),
                        ActiveTab::Settings => self.draw_settings_tab(ui, ctx),
                    });
            });
    }
}

/// Below this width the sidebar layout hides the sidebar and shows a tab strip in the header.
const SIDEBAR_MIN_WIDTH: f32 = 760.0;

impl ArgonControlCenterApp {
    /// Saves the preferences to `settings.json` whenever one of them changes.
    fn persist_prefs_if_changed(&mut self) {
        let now = crate::settings::Settings {
            language: self.lang,
            layout: self.layout,
            colors: self.colors,
            dashboard: self.dashboard,
            temp_unit: self.temp_unit,
            ui_scale: self.ui_scale,
            sidebar_collapsed: self.sidebar_collapsed,
            tab: self.active_tab,
        };
        if now != self.saved_prefs {
            if let Err(e) = crate::settings::save(&now) {
                eprintln!("settings: {e}");
            }
            self.saved_prefs = now;
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
                        if self.colors == ColorScheme::System {
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
        if self.colors == ColorScheme::Aorus
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
        let pal = self.active_palette();
        let collapsed = self.sidebar_collapsed;

        // Header: logo (+ name when expanded) and the collapse / expand toggle.
        let logo = |ui: &mut Ui| {
            let (logo_rect, _) = ui.allocate_exact_size(Vec2::splat(26.0), Sense::hover());
            ui.painter()
                .rect_filled(logo_rect, CornerRadius::same(4), pal.selection_bg);
            ui.painter().text(
                logo_rect.center(),
                Align2::CENTER_CENTER,
                "⚙",
                FontId::new(15.0, FontFamily::Proportional),
                crate::system_colors::on_color(pal.selection_bg),
            );
        };
        let mut toggle = false;
        if collapsed {
            ui.vertical_centered(|ui| {
                logo(ui);
                ui.add_space(4.0);
                toggle = ui
                    .add(egui::Button::new(RichText::new("»").size(14.0)).frame(false))
                    .on_hover_text(t.sidebar_expand)
                    .clicked();
            });
        } else {
            ui.horizontal(|ui| {
                logo(ui);
                ui.label(
                    RichText::new("Argon")
                        .size(14.5)
                        .strong()
                        .color(pal.text_normal),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    toggle = ui
                        .add(egui::Button::new(RichText::new("«").size(14.0)).frame(false))
                        .on_hover_text(t.sidebar_collapse)
                        .clicked();
                });
            });
        }
        if toggle {
            self.sidebar_collapsed = !self.sidebar_collapsed;
        }

        ui.add_space(4.0);
        ui.separator();
        ui.add_space(2.0);

        // Navigation: full labels, or icons only (name in the tooltip) when collapsed.
        for (tab, label, tip) in nav_tabs(self.lang) {
            let selected = self.active_tab == tab;
            let (rect, resp) =
                ui.allocate_exact_size(Vec2::new(ui.available_width(), 30.0), Sense::click());
            let painter = ui.painter_at(rect);
            if selected {
                painter.rect_filled(rect, CornerRadius::same(4), pal.selected_fill);
                let bar = Rect::from_min_size(
                    Pos2::new(rect.left() + 1.0, rect.top() + 6.0),
                    Vec2::new(3.0, rect.height() - 12.0),
                );
                painter.rect_filled(bar, CornerRadius::same(2), pal.focus_blue);
            } else if resp.hovered() {
                painter.rect_filled(rect, CornerRadius::same(4), pal.hover_bg);
            }
            let color = if selected {
                pal.on_selected
            } else {
                pal.text_normal
            };
            if collapsed {
                let icon = label.split_once(' ').map(|(i, _)| i).unwrap_or(label);
                painter.text(
                    rect.center(),
                    Align2::CENTER_CENTER,
                    icon,
                    FontId::new(16.0, FontFamily::Proportional),
                    color,
                );
            } else {
                painter.text(
                    Pos2::new(rect.left() + 10.0, rect.center().y),
                    Align2::LEFT_CENTER,
                    label,
                    FontId::new(13.0, FontFamily::Proportional),
                    color,
                );
            }
            if resp.clicked() {
                self.active_tab = tab;
            }
            if resp.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            aorus_tooltip(resp, label, tip, None);
            ui.add_space(1.0);
        }
    }

    fn draw_kde_page_header(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        let pal = self.active_palette();
        let title = nav_tabs(self.lang)
            .iter()
            .find(|(tab, _, _)| *tab == self.active_tab)
            .map(|(_, label, _)| *label)
            .unwrap_or_default();

        ui.label(
            RichText::new(title)
                .size(17.0)
                .strong()
                .color(pal.text_normal),
        );

        // Narrow window: the sidebar is hidden, so show a wrapping tab strip instead.
        if ctx.screen_rect().width() < SIDEBAR_MIN_WIDTH {
            ui.add_space(4.0);
            self.draw_tab_strip(ui, false);
        }
    }

    /// Wrapping row of navigation buttons. `large` uses the wide classic buttons.
    /// Uses the same palette roles as the sidebar so the layout never changes the colors.
    fn draw_tab_strip(&mut self, ui: &mut Ui, large: bool) {
        let pal = self.active_palette();
        let accent_fill = pal.selected_fill;
        let accent = pal.focus_blue;
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(6.0, 6.0);
            for (tab, title, tip) in nav_tabs(self.lang) {
                let is_active = self.active_tab == tab;
                let text = RichText::new(title)
                    .size(if large { 13.0 } else { 12.0 })
                    .strong()
                    .color(if is_active {
                        pal.on_selected
                    } else {
                        uc().text_secondary
                    });
                let mut btn = egui::Button::new(text)
                    .fill(if is_active {
                        accent_fill
                    } else {
                        Color32::TRANSPARENT
                    })
                    .stroke(if is_active {
                        Stroke::new(1.5_f32, accent)
                    } else {
                        Stroke::NONE
                    });
                if large {
                    btn = btn.min_size(Vec2::new(120.0, 30.0));
                }
                let resp = ui.add(btn);
                if resp.clicked() {
                    self.active_tab = tab;
                }
                aorus_tooltip(resp, title, tip, None);
            }
        });
    }

    fn draw_top_bar(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        let pal = self.active_palette();
        let narrow = ctx.screen_rect().width() < SIDEBAR_MIN_WIDTH;

        // Same branding as the sidebar: logo and accent come from the active palette.
        // Status (daemon, 5 V, board) lives in the footer.
        ui.horizontal(|ui| {
            let (logo_rect, _) = ui.allocate_exact_size(Vec2::new(30.0, 30.0), Sense::hover());
            let painter = ui.painter();
            painter.rect_filled(logo_rect, CornerRadius::same(5), pal.selection_bg);
            painter.text(
                logo_rect.center(),
                Align2::CENTER_CENTER,
                "⚙",
                FontId::new(17.0, FontFamily::Proportional),
                crate::system_colors::on_color(pal.selection_bg),
            );
            ui.label(
                RichText::new("Argon")
                    .size(18.0)
                    .strong()
                    .color(pal.text_normal),
            );
            ui.label(
                RichText::new("Control Center")
                    .size(18.0)
                    .strong()
                    .color(pal.focus_blue),
            );
        });

        ui.add_space(6.0);
        egui::Frame::new()
            .fill(uc().bg_subgroup)
            .stroke(Stroke::new(1.0_f32, uc().border_subtle))
            .corner_radius(CornerRadius::same(6))
            .inner_margin(egui::Margin::symmetric(6, 4))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                self.draw_tab_strip(ui, !narrow);
            });
    }

    /// Footer (full window width): action status, daemon / 5 V / network / session, and a
    /// compact system line. Replaces the sidebar status box and the top-bar badges.
    fn draw_bottom_bar(&mut self, ui: &mut Ui) {
        let t = texts(self.lang);
        let pal = self.active_palette();
        let mut open_services = false;
        ui.horizontal_wrapped(|ui| {
            let msg = self.status_message.as_deref().unwrap_or(t.ready_status);
            let status_color = if self.status_is_error {
                uc().red
            } else {
                uc().cyan
            };
            ui.label(RichText::new(msg).size(12.0).strong().color(status_color));

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 10.0;
                let priv_label = if is_root() {
                    t.mode_root
                } else {
                    t.mode_polkit
                };
                let mode = ui.add(
                    egui::Label::new(
                        RichText::new(priv_label)
                            .size(11.5)
                            .color(uc().text_secondary),
                    )
                    .sense(Sense::hover()),
                );
                aorus_tooltip(mode, t.mode_label, t.mode_tip, Some("/usr/bin/pkexec"));
                ui.label(
                    RichText::new(format!(
                        "{} {}",
                        t.uptime_label,
                        self.telemetry.format_uptime()
                    ))
                    .size(11.5)
                    .color(uc().text_secondary),
                );
                ui.label(
                    RichText::new(format!("IP {}", self.telemetry.ip_address))
                        .size(11.5)
                        .color(uc().text_secondary),
                );

                let (v_col, v_txt) = if self.telemetry.undervoltage_alarm {
                    (pal.negative_red, t.voltage_alert)
                } else {
                    (pal.positive_green, t.voltage_stable)
                };
                let alarm_path = crate::power::alarm_path()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "rpi_volt in0_lcrit_alarm".into());
                let v = ui.add(
                    egui::Label::new(RichText::new(v_txt).size(11.5).strong().color(v_col))
                        .sense(Sense::hover()),
                );
                aorus_tooltip(v, "5V Power Rail", t.voltage_tip, Some(&alarm_path));

                let (d_col, d_txt) = if self.telemetry.argon_service_active {
                    (pal.positive_green, t.daemon_online)
                } else {
                    (pal.negative_red, t.daemon_offline)
                };
                let d = ui.add(
                    egui::Button::new(RichText::new(d_txt).size(11.5).strong().color(d_col))
                        .frame(false),
                );
                if d.clicked() {
                    open_services = true;
                }
                aorus_tooltip(
                    d,
                    "Systemd Daemon (argononed.service)",
                    t.daemon_badge_tip,
                    Some("/lib/systemd/system/argononed.service"),
                );
            });
        });
        if open_services {
            self.active_tab = ActiveTab::ServicesAndConfig;
        }

        // Compact system line; full details on hover.
        let (short, details) = self.system_summary();
        let board = crate::board::current();
        let resp = ui.add(
            egui::Label::new(RichText::new(short).size(11.0).color(uc().text_secondary))
                .truncate()
                .sense(Sense::hover()),
        );
        aorus_tooltip(
            resp,
            t.card_specs,
            &format!("{details}\n\n{}", board.details()),
            None,
        );
    }

    // Compact dashboard, btop-style: one titled box per row (cpu, mem, net, argon), each split
    // in two columns (meters | details and graph). Rows open the page where the value changes.
    fn draw_kde_compact_overview_tab(&mut self, ui: &mut Ui) {
        let mut go: Option<ActiveTab> = None;
        self.btop_cpu_box(ui, &mut go);
        ui.add_space(10.0);
        if self.gpu_available() {
            self.btop_gpu_box(ui);
            ui.add_space(10.0);
        }
        self.btop_mem_box(ui);
        ui.add_space(10.0);
        self.btop_net_box(ui);
        ui.add_space(10.0);
        self.btop_argon_box(ui, &mut go);
        if let Some(tab) = go {
            self.active_tab = tab;
        }
    }

    fn btop_cpu_box(&self, ui: &mut Ui, go: &mut Option<ActiveTab>) {
        let t = texts(self.lang);
        let pal = self.active_palette();
        let tel = &self.telemetry;
        // Both columns can request a page change.
        let target = std::cell::Cell::new(None);
        btop_box(ui, "cpu", pal.focus_blue, |ui| {
            two_columns(
                ui,
                |ui| {
                    let total = tel.cpu_total_usage_pct / 100.0;
                    if gradient_meter(
                        ui,
                        t.g_cpu,
                        total,
                        &format!("{:5.1}%", tel.cpu_total_usage_pct),
                    )
                    .clicked()
                    {
                        target.set(Some(ActiveTab::Overclocking));
                    }
                    ui.add_space(2.0);
                    for (name, pct) in &tel.cpu_core_usage_pct {
                        if gradient_meter(ui, name, pct / 100.0, &format!("{pct:5.1}%")).clicked() {
                            target.set(Some(ActiveTab::Overclocking));
                        }
                    }
                },
                |ui| {
                    let oc = if self.boot_oc.exceeds_stock(&crate::board::current().oc) {
                        "  OC"
                    } else {
                        ""
                    };
                    if info_row(
                        ui,
                        t.mt_clock,
                        &format!("{} MHz  {}{oc}", tel.cpu_cur_freq_mhz, tel.cpu_governor),
                        uc().text_primary,
                        true,
                    )
                    .clicked()
                    {
                        target.set(Some(ActiveTab::Overclocking));
                    }
                    info_row(
                        ui,
                        t.mt_load,
                        &format!(
                            "{:.2} {:.2} {:.2}",
                            tel.load_avg.0, tel.load_avg.1, tel.load_avg.2
                        ),
                        uc().text_primary,
                        false,
                    );
                    info_row(
                        ui,
                        t.mt_uptime,
                        &tel.format_uptime(),
                        uc().text_primary,
                        false,
                    );
                },
            );
        });
        if let Some(tab) = target.get() {
            *go = Some(tab);
        }
    }

    fn btop_mem_box(&self, ui: &mut Ui) {
        let t = texts(self.lang);
        let pal = self.active_palette();
        let tel = &self.telemetry;
        btop_box(ui, "mem", pal.visited_purple, |ui| {
            two_columns(
                ui,
                |ui| {
                    let gb = |mb: u64| mb as f32 / 1024.0;
                    gradient_meter(
                        ui,
                        t.mt_mem,
                        tel.ram_used_mb as f32 / tel.ram_total_mb.max(1) as f32,
                        &format!("{:.1}/{:.1}G", gb(tel.ram_used_mb), gb(tel.ram_total_mb)),
                    );
                    gradient_meter(
                        ui,
                        t.mt_swap,
                        tel.swap_used_mb as f32 / tel.swap_total_mb.max(1) as f32,
                        &format!("{}/{}M", tel.swap_used_mb, tel.swap_total_mb),
                    );
                },
                |ui| {
                    ui.label(
                        RichText::new(t.card_storage)
                            .monospace()
                            .size(11.0)
                            .color(uc().text_secondary),
                    );
                    if tel.disks.is_empty() {
                        ui.label(RichText::new(t.no_disks).monospace().size(11.0));
                    }
                    for d in &tel.disks {
                        let label: String = if d.mount_point == "/" {
                            "/".into()
                        } else {
                            d.mount_point
                                .rsplit('/')
                                .next()
                                .unwrap_or(&d.mount_point)
                                .chars()
                                .take(7)
                                .collect()
                        };
                        gradient_meter(
                            ui,
                            &label,
                            d.usage_pct / 100.0,
                            &format!("{:.0}/{:.0}G", d.used_gb, d.total_gb),
                        )
                        .on_hover_text(format!("{} ({})", d.mount_point, d.device));
                    }
                },
            );
        });
    }

    /// True when at least one GPU source (V3D load, CMA, firmware clocks) is readable.
    fn gpu_available(&self) -> bool {
        let tel = &self.telemetry;
        tel.gpu_load.is_some() || tel.cma_mb.is_some() || tel.firmware.v3d_mhz.is_some()
    }

    /// GPU clock to show: live V3D clock from the firmware, else the configured `gpu_freq`.
    fn gpu_clock_text(&self) -> String {
        match self.telemetry.firmware.v3d_mhz {
            Some(mhz) => format!("{mhz} MHz"),
            None => format!("{} MHz (config)", self.boot_oc.gpu_freq_mhz),
        }
    }

    fn btop_gpu_box(&self, ui: &mut Ui) {
        let t = texts(self.lang);
        let pal = self.active_palette();
        let tel = &self.telemetry;
        btop_box(ui, "gpu", pal.positive_green, |ui| {
            two_columns(
                ui,
                |ui| {
                    if let Some(l) = tel.gpu_load {
                        gradient_meter(
                            ui,
                            t.gpu_render,
                            l.render / 100.0,
                            &format!("{:5.1}%", l.render),
                        );
                        gradient_meter(ui, t.gpu_bin, l.bin / 100.0, &format!("{:5.1}%", l.bin));
                        gradient_meter(
                            ui,
                            t.gpu_compute,
                            l.csd / 100.0,
                            &format!("{:5.1}%", l.csd),
                        );
                    }
                    if let Some((used, total)) = tel.cma_mb {
                        gradient_meter(
                            ui,
                            "CMA",
                            used as f32 / total.max(1) as f32,
                            &format!("{used}/{total}M"),
                        )
                        .on_hover_text(t.gpu_cma_tip);
                    }
                },
                |ui| {
                    let fw = &tel.firmware;
                    let txt = uc().text_primary;
                    // Clocks side by side; without firmware access, the configured clock.
                    if fw.v3d_mhz.is_some() || fw.core_mhz.is_some() {
                        let mut clocks = Vec::new();
                        if let Some(m) = fw.v3d_mhz {
                            clocks.push(("v3d", format!("{m} MHz"), txt));
                        }
                        if let Some(m) = fw.core_mhz {
                            clocks.push(("core", format!("{m} MHz"), txt));
                        }
                        if let Some(m) = fw.h264_mhz {
                            clocks.push(("h264", format!("{m} MHz"), txt));
                        }
                        info_pairs_row(ui, &clocks);
                    } else {
                        info_row(ui, t.mt_clock, &self.gpu_clock_text(), txt, false);
                    }
                    // Electrical / memory / activity.
                    let mut misc = Vec::new();
                    if let Some(v) = fw.core_volts {
                        misc.push(("Vcore", format!("{v:.3} V"), txt));
                    }
                    if let Some(mb) = fw.gpu_mem_mb {
                        misc.push(("split", format!("{mb} MB"), txt));
                    }
                    if let Some(l) = tel.gpu_load {
                        misc.push(("jobs/s", format!("{:.0}", l.render_jobs_per_s), txt));
                    }
                    info_pairs_row(ui, &misc);
                    // Throttling status on its own line (can be long).
                    if let Some(th) = fw.throttled {
                        let (text, bad) = throttle_summary(th, self.lang);
                        info_row(
                            ui,
                            t.mt_throttle,
                            &text,
                            if bad {
                                pal.negative_red
                            } else {
                                pal.positive_green
                            },
                            false,
                        );
                    }
                },
            );
        });
    }

    fn btop_net_box(&self, ui: &mut Ui) {
        let pal = self.active_palette();
        let tel = &self.telemetry;
        let peak = tel
            .net_rx_history
            .iter()
            .chain(tel.net_tx_history.iter())
            .copied()
            .fold(100.0_f32, f32::max);
        btop_box(ui, "net", pal.positive_green, |ui| {
            two_columns(
                ui,
                |ui| {
                    gradient_meter(
                        ui,
                        "⬇ rx",
                        tel.net_rx_kbps / peak,
                        &format!("{:.1} KB/s", tel.net_rx_kbps),
                    );
                    gradient_meter(
                        ui,
                        "⬆ tx",
                        tel.net_tx_kbps / peak,
                        &format!("{:.1} KB/s", tel.net_tx_kbps),
                    );
                },
                |ui| {
                    info_row(ui, "IP", &tel.ip_address, uc().text_primary, false);
                    info_row(
                        ui,
                        "peak",
                        &format!("{peak:.0} KB/s"),
                        uc().text_secondary,
                        false,
                    );
                },
            );
        });
    }

    fn btop_argon_box(&self, ui: &mut Ui, go: &mut Option<ActiveTab>) {
        let t = texts(self.lang);
        let pal = self.active_palette();
        let tel = &self.telemetry;
        let target = std::cell::Cell::new(None);
        btop_box(ui, "argon", pal.neutral_orange, |ui| {
            two_columns(
                ui,
                |ui| {
                    let mode = if tel.manual_fan_override.is_some() {
                        t.fan_mode_manual
                    } else {
                        t.fan_mode_auto
                    };
                    if gradient_meter(
                        ui,
                        t.mt_fan,
                        tel.target_fan_speed_pct as f32 / 100.0,
                        &format!("{mode} {}%", tel.target_fan_speed_pct),
                    )
                    .clicked()
                    {
                        target.set(Some(ActiveTab::SmartFan));
                    }
                    // Temperature on a 25-85 °C scale (85 °C is the firmware limit).
                    let temp_frac = ((tel.cpu_temp_c - 25.0) / 60.0).clamp(0.0, 1.0);
                    if gradient_meter(
                        ui,
                        t.mt_temp,
                        temp_frac,
                        &self.temp_unit.format_temp(tel.cpu_temp_c),
                    )
                    .clicked()
                    {
                        target.set(Some(ActiveTab::SmartFan));
                    }
                },
                |ui| {
                    info_row(
                        ui,
                        "max",
                        &self.temp_unit.format_temp(tel.max_temp_c),
                        uc().text_secondary,
                        false,
                    );
                    info_row(
                        ui,
                        t.mt_power,
                        if tel.undervoltage_alarm {
                            t.voltage_alert
                        } else {
                            t.voltage_stable
                        },
                        if tel.undervoltage_alarm {
                            pal.negative_red
                        } else {
                            pal.positive_green
                        },
                        false,
                    );
                    if info_row(
                        ui,
                        t.mt_daemon,
                        if tel.argon_service_active {
                            t.daemon_online
                        } else {
                            t.daemon_offline
                        },
                        if tel.argon_service_active {
                            pal.positive_green
                        } else {
                            pal.negative_red
                        },
                        true,
                    )
                    .clicked()
                    {
                        target.set(Some(ActiveTab::ServicesAndConfig));
                    }
                },
            );
        });
        if let Some(tab) = target.get() {
            *go = Some(tab);
        }
    }

    // =========================================================================
    // TAB 1B: KDE SYSTEM MONITOR FULL DASHBOARD (overview.page + history.page)
    // =========================================================================
    fn draw_kde_dashboard_tab(&mut self, ui: &mut Ui) {
        let t = texts(self.lang);
        let pal = self.active_palette();

        // 1. Sensor rings; each ring opens its page.
        let mut clicked_ring: Option<&str> = None;
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
                        if r0.clicked() {
                            clicked_ring = Some(t.gauge_cpu_clock);
                        }
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
                        if r1.clicked() {
                            clicked_ring = Some(t.gauge_cpu_temp);
                        }
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
                        if r2.clicked() {
                            clicked_ring = Some(t.gauge_fan_speed);
                        }
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
                        if r3.clicked() {
                            clicked_ring = Some(t.gauge_memory);
                        }
                        aorus_tooltip(
                            r3,
                            t.gauge_memory,
                            t.gauge_memory_tip,
                            Some("/proc/meminfo"),
                        );
                    });
                } else {
                    ui.columns(2, |cols| {
                        if draw_kde_donut_chart(
                            &mut cols[0],
                            t.gauge_cpu_clock,
                            &cpu_val,
                            &cpu_sub,
                            cpu_frac,
                            pal.focus_blue,
                            &pal,
                        )
                        .clicked()
                        {
                            clicked_ring = Some(t.gauge_cpu_clock);
                        }
                        if draw_kde_donut_chart(
                            &mut cols[1],
                            t.gauge_cpu_temp,
                            &temp_val,
                            &temp_sub,
                            temp_frac,
                            temp_col,
                            &pal,
                        )
                        .clicked()
                        {
                            clicked_ring = Some(t.gauge_cpu_temp);
                        }
                    });
                    ui.separator();
                    ui.columns(2, |cols| {
                        if draw_kde_donut_chart(
                            &mut cols[0],
                            t.gauge_fan_speed,
                            &fan_val,
                            fan_sub,
                            fan_frac,
                            Color32::from_rgb(26, 188, 156),
                            &pal,
                        )
                        .clicked()
                        {
                            clicked_ring = Some(t.gauge_fan_speed);
                        }
                        if draw_kde_donut_chart(
                            &mut cols[1],
                            t.gauge_memory,
                            &ram_val,
                            &ram_sub,
                            ram_frac,
                            pal.visited_purple,
                            &pal,
                        )
                        .clicked()
                        {
                            clicked_ring = Some(t.gauge_memory);
                        }
                    });
                }
            },
        );
        if let Some(tab) = clicked_ring.and_then(|title| ring_target(self.lang, title)) {
            self.active_tab = tab;
        }

        ui.add_space(12.0);
        self.draw_status_strip(ui);
        ui.add_space(12.0);

        // 2. Temperature / fan / CPU history (full row)
        aorus_card_with_tip(
            ui,
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
                        (&self.telemetry.cpu_history, pal.focus_blue, 100.0),
                        (&self.telemetry.fan_history, pal.positive_green, 100.0),
                        (&self.telemetry.temp_history, pal.neutral_orange, 100.0),
                    ],
                    &pal,
                );
            },
        );

        ui.add_space(12.0);

        // 3. Per-core load (full row)
        self.draw_core_load_card(ui);

        ui.add_space(12.0);

        // 4. Memory history (full row)
        aorus_card(ui, t.kde_mem_history_title, pal.visited_purple, |ui| {
            ui.horizontal_wrapped(|ui| {
                legend_badge(
                    ui,
                    &format!(
                        "RAM {:.1}%  ({} / {} MB)",
                        self.telemetry.ram_usage_pct,
                        self.telemetry.ram_used_mb,
                        self.telemetry.ram_total_mb
                    ),
                    pal.visited_purple,
                );
            });
            ui.add_space(6.0);
            draw_kde_area_line_chart(
                ui,
                130.0,
                &[(&self.telemetry.ram_history, pal.visited_purple, 100.0)],
                &pal,
            );
        });

        ui.add_space(12.0);

        // 5. Network throughput history (full row), scaled to its own peak
        aorus_card(ui, t.kde_net_history_title, pal.focus_blue, |ui| {
            let max_net = self
                .telemetry
                .net_rx_history
                .iter()
                .chain(self.telemetry.net_tx_history.iter())
                .copied()
                .fold(100.0_f32, f32::max);
            ui.horizontal_wrapped(|ui| {
                legend_badge(
                    ui,
                    &format!("⬇ Download {:.1} KB/s", self.telemetry.net_rx_kbps),
                    pal.focus_blue,
                );
                ui.add_space(8.0);
                legend_badge(
                    ui,
                    &format!("⬆ Upload {:.1} KB/s", self.telemetry.net_tx_kbps),
                    pal.positive_green,
                );
                ui.add_space(8.0);
                ui.label(
                    RichText::new(format!("max {max_net:.0} KB/s"))
                        .size(11.0)
                        .color(uc().text_secondary),
                );
            });
            ui.add_space(6.0);
            draw_kde_area_line_chart(
                ui,
                130.0,
                &[
                    (&self.telemetry.net_rx_history, pal.focus_blue, max_net),
                    (&self.telemetry.net_tx_history, pal.positive_green, max_net),
                ],
                &pal,
            );
        });

        ui.add_space(12.0);

        // 6. GPU (full row), when a source is available.
        if self.gpu_available() {
            let tel = &self.telemetry;
            aorus_card_with_tip(ui, t.g_gpu, t.gpu_tip, pal.positive_green, |ui| {
                ui.horizontal_wrapped(|ui| {
                    if let Some(l) = tel.gpu_load {
                        legend_badge(
                            ui,
                            &format!("{} {:.1}%", t.gpu_render, l.render),
                            pal.positive_green,
                        );
                        ui.add_space(8.0);
                        legend_badge(ui, &format!("{} {:.1}%", t.gpu_bin, l.bin), pal.focus_blue);
                        ui.add_space(8.0);
                    }
                    if let Some((used, total)) = tel.cma_mb {
                        legend_badge(ui, &format!("CMA {used}/{total} MB"), pal.visited_purple);
                        ui.add_space(8.0);
                    }
                    ui.label(
                        RichText::new(self.gpu_clock_text())
                            .size(11.5)
                            .color(uc().text_secondary),
                    );
                });
                ui.add_space(6.0);
                draw_kde_area_line_chart(
                    ui,
                    130.0,
                    &[(&tel.gpu_history, pal.positive_green, 100.0)],
                    &pal,
                );
            });
            ui.add_space(12.0);
        }

        // 7. Storage (full row). System details live in the footer.
        self.draw_storage_card(ui);
    }

    // =========================================================================
    // TAB 1B: DASHBOARD (RADIAL GAUGES)
    // =========================================================================
    // Gauges dashboard: radial gauges only (no charts). CPU with one gauge per core,
    // sensors, storage and network.
    fn draw_dashboard_tab(&mut self, ui: &mut Ui) {
        let t = texts(self.lang);
        let pal = self.active_palette();
        let tel = &self.telemetry;
        let mut go: Option<ActiveTab> = None;

        // CPU: total load plus one gauge per core.
        aorus_card_with_tip(ui, t.g_cpu, t.card_core_load_tip, uc().orange, |ui| {
            let total = tel.cpu_total_usage_pct / 100.0;
            let big = GaugeSpec {
                title: t.total_cpu_load.to_string(),
                ratio: total,
                value: format!("{:.0}", tel.cpu_total_usage_pct),
                unit: "%".into(),
                sub: format!("{} MHz · {}", tel.cpu_cur_freq_mhz, tel.cpu_governor),
                color: level_color(total),
                tip: t.card_core_load_tip,
                tab: Some(ActiveTab::Overclocking),
            };
            let cores: Vec<GaugeSpec> = tel
                .cpu_core_usage_pct
                .iter()
                .map(|(name, pct)| GaugeSpec {
                    title: name.clone(),
                    ratio: pct / 100.0,
                    value: format!("{pct:.0}"),
                    unit: "%".into(),
                    sub: String::new(),
                    color: level_color(pct / 100.0),
                    tip: t.card_core_load_tip,
                    tab: Some(ActiveTab::Overclocking),
                })
                .collect();
            if ui.available_width() >= 620.0 {
                ui.columns(2, |cols| {
                    gauge_cell(&mut cols[0], &big, 170.0, &mut go);
                    gauge_grid(&mut cols[1], &cores, 105.0, 95.0, &mut go);
                });
            } else {
                gauge_cell(ui, &big, 160.0, &mut go);
                gauge_grid(ui, &cores, 105.0, 95.0, &mut go);
            }
        });

        ui.add_space(12.0);

        // Sensors.
        let temp_frac = ((tel.cpu_temp_c - 25.0) / 60.0).clamp(0.0, 1.0);
        let max_clock = tel.cpu_hw_max_mhz.max(1) as f32;
        let ram_frac = tel.ram_used_mb as f32 / tel.ram_total_mb.max(1) as f32;
        let swap_frac = tel.swap_used_mb as f32 / tel.swap_total_mb.max(1) as f32;
        let sensors = vec![
            GaugeSpec {
                title: t.gauge_cpu_clock.into(),
                ratio: tel.cpu_cur_freq_mhz as f32 / max_clock,
                value: tel.cpu_cur_freq_mhz.to_string(),
                unit: "MHz".into(),
                sub: format!("max {}", tel.cpu_hw_max_mhz),
                color: pal.focus_blue,
                tip: t.gauge_cpu_clock_tip,
                tab: Some(ActiveTab::Overclocking),
            },
            GaugeSpec {
                title: t.gauge_cpu_temp.into(),
                ratio: temp_frac,
                value: match self.temp_unit {
                    TempUnit::Celsius => format!("{:.1}", tel.cpu_temp_c),
                    TempUnit::Fahrenheit => format!("{:.1}", tel.cpu_temp_c * 9.0 / 5.0 + 32.0),
                },
                unit: self.temp_unit.symbol().into(),
                sub: format!("max {}", self.temp_unit.format_temp(tel.max_temp_c)),
                color: level_color(temp_frac),
                tip: t.gauge_cpu_temp_tip,
                tab: Some(ActiveTab::SmartFan),
            },
            GaugeSpec {
                title: t.gauge_fan_speed.into(),
                ratio: tel.target_fan_speed_pct as f32 / 100.0,
                value: tel.target_fan_speed_pct.to_string(),
                unit: "%".into(),
                sub: if tel.manual_fan_override.is_some() {
                    t.fan_mode_manual
                } else {
                    t.fan_mode_auto
                }
                .into(),
                color: pal.positive_green,
                tip: t.gauge_fan_speed_tip,
                tab: Some(ActiveTab::SmartFan),
            },
            GaugeSpec {
                title: t.gauge_memory.into(),
                ratio: ram_frac,
                value: format!("{:.0}", ram_frac * 100.0),
                unit: "%".into(),
                sub: format!(
                    "{:.1}/{:.1} GB",
                    tel.ram_used_mb as f32 / 1024.0,
                    tel.ram_total_mb as f32 / 1024.0
                ),
                color: pal.visited_purple,
                tip: t.gauge_memory_tip,
                tab: None,
            },
            GaugeSpec {
                title: t.g_swap.into(),
                ratio: swap_frac,
                value: format!("{:.0}", swap_frac * 100.0),
                unit: "%".into(),
                sub: format!("{}/{} MB", tel.swap_used_mb, tel.swap_total_mb),
                color: level_color(swap_frac),
                tip: t.gauge_memory_tip,
                tab: None,
            },
        ];
        aorus_card_with_tip(
            ui,
            t.kde_overview_sensors_title,
            t.tab_dashboard_tip,
            pal.focus_blue,
            |ui| {
                gauge_grid(ui, &sensors, 150.0, 150.0, &mut go);
            },
        );

        ui.add_space(12.0);

        // GPU: load, CMA memory and clock, when available.
        if self.gpu_available() {
            let mut gpu: Vec<GaugeSpec> = Vec::new();
            if let Some(l) = tel.gpu_load {
                gpu.push(GaugeSpec {
                    title: t.gpu_render.into(),
                    ratio: l.render / 100.0,
                    value: format!("{:.0}", l.render),
                    unit: "%".into(),
                    sub: format!("{:.0} jobs/s", l.render_jobs_per_s),
                    color: level_color(l.render / 100.0),
                    tip: t.gpu_tip,
                    tab: None,
                });
            }
            if let Some((used, total)) = tel.cma_mb {
                let frac = used as f32 / total.max(1) as f32;
                gpu.push(GaugeSpec {
                    title: "CMA".into(),
                    ratio: frac,
                    value: format!("{:.0}", frac * 100.0),
                    unit: "%".into(),
                    sub: format!("{used}/{total} MB"),
                    color: pal.visited_purple,
                    tip: t.gpu_cma_tip,
                    tab: None,
                });
            }
            let (mhz, live) = match tel.firmware.v3d_mhz {
                Some(m) => (m, true),
                None => (self.boot_oc.gpu_freq_mhz, false),
            };
            let max = crate::board::current().oc.gpu_range.1.max(1);
            gpu.push(GaugeSpec {
                title: t.gpu_clock.into(),
                ratio: mhz as f32 / max as f32,
                value: mhz.to_string(),
                unit: "MHz".into(),
                sub: if live { String::new() } else { "config".into() },
                color: pal.focus_blue,
                tip: t.gpu_tip,
                tab: Some(ActiveTab::Overclocking),
            });
            aorus_card_with_tip(ui, t.g_gpu, t.gpu_tip, pal.positive_green, |ui| {
                gauge_grid(ui, &gpu, 140.0, 150.0, &mut go);
            });
            ui.add_space(12.0);
        }

        // Storage: one gauge per mounted disk.
        let disks: Vec<GaugeSpec> = tel
            .disks
            .iter()
            .map(|d| GaugeSpec {
                title: d.mount_point.clone(),
                ratio: d.usage_pct / 100.0,
                value: format!("{:.0}", d.usage_pct),
                unit: "%".into(),
                sub: format!("{:.0}/{:.0} GB", d.used_gb, d.total_gb),
                color: level_color(d.usage_pct / 100.0),
                tip: t.card_storage,
                tab: None,
            })
            .collect();
        if !disks.is_empty() {
            aorus_card(ui, t.card_storage, uc().cyan, |ui| {
                gauge_grid(ui, &disks, 140.0, 150.0, &mut go);
            });
            ui.add_space(12.0);
        }

        // Network: download / upload scaled to the observed peak.
        let peak = tel
            .net_rx_history
            .iter()
            .chain(tel.net_tx_history.iter())
            .copied()
            .fold(100.0_f32, f32::max);
        let net = vec![
            GaugeSpec {
                title: t.g_download.into(),
                ratio: tel.net_rx_kbps / peak,
                value: format!("{:.1}", tel.net_rx_kbps),
                unit: "KB/s".into(),
                sub: format!("peak {peak:.0}"),
                color: pal.focus_blue,
                tip: t.kde_net_history_title,
                tab: None,
            },
            GaugeSpec {
                title: t.g_upload.into(),
                ratio: tel.net_tx_kbps / peak,
                value: format!("{:.1}", tel.net_tx_kbps),
                unit: "KB/s".into(),
                sub: format!("peak {peak:.0}"),
                color: pal.positive_green,
                tip: t.kde_net_history_title,
                tab: None,
            },
        ];
        aorus_card(ui, t.kde_net_history_title, pal.focus_blue, |ui| {
            gauge_grid(ui, &net, 140.0, 150.0, &mut go);
        });

        ui.add_space(12.0);
        self.draw_status_strip(ui);

        if let Some(tab) = go {
            self.active_tab = tab;
        }
    }

    fn draw_core_load_card(&mut self, ui: &mut Ui) {
        let t = texts(self.lang);
        let pal = self.active_palette();
        aorus_card_with_tip(
            ui,
            t.card_core_load,
            t.card_core_load_tip,
            uc().orange,
            |ui| {
                // Legend (one colour per core) + total load.
                let colors = [
                    pal.focus_blue,
                    pal.positive_green,
                    pal.neutral_orange,
                    pal.visited_purple,
                    pal.negative_red,
                    uc().cyan,
                    uc().amber,
                    uc().text_secondary,
                ];
                ui.horizontal_wrapped(|ui| {
                    ui.label(
                        RichText::new(format!(
                            "{}: {:.1}%",
                            t.total_cpu_load, self.telemetry.cpu_total_usage_pct
                        ))
                        .strong()
                        .color(uc().text_primary),
                    );
                    ui.add_space(8.0);
                    for (i, (core_name, _)) in self.telemetry.cpu_core_usage_pct.iter().enumerate()
                    {
                        legend_badge(ui, core_name, colors[i % colors.len()]);
                        ui.add_space(4.0);
                    }
                });
                ui.add_space(6.0);

                // History: one line per core over the last 60 s.
                let series: Vec<(&VecDeque<f32>, Color32, f32)> = self
                    .telemetry
                    .core_history
                    .iter()
                    .enumerate()
                    .map(|(i, h)| (h, colors[i % colors.len()], 100.0))
                    .collect();
                if !series.is_empty() {
                    draw_kde_area_line_chart(ui, 150.0, &series, &pal);
                }
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
            },
        );
    }

    /// Read-only status row shared by every dashboard: fan mode, clock profile and the
    /// argononed state. Each item opens the page where it can be changed safely.
    fn draw_status_strip(&mut self, ui: &mut Ui) {
        let t = texts(self.lang);
        let pal = self.active_palette();
        let profile = crate::board::current().oc;
        let fan = match self.telemetry.manual_fan_override {
            Some(p) => format!("{}  {} · {p}%", t.tab_smart_fan, t.fan_mode_manual),
            None => format!(
                "{}  {} · {}%",
                t.tab_smart_fan, t.fan_mode_auto, self.telemetry.target_fan_speed_pct
            ),
        };
        let clock = format!(
            "{}  {} · {} MHz{}",
            t.tab_overclock,
            self.telemetry.cpu_governor,
            self.telemetry.cpu_max_freq_mhz,
            if self.boot_oc.exceeds_stock(&profile) {
                " · OC"
            } else {
                ""
            }
        );
        let (d_txt, d_col) = if self.telemetry.argon_service_active {
            (t.daemon_online, pal.positive_green)
        } else {
            (t.daemon_offline, pal.negative_red)
        };
        let mut go: Option<ActiveTab> = None;
        egui::Frame::new()
            .fill(uc().bg_subgroup)
            .stroke(Stroke::new(1.0_f32, uc().border_subtle))
            .corner_radius(CornerRadius::same(5))
            .inner_margin(egui::Margin::symmetric(10, 6))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 18.0;
                    for (text, color, tab, tip) in [
                        (
                            fan,
                            uc().text_primary,
                            ActiveTab::SmartFan,
                            t.tab_smart_fan_tip,
                        ),
                        (
                            clock,
                            uc().text_primary,
                            ActiveTab::Overclocking,
                            t.tab_overclock_tip,
                        ),
                        (
                            d_txt.to_string(),
                            d_col,
                            ActiveTab::ServicesAndConfig,
                            t.daemon_badge_tip,
                        ),
                    ] {
                        let resp = ui.add(
                            egui::Button::new(RichText::new(text).size(12.5).strong().color(color))
                                .frame(false),
                        );
                        if resp.clicked() {
                            go = Some(tab);
                        }
                        resp.on_hover_text(tip);
                    }
                });
            });
        if let Some(tab) = go {
            self.active_tab = tab;
        }
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
                ui.label(RichText::new(t.raid_arrays_title).strong())
                    .on_hover_text("/proc/mdstat");
                for r in &self.telemetry.raid_arrays {
                    ui.label(format!(
                        "• {} [{}] — {} ({})",
                        r.name, r.level, r.state, r.disks
                    ));
                }
            }
        });
    }

    /// System summary for the footer: one compact line plus the full details for its tooltip.
    fn system_summary(&self) -> (String, String) {
        let t = texts(self.lang);
        let board = crate::board::current();
        let tel = &self.telemetry;
        let short = format!(
            "{}  ·  {}  ·  Linux {}  ·  CPU {}–{} MHz  ·  Boot {} MHz",
            board.summary(),
            tel.os_name,
            tel.kernel_version,
            tel.cpu_min_freq_mhz,
            tel.cpu_max_freq_mhz,
            self.boot_oc.arm_freq_mhz
        );
        let ram = board
            .ram_mb
            .map(|m| format!("{} GB", m / 1024))
            .unwrap_or_else(|| "—".into());
        let details = [
            (t.spec_board, tel.board_model.clone()),
            (t.spec_soc, format!("{} · {ram}", board.soc.name())),
            (t.spec_case, board.case.name().to_string()),
            (t.spec_os, tel.os_name.clone()),
            (t.spec_kernel, tel.kernel_version.clone()),
            (
                t.spec_cpu_scale,
                format!(
                    "{} – {} MHz (HW max {} MHz)",
                    tel.cpu_min_freq_mhz, tel.cpu_max_freq_mhz, tel.cpu_hw_max_mhz
                ),
            ),
            (
                t.spec_swap,
                format!("{} / {} MB", tel.swap_used_mb, tel.swap_total_mb),
            ),
            (
                t.spec_boot_oc,
                format!(
                    "ARM {} MHz · GPU {} MHz · OV {}",
                    self.boot_oc.arm_freq_mhz, self.boot_oc.gpu_freq_mhz, self.boot_oc.over_voltage
                ),
            ),
        ]
        .iter()
        .map(|(k, v)| format!("{k} {v}"))
        .collect::<Vec<_>>()
        .join("\n");
        (short, details)
    }

    // =========================================================================
    // TAB 2: SMART FAN 5 (CLEAN GROUPED UX + RICH TOOLTIPS)
    // =========================================================================
    // =========================================================================
    // SETTINGS: appearance, language, units, about
    // =========================================================================
    fn draw_settings_tab(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        let t = texts(self.lang);
        let mut restyle = false;

        aorus_card_with_tip(
            ui,
            t.set_appearance,
            t.set_appearance_tip,
            uc().cyan,
            |ui| {
                settings_label(ui, t.set_layout);
                choice_row(
                    ui,
                    &mut self.layout,
                    &[
                        (
                            Layout::Sidebar,
                            t.set_layout_sidebar,
                            t.set_layout_sidebar_desc,
                        ),
                        (Layout::TopTabs, t.set_layout_tabs, t.set_layout_tabs_desc),
                    ],
                );
                ui.add_space(8.0);
                settings_label(ui, t.set_colors);
                if choice_row(
                    ui,
                    &mut self.colors,
                    &[
                        (
                            ColorScheme::System,
                            t.set_colors_system,
                            t.set_colors_system_desc,
                        ),
                        (
                            ColorScheme::Aorus,
                            t.set_colors_aorus,
                            t.set_colors_aorus_desc,
                        ),
                    ],
                ) {
                    restyle = true;
                }
                ui.add_space(8.0);
                settings_label(ui, t.set_dashboard);
                choice_row(
                    ui,
                    &mut self.dashboard,
                    &[
                        (
                            DashboardStyle::Compact,
                            t.set_dash_compact,
                            t.set_dash_compact_desc,
                        ),
                        (
                            DashboardStyle::Charts,
                            t.set_dash_charts,
                            t.set_dash_charts_desc,
                        ),
                        (
                            DashboardStyle::Gauges,
                            t.set_dash_gauges,
                            t.set_dash_gauges_desc,
                        ),
                    ],
                );
                ui.add_space(8.0);
                settings_label(ui, t.set_scale);
                ui.horizontal_wrapped(|ui| {
                    for pct in [80_u32, 90, 100, 115, 130, 150] {
                        let v = pct as f32 / 100.0;
                        let selected = (self.ui_scale - v).abs() < 0.01;
                        if segmented_pill(
                            ui,
                            &format!("{pct}%"),
                            selected,
                            uc().btn_cyan,
                            uc().cyan,
                        )
                        .on_hover_text(t.set_scale_tip)
                        .clicked()
                        {
                            self.ui_scale = crate::settings::clamp_scale(v);
                            ctx.set_zoom_factor(self.ui_scale);
                        }
                    }
                });
            },
        );

        ui.add_space(12.0);
        aorus_card_with_tip(ui, t.set_language, t.set_language_tip, uc().green, |ui| {
            choice_row(
                ui,
                &mut self.lang,
                &[
                    (Language::EnUs, "English", "English (United States)"),
                    (Language::PtBr, "Português", "Português (Brasil)"),
                ],
            );
        });

        ui.add_space(12.0);
        let mut apply_argon_unit = false;
        aorus_card_with_tip(ui, t.set_units, t.set_units_tip, uc().orange, |ui| {
            settings_label(ui, t.set_temp_display);
            choice_row(
                ui,
                &mut self.temp_unit,
                &[
                    (TempUnit::Celsius, "°C", "Celsius"),
                    (TempUnit::Fahrenheit, "°F", "Fahrenheit"),
                ],
            );
            ui.add_space(8.0);
            settings_label(ui, t.set_argon_unit);
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    RichText::new(format!(
                        "{} {}",
                        t.set_argon_unit_current,
                        self.argon_unit.symbol()
                    ))
                    .size(12.5)
                    .color(uc().text_secondary),
                );
                let differs = self.argon_unit != self.temp_unit;
                let btn = ui.add_enabled(
                    differs,
                    egui::Button::new(
                        RichText::new(format!(
                            "{} ({})",
                            t.set_apply_argon_unit,
                            self.temp_unit.symbol()
                        ))
                        .strong(),
                    ),
                );
                if btn.on_hover_text(t.set_apply_argon_unit_tip).clicked() {
                    apply_argon_unit = true;
                }
            });
        });
        if apply_argon_unit {
            let res = save_temp_unit(self.temp_unit);
            if res.is_ok() {
                self.argon_unit = self.temp_unit;
            }
            self.log_result("Argon temperature unit", res);
        }

        ui.add_space(12.0);
        aorus_card_with_tip(ui, t.set_hw_access, t.set_hw_access_tip, uc().green, |ui| {
            for c in crate::doctor::run() {
                let resp = info_row(
                    ui,
                    if c.ok { "ok" } else { "--" },
                    c.name,
                    if c.ok {
                        uc().green
                    } else {
                        uc().text_secondary
                    },
                    false,
                );
                resp.on_hover_text(if c.ok {
                    c.enables.to_string()
                } else {
                    format!("{}\n\n{}", c.enables, c.fix)
                });
            }
        });

        ui.add_space(12.0);
        aorus_card_with_tip(ui, t.set_about, t.set_about_tip, uc().amber, |ui| {
            let board = crate::board::current();
            let path = crate::settings::path()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "—".into());
            for (key, value) in [
                (t.set_version, env!("CARGO_PKG_VERSION").to_string()),
                (t.set_hardware, board.summary()),
                (t.set_color_source, self.kde_palette.describe()),
                (t.set_settings_file, path),
                (t.set_credits, crate::pinout::CREDIT.to_string()),
            ] {
                // Key and value stacked so long values (paths) wrap instead of widening the page.
                ui.label(RichText::new(key).size(11.5).color(uc().text_secondary));
                ui.add(
                    egui::Label::new(
                        RichText::new(value)
                            .size(12.5)
                            .strong()
                            .color(uc().text_primary),
                    )
                    .wrap(),
                );
                ui.add_space(4.0);
            }
            ui.add_space(8.0);
            if ui
                .button(RichText::new(t.set_reset).strong())
                .on_hover_text(t.set_reset_tip)
                .clicked()
            {
                let d = crate::settings::Settings::default();
                self.layout = d.layout;
                self.colors = d.colors;
                self.dashboard = d.dashboard;
                self.lang = d.language;
                self.temp_unit = self.argon_unit;
                self.ui_scale = d.ui_scale;
                ctx.set_zoom_factor(self.ui_scale);
                restyle = true;
            }
        });

        if restyle {
            if self.colors == ColorScheme::System {
                self.kde_palette = KdeBreezePalette::load_from_system();
            }
            self.apply_style(ctx);
        }
    }

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
                            "{:.1} °C  ➡  {}: {}% PWM",
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
                            // Slider shrinks with the window so the 4-column grid never clips.
                            ui.spacing_mut().slider_width =
                                (ui.available_width() - 290.0).clamp(60.0, 220.0);
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
                                            .color(uc().heading),
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
    // TAB 3: OVERCLOCKING (PRESETS, LIVE CPUFREQ, FIRMWARE, STABILITY)
    // =========================================================================
    fn draw_overclocking_tab(&mut self, ui: &mut Ui) {
        let t = texts(self.lang);
        let lang = self.lang;

        self.draw_power_guard_card(ui);
        self.draw_trial_card(ui);

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

        ui.add_space(12.0);
        self.draw_stability_card(ui);
    }

    /// Save / trial-boot controls for the firmware overclock card.
    ///
    /// - At or below factory clocks: plain save to config.txt.
    /// - Above factory clocks: blocked while the supply shows under-voltage; otherwise a trial
    ///   boot (tryboot.txt) on boards that support it, or a save that requires accepting the risk.
    fn draw_firmware_save_controls(&mut self, ui: &mut Ui) {
        let t = texts(self.lang);
        let profile = crate::board::current().oc;
        let risky = self.boot_oc.exceeds_stock(&profile);

        if !risky {
            let resp =
                aorus_full_cta_button(ui, t.btn_save_firmware_oc, uc().btn_orange, uc().orange);
            if resp.clicked() {
                let res = save_boot_config_scoped(&self.boot_oc, ConfigScope::Overclock);
                self.log_result("Firmware OC (config.txt)", res);
            }
            aorus_tooltip(
                resp,
                t.btn_save_firmware_oc,
                t.btn_save_firmware_oc_tip,
                Some(get_boot_config_path()),
            );
            return;
        }

        if self.power_blocks_overclock() {
            ui.add_enabled(
                false,
                egui::Button::new(RichText::new(t.btn_save_firmware_oc).strong()),
            );
            ui.add(
                egui::Label::new(
                    RichText::new(t.oc_power_blocked.replace(
                        "{n}",
                        &self.telemetry.undervoltage_events.max(1).to_string(),
                    ))
                    .color(uc().red),
                )
                .wrap(),
            );
            return;
        }

        if crate::tryboot::supported() {
            ui.label(RichText::new(t.oc_needs_trial).color(uc().amber));
            let resp = aorus_full_cta_button(ui, t.btn_trial_prepare, uc().btn_orange, uc().orange);
            if resp.clicked() {
                let res = crate::tryboot::prepare(&self.boot_oc);
                self.log_result("Trial boot (tryboot.txt)", res);
                self.trial_status = crate::tryboot::status();
            }
            let path = crate::tryboot::tryboot_path().display().to_string();
            aorus_tooltip(
                resp,
                t.btn_trial_prepare,
                t.btn_trial_prepare_tip,
                Some(&path),
            );
        } else {
            ui.add(egui::Label::new(RichText::new(t.oc_no_tryboot).color(uc().amber)).wrap());
            ui.checkbox(&mut self.accept_oc_risk, t.oc_accept_risk);
            let resp = ui.add_enabled(
                self.accept_oc_risk,
                egui::Button::new(RichText::new(t.btn_save_firmware_oc).strong()),
            );
            if resp.clicked() {
                let res = save_boot_config_scoped(&self.boot_oc, ConfigScope::Overclock);
                self.log_result("Firmware OC (config.txt)", res);
                self.accept_oc_risk = false;
            }
        }
    }

    /// True when the 5 V supply has dipped since boot and the user has not overridden the block.
    /// Under-voltage seen since boot: kernel log, rpi_volt alarm or firmware flags.
    fn power_problem(&self) -> bool {
        let tel = &self.telemetry;
        tel.undervoltage_events > 0
            || tel.undervoltage_alarm
            || tel
                .firmware
                .throttled
                .map(|t| t.undervoltage_now() || t.undervoltage_since_boot())
                .unwrap_or(false)
    }

    fn power_blocks_overclock(&self) -> bool {
        self.power_problem() && !self.power_override
    }

    fn draw_power_guard_card(&mut self, ui: &mut Ui) {
        if !self.power_problem() {
            return;
        }
        let t = texts(self.lang);
        let events = self.telemetry.undervoltage_events.max(1);
        aorus_card_with_tip(ui, t.oc_power_title, t.oc_power_tip, uc().red, |ui| {
            ui.add(
                egui::Label::new(
                    RichText::new(t.oc_power_blocked.replace("{n}", &events.to_string()))
                        .strong()
                        .color(uc().red),
                )
                .wrap(),
            );
            if let Some(th) = self.telemetry.firmware.throttled {
                let (text, _) = throttle_summary(th, self.lang);
                ui.label(
                    RichText::new(format!("{}: {text}", t.mt_throttle))
                        .size(12.0)
                        .color(uc().text_secondary),
                )
                .on_hover_text("vcgencmd get_throttled");
            }
            ui.checkbox(&mut self.power_override, t.oc_power_override)
                .on_hover_text(t.oc_power_tip);
        });
        ui.add_space(12.0);
    }

    fn draw_trial_card(&mut self, ui: &mut Ui) {
        use crate::tryboot::Status;
        let status = self.trial_status.clone();
        if status == Status::Idle {
            return;
        }
        let t = texts(self.lang);
        let (text, color) = match &status {
            Status::Ready(s) => (t.trial_ready.replace("{s}", s), uc().cyan),
            Status::InTrial(s) => (t.trial_in.replace("{s}", s), uc().amber),
            Status::Failed(s) => (t.trial_failed.replace("{s}", s), uc().red),
            Status::NotKept(s) => (t.trial_not_kept.replace("{s}", s), uc().amber),
            Status::Idle => unreachable!(),
        };
        let stable_now = self
            .stability
            .as_ref()
            .map(|r| r.snapshot().verdict == Some(crate::stability::Verdict::Stable))
            .unwrap_or(false);
        let mut action: Option<&str> = None;
        aorus_card_with_tip(ui, t.trial_title, t.trial_tip, color, |ui| {
            ui.add(egui::Label::new(RichText::new(text).strong().color(uc().text_primary)).wrap());
            ui.add_space(6.0);
            ui.horizontal_wrapped(|ui| {
                match status {
                    Status::Ready(_) | Status::NotKept(_) => {
                        if crate::tryboot::tryboot_path().exists()
                            && aorus_action_button(
                                ui,
                                t.btn_trial_reboot,
                                uc().btn_orange,
                                uc().orange,
                            )
                            .on_hover_text(t.trial_tip)
                            .clicked()
                        {
                            action = Some("reboot");
                        }
                    }
                    Status::InTrial(_) => {
                        let keep = ui
                            .add_enabled(
                                stable_now,
                                egui::Button::new(RichText::new(t.btn_trial_keep).strong()),
                            )
                            .on_hover_text(t.btn_trial_keep_tip)
                            .on_disabled_hover_text(t.btn_trial_keep_tip);
                        if keep.clicked() {
                            action = Some("keep");
                        }
                    }
                    _ => {}
                }
                if ui.button(t.btn_trial_discard).clicked() {
                    action = Some("discard");
                }
            });
        });
        match action {
            Some("reboot") => {
                let res = crate::tryboot::reboot_into_trial();
                self.log_result("Trial boot", res);
            }
            Some("keep") => {
                let res = crate::tryboot::keep();
                if res.is_ok() {
                    self.boot_oc = load_boot_config();
                }
                self.log_result("Keep trial overclock", res);
            }
            Some("discard") => {
                let res = crate::tryboot::discard();
                self.log_result("Discard trial overclock", res);
            }
            _ => {}
        }
        if action.is_some() {
            self.trial_status = crate::tryboot::status();
        }
        ui.add_space(12.0);
    }

    fn draw_stability_card(&mut self, ui: &mut Ui) {
        use crate::stability::{Report, StabilityTest, Verdict};
        let t = texts(self.lang);
        let running = self
            .stability
            .as_ref()
            .map(|s| s.is_running())
            .unwrap_or(false);
        if running {
            ui.ctx().request_repaint_after(Duration::from_millis(500));
        }
        let mut start = false;
        let mut stop = false;
        aorus_card_with_tip(ui, t.stab_title, t.stab_tip, uc().green, |ui| {
            ui.horizontal_wrapped(|ui| {
                for m in [1_u32, 3, 10] {
                    let sel = self.stability_minutes == m;
                    let resp =
                        segmented_pill(ui, &format!("{m} min"), sel, uc().btn_green, uc().green);
                    if resp.clicked() && !running {
                        self.stability_minutes = m;
                    }
                }
                ui.add_space(8.0);
                if running {
                    if aorus_action_button(ui, t.btn_stab_stop, uc().btn_slate, uc().red).clicked()
                    {
                        stop = true;
                    }
                } else if aorus_action_button(ui, t.btn_stab_start, uc().btn_green, uc().green)
                    .on_hover_text(t.stab_tip)
                    .clicked()
                {
                    start = true;
                }
            });

            let current = self.stability.as_ref().map(|s| s.snapshot());
            let report: Option<Report> = current.or_else(|| self.last_stability.clone());
            let Some(r) = report else { return };
            ui.add_space(6.0);
            if r.finished || running {
                ui.add(egui::ProgressBar::new(r.progress()).show_percentage());
            }
            egui::Grid::new("stability_grid")
                .num_columns(2)
                .spacing([14.0, 4.0])
                .show(ui, |ui| {
                    spec_row(
                        ui,
                        t.stab_max_temp,
                        &self.temp_unit.format_temp(r.max_temp_c),
                    );
                    let min = if r.min_mhz_under_load == 0 || r.min_mhz_under_load == u32::MAX {
                        "—".to_string()
                    } else {
                        format!("{} / {} MHz", r.min_mhz_under_load, r.target_mhz)
                    };
                    spec_row(ui, t.stab_min_clock, &min);
                    spec_row(ui, t.stab_uv, &r.undervoltage_samples.to_string());
                });
            if !running {
                if !r.finished {
                    ui.add(
                        egui::Label::new(
                            RichText::new(
                                t.stab_interrupted
                                    .replace("{t}", &format!("{:.1}", r.last_temp_c))
                                    .replace(
                                        "{m}",
                                        &r.min_mhz_under_load.min(r.target_mhz).to_string(),
                                    ),
                            )
                            .strong()
                            .color(uc().red),
                        )
                        .wrap(),
                    );
                } else if let Some(v) = r.verdict {
                    let (txt, col) = match v {
                        Verdict::Stable => (t.stab_v_stable, uc().green),
                        Verdict::Throttled => (t.stab_v_throttled, uc().amber),
                        Verdict::TooHot => (t.stab_v_hot, uc().amber),
                        Verdict::Undervoltage => (t.stab_v_uv, uc().red),
                        Verdict::Stopped => (t.stab_v_stopped, uc().text_secondary),
                    };
                    let prefix = if self.stability.is_none() {
                        t.stab_last
                    } else {
                        ""
                    };
                    ui.label(
                        RichText::new(format!("{prefix} {txt}").trim().to_string())
                            .strong()
                            .color(col),
                    );
                }
            }
        });
        if stop {
            if let Some(s) = &self.stability {
                s.stop();
            }
        }
        if start {
            let context = match &self.trial_status {
                crate::tryboot::Status::InTrial(s) => s.clone(),
                _ => String::new(),
            };
            self.stability = Some(StabilityTest::start(
                Duration::from_secs(self.stability_minutes as u64 * 60),
                context,
            ));
        }
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
        let fw_tip = format!("{} ({})", t.card_firmware_oc_tip, get_boot_config_path());
        aorus_card_with_tip(ui, t.card_firmware_oc, &fw_tip, uc().orange, |ui| {
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
                            if self.gpu_available() {
                                ui.label("");
                                let load = self
                                    .telemetry
                                    .gpu_load
                                    .map(|l| format!("{:.0}%", l.overall()))
                                    .unwrap_or_else(|| "—".into());
                                ui.label(
                                    RichText::new(format!(
                                        "{}: {load} · {}",
                                        t.gpu_now,
                                        self.gpu_clock_text()
                                    ))
                                    .size(11.5)
                                    .color(uc().text_secondary),
                                )
                                .on_hover_text(t.gpu_tip);
                                ui.end_row();
                            }

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
                                        .color(uc().heading),
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
            self.draw_firmware_save_controls(ui);
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
            section_group_header(ui, t.group_buses, uc().heading);
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
                let res = save_boot_config_scoped(&self.boot_oc, ConfigScope::Features);
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
                ui.label(RichText::new(t.oled_section).strong().color(uc().heading))
                    .on_hover_text("/etc/argoneonoled.conf");
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

    /// Draws the 40-pin diagram with live GPIO states on top. Returns false when the diagram
    /// could not be rendered (the caller then shows the list view).
    fn draw_pinout_diagram(&mut self, ui: &mut Ui) -> bool {
        let t = texts(self.lang);
        if self.pinout.is_none() {
            if self.pinout_rx.is_some() {
                // Still rendering on the start-up thread: show a short placeholder.
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(RichText::new(t.gpio_diagram_loading).color(uc().text_secondary));
                });
                ui.ctx().request_repaint_after(Duration::from_millis(100));
                return true;
            }
            // Fallback (pre-render thread failed to start): render now.
            self.pinout = Some(PinoutTexture::from_render(
                ui.ctx(),
                crate::pinout::render(900),
            ));
        }
        let Some(diagram) = self.pinout.as_ref() else {
            return false;
        };
        let Some(texture) = diagram.texture.as_ref() else {
            return false;
        };

        let w = ui.available_width().min(620.0);
        let size = Vec2::new(w, w * diagram.aspect);
        let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
        let painter = ui.painter_at(rect);
        painter.image(
            texture.id(),
            rect,
            Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
            Color32::WHITE,
        );

        let pins = diagram.pins;
        let mut clicked: Option<u8> = None;
        for (i, def) in RPI_40PIN_HEADER.iter().enumerate() {
            let HeaderPinKind::Gpio(bcm) = def.kind else {
                continue;
            };
            let b = pins[i];
            let pin_rect = Rect::from_min_max(
                Pos2::new(rect.left() + b[0] * size.x, rect.top() + b[1] * size.y),
                Pos2::new(rect.left() + b[2] * size.x, rect.top() + b[3] * size.y),
            );
            let Some(state) = self.gpio_lab.pins.get(bcm as usize) else {
                continue;
            };
            let fill = if state.level_high {
                Color32::from_rgb(30, 170, 90)
            } else {
                Color32::from_rgb(52, 56, 64)
            };
            painter.rect_filled(pin_rect, CornerRadius::same(3), fill);
            painter.text(
                pin_rect.center(),
                Align2::CENTER_CENTER,
                def.phys_num.to_string(),
                FontId::new(
                    (pin_rect.height() * 0.55).clamp(8.0, 16.0),
                    FontFamily::Proportional,
                ),
                Color32::WHITE,
            );
            if self.gpio_lab.selected_bcm == bcm {
                painter.rect_stroke(
                    pin_rect.expand(2.0),
                    CornerRadius::same(4),
                    Stroke::new(2.5_f32, uc().orange),
                    StrokeKind::Outside,
                );
            }
            let resp = ui.interact(pin_rect, ui.id().with(("pinout-pin", bcm)), Sense::click());
            if resp.clicked() {
                clicked = Some(bcm);
            }
            if resp.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            let tip = format!(
                "{}\nDirection: {} | Pull: {}\nLogic level: {}\nCaptured edges: ⬆ {} rising | ⬇ {} falling",
                state.role_note,
                state.direction.short_label(),
                state.pull.short_label(),
                if state.level_high { "HIGH (3.3V)" } else { "LOW (0.0V)" },
                state.rising_edges,
                state.falling_edges
            );
            aorus_tooltip(
                resp,
                &format!("Physical pin #{} — GPIO {}", def.phys_num, bcm),
                &tip,
                Some(&format!("pinctrl get {bcm}")),
            );
        }
        if let Some(bcm) = clicked {
            self.gpio_lab.selected_bcm = bcm;
        }

        ui.horizontal_wrapped(|ui| {
            ui.label(
                RichText::new("■ HIGH")
                    .color(Color32::from_rgb(30, 170, 90))
                    .strong(),
            );
            ui.label(RichText::new("■ LOW").color(uc().text_secondary).strong());
            ui.label(
                RichText::new(t.gpio_diagram_hint)
                    .size(11.5)
                    .color(uc().text_secondary),
            );
        });
        true
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

                // View switch: annotated diagram (default) or the compact list.
                ui.horizontal_wrapped(|ui| {
                    for (diagram, label) in [(true, t.gpio_view_diagram), (false, t.gpio_view_list)]
                    {
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
                ui.add_space(4.0);
                let diagram_shown = self.gpio_view_diagram && self.draw_pinout_diagram(ui);

                // Interactive 40-Pin Raspberry Pi Physical Header (20 rows x 2 columns)
                if !diagram_shown {
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
                                    // Two columns mirror the physical header; narrow windows
                                    // list the pins 1..40 in a single column instead.
                                    let two_cols = ui.available_width() >= 2.0 * 210.0 + 30.0;
                                    egui::Grid::new("rpi_40pin_grid")
                                        .num_columns(if two_cols { 2 } else { 1 })
                                        .spacing([10.0, 4.0])
                                        .striped(true)
                                        .show(ui, |ui| {
                                            if two_cols {
                                                for row in 0..20 {
                                                    let odd_pin = &RPI_40PIN_HEADER[row * 2];
                                                    let even_pin = &RPI_40PIN_HEADER[row * 2 + 1];
                                                    self.draw_single_header_pin(ui, odd_pin, false);
                                                    self.draw_single_header_pin(ui, even_pin, true);
                                                    ui.end_row();
                                                }
                                            } else {
                                                for pin in RPI_40PIN_HEADER.iter() {
                                                    self.draw_single_header_pin(ui, pin, false);
                                                    ui.end_row();
                                                }
                                            }
                                        });
                                });
                        });
                }

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
                                    .color(uc().heading),
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
                                                "[{} | {}] {}  (⬆{} ⬇{})",
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
                let txt = format!("#{:02}  {}", def.phys_num, def.label);
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

                let led = if high { "⏺ 1" } else { "○ 0" };
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
                    "Function: {}\nDirection: {} | Pull: {}\nLogic level: {}\nCaptured edges: ⬆ {} rising | ⬇ {} falling",
                    role,
                    dir_str,
                    pull_str,
                    if high { "HIGH (3.3V)" } else { "LOW (0.0V)" },
                    rising,
                    falling
                );
                aorus_tooltip(
                    resp,
                    &format!("Physical pin #{} — GPIO {}", def.phys_num, bcm),
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
                                    .on_hover_text("Argon MCU · I2C 0x1a")
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
            format!("{} ⬆{} ⬇{}", state_txt, pin.rising_edges, pin.falling_edges),
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

/// Attaches a rich tooltip with title, explanation and an optional Linux/hardware path.
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
            ui.label(RichText::new(title).size(13.0).strong().color(uc().heading));
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
            RichText::new(label.to_string())
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
                ui.label(
                    RichText::new(title)
                        .size(13.0)
                        .strong()
                        .color(uc().text_primary),
                )
                .on_hover_text(desc);
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
                        RichText::new(title.to_string())
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

/// Radial gauge `height` pixels tall; radius, stroke and fonts scale with it. Clickable.
fn draw_radial_gauge(
    ui: &mut Ui,
    height: f32,
    ratio: f32,
    main_value: &str,
    unit_label: &str,
    sub_label: &str,
    accent: Color32,
) -> Response {
    let desired = Vec2::new(ui.available_width(), height);
    let (rect, resp) = ui.allocate_exact_size(desired, Sense::click());
    let painter = ui.painter_at(rect);

    let center = rect.center() + Vec2::new(0.0, 4.0);
    let radius = (rect.height().min(rect.width()) * 0.39).clamp(18.0, 58.0);
    let k = radius / 58.0;
    let stroke_w = (9.0 * k).max(4.0);

    let start_angle = 0.75 * PI;
    let sweep = 1.5 * PI;

    draw_arc(
        &painter,
        center,
        radius,
        start_angle,
        sweep,
        Stroke::new(stroke_w, gauge_track()),
    );

    let ticks = 18;
    for i in 0..=ticks {
        let t = i as f32 / ticks as f32;
        let ang = start_angle + t * sweep;
        let dir = Vec2::new(ang.cos(), ang.sin());
        let p1 = center + dir * (radius + 8.0 * k);
        let p2 = center + dir * (radius + 12.0 * k);
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
            Stroke::new(stroke_w, accent),
        );
    }

    painter.text(
        center + Vec2::new(0.0, -6.0 * k),
        Align2::CENTER_CENTER,
        main_value,
        FontId::new((24.0 * k).max(11.0), FontFamily::Proportional),
        uc().text_primary,
    );
    painter.text(
        center + Vec2::new(0.0, 15.0 * k),
        Align2::CENTER_CENTER,
        unit_label,
        FontId::new((12.5 * k).max(9.0), FontFamily::Proportional),
        accent,
    );

    painter.text(
        Pos2::new(center.x, rect.bottom() - 7.0),
        Align2::CENTER_CENTER,
        sub_label,
        FontId::new((12.0 * k).max(9.0), FontFamily::Proportional),
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
                ui.label(
                    RichText::new(title)
                        .size(12.5)
                        .strong()
                        .color(uc().text_primary),
                );
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

/// Navigation entries in display order: (tab, label, tooltip).
fn nav_tabs(lang: Language) -> [(ActiveTab, &'static str, &'static str); 7] {
    let t = texts(lang);
    [
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
        (ActiveTab::Settings, t.tab_settings, t.tab_settings_tip),
    ]
}

fn settings_label(ui: &mut Ui, text: &str) {
    ui.label(
        RichText::new(text)
            .size(13.0)
            .strong()
            .color(uc().text_primary),
    );
    ui.add_space(2.0);
}

/// Selectable option cards that wrap on narrow windows; the description is shown on hover.
/// Returns true when the selection changed.
fn choice_row<T: Copy + PartialEq>(
    ui: &mut Ui,
    value: &mut T,
    options: &[(T, &str, &str)],
) -> bool {
    let mut changed = false;
    let avail = ui.available_width();
    // As many cards per row as fit at ~150 px each (one per row on very small windows).
    let per_row = ((avail / 158.0).floor() as usize).clamp(1, options.len().max(1));
    let gap = 8.0;
    let card_w = ((avail - gap * (per_row as f32 - 1.0)) / per_row as f32).max(120.0);
    for chunk in options.chunks(per_row) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            for (opt, title, desc) in chunk {
                let selected = *value == *opt;
                let fill = if selected {
                    crate::system_colors::mix(uc().bg_card, uc().cyan, 0.18)
                } else {
                    uc().bg_subgroup
                };
                let stroke = if selected {
                    Stroke::new(1.5_f32, uc().cyan)
                } else {
                    Stroke::new(1.0_f32, uc().border_subtle)
                };
                let resp = egui::Frame::new()
                    .fill(fill)
                    .stroke(stroke)
                    .corner_radius(CornerRadius::same(5))
                    .inner_margin(egui::Margin::symmetric(10, 8))
                    .show(ui, |ui| {
                        // Card width minus inner margin (2 x 10) and stroke (2 x 1.5).
                        ui.set_width((card_w - 24.0).max(40.0));
                        ui.horizontal(|ui| {
                            radio_dot(ui, selected);
                            ui.label(
                                RichText::new(*title)
                                    .size(13.0)
                                    .strong()
                                    .color(uc().text_primary),
                            );
                        });
                    })
                    .response
                    .interact(Sense::click())
                    .on_hover_text(*desc);
                if resp.clicked() && !selected {
                    *value = *opt;
                    changed = true;
                }
                if resp.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
            }
        });
        ui.add_space(gap);
    }
    changed
}

/// Rendered pin-out diagram kept as a GPU texture. The rasterization (font loading + SVG)
/// runs on a background thread at start-up, so opening the GPIO lab is instant.
struct PinoutTexture {
    texture: Option<egui::TextureHandle>,
    aspect: f32,
    pins: [[f32; 4]; 40],
}

impl PinoutTexture {
    fn from_render(ctx: &egui::Context, rendered: Option<crate::pinout::Pinout>) -> Self {
        match rendered {
            Some(p) => PinoutTexture {
                texture: Some(ctx.load_texture(
                    "gpio-pinout",
                    egui::ColorImage::from_rgba_premultiplied(
                        [p.width as usize, p.height as usize],
                        &p.rgba,
                    ),
                    egui::TextureOptions::LINEAR,
                )),
                aspect: p.height as f32 / p.width as f32,
                pins: p.pins,
            },
            None => PinoutTexture {
                texture: None,
                aspect: 1.0,
                pins: [[0.0; 4]; 40],
            },
        }
    }
}

/// Radio-button marker drawn with shapes (font independent).
fn radio_dot(ui: &mut Ui, selected: bool) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(14.0), Sense::hover());
    let c = rect.center();
    let ring = if selected {
        uc().cyan
    } else {
        uc().text_secondary
    };
    ui.painter()
        .circle_stroke(c, 6.0, Stroke::new(1.5_f32, ring));
    if selected {
        ui.painter().circle_filled(c, 3.5, uc().cyan);
    }
}

/// Page opened by clicking a dashboard ring (memory has no settings page).
fn ring_target(lang: Language, title: &str) -> Option<ActiveTab> {
    let t = texts(lang);
    if title == t.gauge_cpu_clock {
        Some(ActiveTab::Overclocking)
    } else if title == t.gauge_cpu_temp || title == t.gauge_fan_speed {
        Some(ActiveTab::SmartFan)
    } else {
        None
    }
}

/// One radial gauge with its title, tooltip and optional page link.
struct GaugeSpec {
    title: String,
    ratio: f32,
    value: String,
    unit: String,
    sub: String,
    color: Color32,
    tip: &'static str,
    tab: Option<ActiveTab>,
}

fn gauge_cell(ui: &mut Ui, g: &GaugeSpec, height: f32, go: &mut Option<ActiveTab>) {
    ui.vertical_centered(|ui| {
        ui.label(
            RichText::new(&g.title)
                .size(12.0)
                .strong()
                .color(uc().heading),
        );
    });
    let resp = draw_radial_gauge(
        ui,
        height,
        g.ratio.clamp(0.0, 1.0),
        &g.value,
        &g.unit,
        &g.sub,
        g.color,
    );
    if g.tab.is_some() && resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if resp.clicked() {
        if let Some(tab) = g.tab {
            *go = Some(tab);
        }
    }
    resp.on_hover_text(g.tip);
}

/// Gauges laid out in rows; as many per row as fit at `min_w` pixels each.
fn gauge_grid(
    ui: &mut Ui,
    items: &[GaugeSpec],
    height: f32,
    min_w: f32,
    go: &mut Option<ActiveTab>,
) {
    if items.is_empty() {
        return;
    }
    let per_row = ((ui.available_width() / min_w).floor() as usize).clamp(1, items.len());
    for chunk in items.chunks(per_row) {
        ui.columns(per_row, |cols| {
            for (col, g) in cols.iter_mut().zip(chunk) {
                gauge_cell(col, g, height, go);
            }
        });
    }
}

/// Green below 60 %, amber below 85 %, red above.
fn level_color(frac: f32) -> Color32 {
    if frac < 0.6 {
        uc().green
    } else if frac < 0.85 {
        uc().amber
    } else {
        uc().red
    }
}

/// btop-style meter: `label  ■■■■■■■□□□□□  value`. The bar is made of blocks filled from left
/// to right (0 -> 100 %); each block takes its colour from a green -> amber -> red gradient
/// according to its position, so the fill shows where the value sits on the scale.
fn gradient_meter(ui: &mut Ui, label: &str, frac: f32, value: &str) -> Response {
    let w = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, 17.0), Sense::click());
    let p = ui.painter_at(rect);
    let mono = FontId::monospace(11.5);
    let label_w = 58.0;
    let value_w = (w * 0.30).clamp(66.0, 120.0);
    p.text(
        Pos2::new(rect.left(), rect.center().y),
        Align2::LEFT_CENTER,
        label,
        mono.clone(),
        uc().heading,
    );
    let bar = Rect::from_min_max(
        Pos2::new(rect.left() + label_w, rect.top() + 4.0),
        Pos2::new(
            (rect.right() - value_w - 6.0).max(rect.left() + label_w + 12.0),
            rect.bottom() - 4.0,
        ),
    );
    let (block, gap) = (4.0, 1.5);
    let n = ((bar.width() + gap) / (block + gap)).floor().max(1.0) as usize;
    let filled = (frac.clamp(0.0, 1.0) * n as f32).round() as usize;
    let track = crate::system_colors::mix(uc().bg_subgroup, uc().text_secondary, 0.18);
    for i in 0..n {
        let x = bar.left() + i as f32 * (block + gap);
        let r = Rect::from_min_size(Pos2::new(x, bar.top()), Vec2::new(block, bar.height()));
        let color = if i < filled {
            gradient_color(i as f32 / n.max(2).saturating_sub(1) as f32)
        } else {
            track
        };
        p.rect_filled(r, CornerRadius::same(1), color);
    }
    p.text(
        Pos2::new(rect.right(), rect.center().y),
        Align2::RIGHT_CENTER,
        value,
        mono,
        uc().text_primary,
    );
    resp
}

/// green (0) -> amber (0.5) -> red (1).
fn gradient_color(pos: f32) -> Color32 {
    let pos = pos.clamp(0.0, 1.0);
    if pos < 0.5 {
        crate::system_colors::mix(uc().green, uc().amber, pos * 2.0)
    } else {
        crate::system_colors::mix(uc().amber, uc().red, (pos - 0.5) * 2.0)
    }
}

/// btop-style box: rounded border with the title set into the top edge.
fn btop_box(ui: &mut Ui, title: &str, accent: Color32, add_contents: impl FnOnce(&mut Ui)) {
    let bg = ui.visuals().panel_fill;
    let resp = egui::Frame::new()
        .stroke(Stroke::new(1.2_f32, accent))
        .corner_radius(CornerRadius::same(6))
        .inner_margin(egui::Margin {
            left: 10,
            right: 10,
            top: 12,
            bottom: 8,
        })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add_contents(ui);
        })
        .response;
    let font = FontId::monospace(12.0);
    let galley = ui
        .painter()
        .layout_no_wrap(format!(" {title} "), font, accent);
    let pos = Pos2::new(
        resp.rect.left() + 12.0,
        resp.rect.top() - galley.size().y / 2.0,
    );
    ui.painter().rect_filled(
        Rect::from_min_size(pos, galley.size()),
        CornerRadius::ZERO,
        bg,
    );
    ui.painter().galley(pos, galley, accent);
}

/// htop header style `Key   value` line; `link` shows a pointer cursor.
fn info_row(ui: &mut Ui, key: &str, value: &str, color: Color32, link: bool) -> Response {
    let (rect, resp) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 18.0), Sense::click());
    let p = ui.painter_at(rect);
    let mono = FontId::monospace(12.0);
    p.text(
        Pos2::new(rect.left(), rect.center().y),
        Align2::LEFT_CENTER,
        key,
        mono.clone(),
        uc().heading,
    );
    p.text(
        Pos2::new(rect.left() + 70.0, rect.center().y),
        Align2::LEFT_CENTER,
        value,
        mono,
        color,
    );
    if link && resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

/// Two columns side by side when there is room (>= 520 px), stacked otherwise.
fn two_columns(ui: &mut Ui, left: impl FnOnce(&mut Ui), right: impl FnOnce(&mut Ui)) {
    if ui.available_width() >= 520.0 {
        ui.scope(|ui| {
            // Gap between the two columns (egui uses the horizontal item spacing).
            ui.spacing_mut().item_spacing.x = 28.0;
            ui.columns(2, |cols| {
                let (l, r) = cols.split_at_mut(1);
                left(&mut l[0]);
                right(&mut r[0]);
            });
        });
    } else {
        left(ui);
        ui.add_space(6.0);
        right(ui);
    }
}

/// Human summary of the firmware throttling flags; `true` when anything was reported.
fn throttle_summary(th: crate::gpu::Throttled, lang: Language) -> (String, bool) {
    let t = texts(lang);
    if !th.any() {
        return (t.thr_ok.to_string(), false);
    }
    let mut now = Vec::new();
    let mut boot = Vec::new();
    for (active, past, name) in [
        (
            th.undervoltage_now(),
            th.undervoltage_since_boot(),
            t.thr_uv,
        ),
        (
            th.freq_capped_now(),
            th.freq_capped_since_boot(),
            t.thr_capped,
        ),
        (
            th.throttled_now(),
            th.throttled_since_boot(),
            t.thr_throttled,
        ),
        (
            th.soft_temp_limit_now(),
            th.soft_temp_limit_since_boot(),
            t.thr_soft,
        ),
    ] {
        if active {
            now.push(name);
        } else if past {
            boot.push(name);
        }
    }
    let mut parts = Vec::new();
    if !now.is_empty() {
        parts.push(format!("{} ({})", now.join(", "), t.thr_now));
    }
    if !boot.is_empty() {
        parts.push(format!("{} ({})", boot.join(", "), t.thr_boot));
    }
    (parts.join("; "), true)
}

/// Several `key value` pairs on one line, in equal-width cells (related values grouped).
fn info_pairs_row(ui: &mut Ui, pairs: &[(&str, String, Color32)]) {
    if pairs.is_empty() {
        return;
    }
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 18.0), Sense::hover());
    let p = ui.painter_at(rect);
    let mono = FontId::monospace(12.0);
    let cell = rect.width() / pairs.len() as f32;
    for (i, (key, value, color)) in pairs.iter().enumerate() {
        let x = rect.left() + i as f32 * cell;
        let key_galley = p.layout_no_wrap(key.to_string(), mono.clone(), uc().heading);
        let key_w = key_galley.size().x;
        p.galley(
            Pos2::new(x, rect.center().y - key_galley.size().y / 2.0),
            key_galley,
            uc().heading,
        );
        p.text(
            Pos2::new(x + key_w + 6.0, rect.center().y),
            Align2::LEFT_CENTER,
            value,
            mono.clone(),
            *color,
        );
    }
}
