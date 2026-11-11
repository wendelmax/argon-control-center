//! Fan page: a live strip (animated fan, temperature gauge, duty and status), the curve
//! editor with the live operating point, profile cards, a knob for the manual speed and a
//! bar for unsaved curve changes.

use super::*;
use std::f32::consts::TAU;

/// Temperature range of the curve editor and the gauge.
const T_MIN: f32 = 25.0;
const T_MAX: f32 = 90.0;
/// Fan revolutions per second drawn at 100 % duty.
const SPIN_RPS_AT_FULL: f32 = 2.5;
const PRESET_ANIM: Duration = Duration::from_millis(280);

/// Preset transition in the curve editor (only when both curves have the same number of points).
pub(super) struct CurveAnim {
    from: Vec<FanCurvePoint>,
    to: Vec<FanCurvePoint>,
    start: Instant,
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn ease_out(t: f32) -> f32 {
    1.0 - (1.0 - t).powi(3)
}

fn same_curve(a: &[FanCurvePoint], b: &[FanCurvePoint]) -> bool {
    a.len() == b.len()
        && a.iter()
            .zip(b)
            .all(|(x, y)| (x.temp_c - y.temp_c).abs() < 0.01 && x.speed_pct == y.speed_pct)
}

/// Number of points that differ from the saved curve (added or removed points count too).
fn changed_points(edited: &[FanCurvePoint], saved: &[FanCurvePoint]) -> usize {
    let mut a = edited.to_vec();
    let mut b = saved.to_vec();
    let by_temp = |x: &FanCurvePoint, y: &FanCurvePoint| {
        x.temp_c
            .partial_cmp(&y.temp_c)
            .unwrap_or(std::cmp::Ordering::Equal)
    };
    a.sort_by(by_temp);
    b.sort_by(by_temp);
    let common = a.len().min(b.len());
    let differing = a
        .iter()
        .zip(&b)
        .filter(|(x, y)| (x.temp_c - y.temp_c).abs() >= 0.01 || x.speed_pct != y.speed_pct)
        .count();
    differing + a.len().max(b.len()) - common
}

fn temp_color(temp: f32) -> Color32 {
    if temp >= 70.0 {
        uc().red
    } else if temp >= 55.0 {
        uc().amber
    } else {
        uc().green
    }
}

/// Snaps a dragged speed like argononed does: below 15 % is off, otherwise at least 25 %.
fn snap_speed(speed: u8) -> u8 {
    if speed < 15 {
        0
    } else {
        speed.max(25)
    }
}

impl ArgonControlCenterApp {
    pub(super) fn draw_smart_fan_tab(&mut self, ui: &mut Ui) {
        self.draw_fan_bus_warning(ui);
        self.fan_live_strip(ui);
        ui.add_space(10.0);
        responsive_two_columns(
            ui,
            self,
            |app, ui| app.fan_curve_card(ui),
            |app, ui| app.fan_controls_card(ui),
        );
        if self.fan_control_subtab == FanControlSubTab::AutoCurve {
            ui.add_space(10.0);
            self.fan_unsaved_bar(ui);
        }
    }

    fn active_curve_mut(&mut self) -> &mut Vec<FanCurvePoint> {
        match self.fan_target_mode {
            FanTargetMode::Cpu => &mut self.cpu_fan_curve,
            FanTargetMode::Hdd => &mut self.hdd_fan_curve,
        }
    }

    fn saved_curve(&self) -> &[FanCurvePoint] {
        match self.fan_target_mode {
            FanTargetMode::Cpu => &self.saved_cpu_curve,
            FanTargetMode::Hdd => &self.saved_hdd_curve,
        }
    }

    /// Duty the fan is running at: the manual override, or the saved curve at the current
    /// temperature.
    fn effective_duty(&self) -> u8 {
        self.telemetry
            .manual_fan_override
            .unwrap_or_else(|| evaluate_fan_speed(self.telemetry.cpu_temp_c, &self.saved_cpu_curve))
    }

    /// True when the duty actually reaches the fan (bus present, and the daemon or a manual
    /// override drives it).
    fn fan_driven(&self) -> bool {
        crate::argon::fan_bus_available()
            && (self.telemetry.argon_service_active || self.telemetry.manual_fan_override.is_some())
    }

    fn apply_preset(&mut self, preset: FanPreset) {
        self.selected_fan_preset = preset;
        if preset == FanPreset::Custom {
            return;
        }
        let to = preset.points();
        let from = self.active_curve_mut().clone();
        if from.len() == to.len() {
            self.curve_anim = Some(CurveAnim {
                from,
                to: to.clone(),
                start: Instant::now(),
            });
        }
        *self.active_curve_mut() = to;
    }

    // ------------------------------------------------------------------ live strip

    fn fan_live_strip(&mut self, ui: &mut Ui) {
        let f = texts(self.lang).fan;
        let duty = self.effective_duty();
        let driven = self.fan_driven();
        let temp = self.telemetry.cpu_temp_c;

        // Advance the fan animation.
        if driven && duty > 0 {
            let dt = ui.input(|i| i.stable_dt).min(0.1);
            self.fan_angle =
                (self.fan_angle + dt * SPIN_RPS_AT_FULL * duty as f32 / 100.0 * TAU) % TAU;
            ui.ctx().request_repaint_after(Duration::from_millis(33));
        }

        let thresholds: Vec<f32> = self.saved_cpu_curve.iter().map(|p| p.temp_c).collect();
        let angle = self.fan_angle;
        let manual = self.telemetry.manual_fan_override.is_some();
        let preset = self.selected_fan_preset.label(self.lang);
        let daemon = self.telemetry.argon_service_active;
        let bus = crate::argon::fan_bus_available();
        let unsaved = !same_curve(&self.cpu_fan_curve, &self.saved_cpu_curve)
            || !same_curve(&self.hdd_fan_curve, &self.saved_hdd_curve);

        aorus_card_with_tip(ui, f.live, f.live_tip, uc().cyan, |ui| {
            ui.horizontal_wrapped(|ui| {
                // Spread the four items over the width when they fit; otherwise wrap.
                let gap = ((ui.available_width() - 6e+02) / 4.0).max(28.0);
                let half = gap / 2.0;
                ui.spacing_mut().item_spacing = Vec2::new(0.0, 10.0);
                ui.add_space(half);
                draw_fan_rotor(ui, angle, duty, driven);
                ui.add_space(gap);
                draw_temp_gauge(ui, temp, &thresholds, f.temperature);

                // Duty readout.
                ui.add_space(gap);
                let (rect, _) = ui.allocate_exact_size(Vec2::new(150.0, 110.0), Sense::hover());
                let p = ui.painter_at(rect);
                p.text(
                    Pos2::new(rect.left(), rect.top() + 34.0),
                    Align2::LEFT_CENTER,
                    format!("{duty}%"),
                    FontId::new(38.0, FontFamily::Proportional),
                    if driven {
                        uc().text_primary
                    } else {
                        uc().text_secondary
                    },
                );
                p.text(
                    Pos2::new(rect.left(), rect.top() + 66.0),
                    Align2::LEFT_CENTER,
                    f.duty,
                    FontId::new(12.0, FontFamily::Proportional),
                    uc().text_secondary,
                );
                p.text(
                    Pos2::new(rect.left(), rect.top() + 88.0),
                    Align2::LEFT_CENTER,
                    if manual {
                        f.mode_manual.to_string()
                    } else {
                        format!("{} · {preset}", f.mode_curve)
                    },
                    FontId::new(12.5, FontFamily::Proportional),
                    uc().heading,
                );

                // Status lines.
                ui.add_space(gap);
                let (rect, _) = ui.allocate_exact_size(Vec2::new(190.0, 110.0), Sense::hover());
                let p = ui.painter_at(rect);
                let rows = [
                    (
                        daemon,
                        "argononed",
                        if daemon { f.active } else { f.stopped },
                    ),
                    (bus, "I2C", if bus { f.connected } else { f.off }),
                    (!unsaved, f.curve, if unsaved { f.unsaved } else { f.saved }),
                ];
                for (i, (ok, name, value)) in rows.iter().enumerate() {
                    let y = rect.top() + 24.0 + i as f32 * 28.0;
                    let col = if *ok { uc().green } else { uc().amber };
                    p.circle_filled(Pos2::new(rect.left() + 6.0, y), 5.0, col);
                    p.text(
                        Pos2::new(rect.left() + 18.0, y),
                        Align2::LEFT_CENTER,
                        *name,
                        FontId::new(13.0, FontFamily::Proportional),
                        uc().text_primary,
                    );
                    p.text(
                        Pos2::new(rect.left() + 100.0, y),
                        Align2::LEFT_CENTER,
                        *value,
                        FontId::new(12.5, FontFamily::Proportional),
                        uc().text_secondary,
                    );
                }
            });
        });
    }

    // ------------------------------------------------------------------ curve card

    fn fan_curve_card(&mut self, ui: &mut Ui) {
        let t = texts(self.lang);
        let f = t.fan;
        aorus_card(ui, t.card_fan_curve_editor, uc().orange, |ui| {
            ui.horizontal(|ui| {
                for (mode, label, path) in [
                    (FanTargetMode::Cpu, t.cpu_fan_target, CPU_FAN_CONF),
                    (FanTargetMode::Hdd, t.hdd_fan_target, HDD_FAN_CONF),
                ] {
                    let r = segmented_pill(
                        ui,
                        label,
                        self.fan_target_mode == mode,
                        uc().btn_orange,
                        uc().orange,
                    );
                    if r.clicked() {
                        self.fan_target_mode = mode;
                        self.curve_anim = None;
                    }
                    r.on_hover_text(path);
                }
            });
            ui.add_space(6.0);

            // Preset transition: draw the interpolated curve while it runs.
            let mut shown: Option<Vec<FanCurvePoint>> = None;
            if let Some(anim) = &self.curve_anim {
                let k = anim.start.elapsed().as_secs_f32() / PRESET_ANIM.as_secs_f32();
                if k >= 1.0 {
                    self.curve_anim = None;
                } else {
                    let e = ease_out(k);
                    shown = Some(
                        anim.from
                            .iter()
                            .zip(&anim.to)
                            .map(|(a, b)| FanCurvePoint {
                                temp_c: lerp(a.temp_c, b.temp_c, e),
                                speed_pct: lerp(a.speed_pct as f32, b.speed_pct as f32, e).round()
                                    as u8,
                            })
                            .collect(),
                    );
                    ui.ctx().request_repaint();
                }
            }

            let temp = self.telemetry.cpu_temp_c;
            let trail: Vec<(f32, f32)> = self
                .telemetry
                .temp_history
                .iter()
                .zip(&self.telemetry.fan_history)
                .map(|(t, d)| (*t, *d))
                .collect();
            let pulse = self.fan_driven();
            let mut edit = CurveEdit::None;
            match shown {
                Some(mut pts) => {
                    draw_curve_editor(ui, &mut pts, &mut None, temp, &trail, pulse, f);
                }
                None => {
                    let mut dragging = self.dragging_point_idx;
                    let curve = match self.fan_target_mode {
                        FanTargetMode::Cpu => &mut self.cpu_fan_curve,
                        FanTargetMode::Hdd => &mut self.hdd_fan_curve,
                    };
                    edit = draw_curve_editor(ui, curve, &mut dragging, temp, &trail, pulse, f);
                    self.dragging_point_idx = dragging;
                }
            }
            if edit != CurveEdit::None {
                self.selected_fan_preset = FanPreset::Custom;
            }

            ui.add_space(4.0);
            ui.label(
                RichText::new(f.curve_hint)
                    .size(11.5)
                    .color(uc().text_secondary),
            );
        });
    }

    // ------------------------------------------------------------------ controls card

    fn fan_controls_card(&mut self, ui: &mut Ui) {
        let t = texts(self.lang);
        let f = t.fan;
        aorus_card(ui, t.card_fan_profiles, uc().cyan, |ui| {
            ui.horizontal(|ui| {
                let r = segmented_pill(
                    ui,
                    f.mode_curve,
                    self.fan_control_subtab == FanControlSubTab::AutoCurve,
                    uc().btn_cyan,
                    uc().cyan,
                );
                if r.clicked() {
                    self.fan_control_subtab = FanControlSubTab::AutoCurve;
                }
                aorus_tooltip(
                    r,
                    f.mode_curve,
                    t.fan_tab_auto_curve_tip,
                    Some("argononed.service"),
                );
                let r = segmented_pill(
                    ui,
                    f.mode_manual,
                    self.fan_control_subtab == FanControlSubTab::ManualOverride,
                    uc().btn_orange,
                    uc().orange,
                );
                if r.clicked() {
                    self.fan_control_subtab = FanControlSubTab::ManualOverride;
                }
                aorus_tooltip(
                    r,
                    f.mode_manual,
                    t.fan_tab_manual_override_tip,
                    Some("/dev/i2c-1 @ 0x1a"),
                );
            });
            ui.add_space(10.0);
            match self.fan_control_subtab {
                FanControlSubTab::AutoCurve => self.fan_curve_controls(ui),
                FanControlSubTab::ManualOverride => self.fan_manual_controls(ui),
            }
        });
    }

    fn fan_curve_controls(&mut self, ui: &mut Ui) {
        let t = texts(self.lang);
        let f = t.fan;
        if !self.telemetry.argon_service_active {
            egui::Frame::new()
                .fill(tint(uc().red))
                .stroke(Stroke::new(1.0_f32, uc().red))
                .corner_radius(CornerRadius::same(6))
                .inner_margin(egui::Margin::symmetric(10, 8))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(
                        RichText::new(t.smart_fan_daemon_off_warn)
                            .size(12.5)
                            .color(uc().amber),
                    );
                    ui.add_space(4.0);
                    let r = aorus_action_button(
                        ui,
                        t.btn_quick_start_daemon,
                        uc().btn_green,
                        uc().green,
                    );
                    if r.clicked() {
                        let res = control_argononed_service("start");
                        self.log_result("Start argononed.service", res);
                        self.telemetry.last_slow_refresh = None;
                    }
                    aorus_tooltip(
                        r,
                        t.btn_quick_start_daemon,
                        t.btn_start_tip,
                        Some("systemctl start argononed.service"),
                    );
                });
            ui.add_space(10.0);
        }

        // Profile cards with a thumbnail of each curve.
        ui.label(
            RichText::new(f.profiles)
                .size(12.0)
                .strong()
                .color(uc().heading),
        );
        ui.add_space(4.0);
        let presets = [
            FanPreset::Silent,
            FanPreset::BalancedDefault,
            FanPreset::Performance,
            FanPreset::AlwaysOnTurbo,
        ];
        let gap = 8.0;
        let per_row = if ui.available_width() >= 4.0 * 92.0 + 3.0 * gap {
            4
        } else {
            2
        };
        let card_w = (ui.available_width() - gap * (per_row as f32 - 1.0)) / per_row as f32;
        let mut picked: Option<FanPreset> = None;
        for row in presets.chunks(per_row) {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = gap;
                for &preset in row {
                    let selected = self.selected_fan_preset == preset;
                    if profile_card(ui, preset, preset.label(self.lang), selected, card_w).clicked()
                    {
                        picked = Some(preset);
                    }
                }
            });
            ui.add_space(gap);
        }
        if let Some(p) = picked {
            self.apply_preset(p);
        }
        if self.selected_fan_preset == FanPreset::Custom {
            ui.label(
                RichText::new(f.custom_curve)
                    .size(12.0)
                    .color(uc().text_secondary),
            );
        }

        ui.add_space(6.0);
        let count = match self.fan_target_mode {
            FanTargetMode::Cpu => self.cpu_fan_curve.len(),
            FanTargetMode::Hdd => self.hdd_fan_curve.len(),
        };
        egui::CollapsingHeader::new(format!("{} ({count})", f.points))
            .id_salt("fan_points_table")
            .default_open(false)
            .show(ui, |ui| self.fan_points_table(ui));
    }

    /// Exact values of the curve points (the same points the editor drags).
    fn fan_points_table(&mut self, ui: &mut Ui) {
        let t = texts(self.lang);
        let mut remove_idx: Option<usize> = None;
        let mut changed = false;
        let curve = match self.fan_target_mode {
            FanTargetMode::Cpu => &mut self.cpu_fan_curve,
            FanTargetMode::Hdd => &mut self.hdd_fan_curve,
        };
        let can_remove = curve.len() > 1;
        ui.spacing_mut().slider_width = (ui.available_width() - 250.0).clamp(60.0, 200.0);
        egui::Grid::new("fan_stages_grid")
            .num_columns(4)
            .spacing([10.0, 6.0])
            .striped(true)
            .show(ui, |ui| {
                for (idx, pt) in curve.iter_mut().enumerate() {
                    ui.label(
                        RichText::new(format!("#{}", idx + 1))
                            .strong()
                            .color(uc().heading),
                    );
                    changed |= ui
                        .add(
                            egui::DragValue::new(&mut pt.temp_c)
                                .speed(0.5)
                                .range(1.0..=95.0)
                                .suffix(" °C"),
                        )
                        .changed();
                    changed |= ui
                        .add(egui::Slider::new(&mut pt.speed_pct, 0..=100).suffix(" %"))
                        .changed();
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
        if let Some(idx) = remove_idx {
            curve.remove(idx);
            changed = true;
        }
        let add = ui.button(t.btn_add_stage);
        if add.clicked() {
            let (last_t, last_s) = curve
                .last()
                .map(|p| (p.temp_c, p.speed_pct))
                .unwrap_or((50.0, 30));
            curve.push(FanCurvePoint {
                temp_c: (last_t + 5.0).min(90.0),
                speed_pct: (last_s + 20).min(100),
            });
            changed = true;
        }
        aorus_tooltip(add, t.btn_add_stage, t.btn_add_stage_tip, None);
        if changed {
            self.selected_fan_preset = FanPreset::Custom;
        }
    }

    fn fan_manual_controls(&mut self, ui: &mut Ui) {
        let t = texts(self.lang);
        let f = t.fan;
        ui.label(
            RichText::new(t.fan_manual_desc)
                .size(12.5)
                .color(uc().text_secondary),
        );
        ui.add_space(8.0);
        ui.vertical_centered(|ui| {
            speed_knob(ui, &mut self.manual_fan_slider, f.knob_hint);
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                // Center the chips under the knob.
                let w = 5.0 * 46.0 + 4.0 * ui.spacing().item_spacing.x;
                ui.add_space(((ui.available_width() - w) / 2.0).max(0.0));
                for pct in [0u8, 25, 50, 75, 100] {
                    let r = ui.add_sized(
                        [46.0, 24.0],
                        egui::Button::new(RichText::new(format!("{pct}%")).size(12.0).color(
                            if self.manual_fan_slider == pct {
                                Color32::WHITE
                            } else {
                                uc().text_primary
                            },
                        ))
                        .fill(if self.manual_fan_slider == pct {
                            uc().btn_orange
                        } else {
                            uc().btn_slate
                        })
                        .corner_radius(CornerRadius::same(12)),
                    );
                    if r.clicked() {
                        self.manual_fan_slider = pct;
                    }
                }
            });
        });
        ui.add_space(12.0);
        let apply = aorus_full_cta_button(ui, t.btn_apply_manual_fan, uc().btn_green, uc().green);
        if apply.clicked() {
            let spd = self.manual_fan_slider;
            let res = set_immediate_fan_speed(spd);
            if res.is_ok() {
                self.telemetry.manual_fan_override = Some(spd);
            }
            self.log_result(&format!("Manual fan ({spd}%)"), res);
        }
        aorus_tooltip(
            apply,
            t.btn_apply_manual_fan,
            t.btn_apply_manual_fan_tip,
            Some("/dev/i2c-1 @ 0x1a (reg 0x80)"),
        );
        if self.telemetry.manual_fan_override.is_some() {
            ui.add_space(6.0);
            if aorus_full_cta_button(ui, t.btn_return_auto, uc().btn_cyan, uc().cyan).clicked() {
                self.telemetry.manual_fan_override = None;
                let res = control_argononed_service("restart");
                self.log_result("Restore automatic fan curve", res);
            }
        }
    }

    // ------------------------------------------------------------------ unsaved bar

    fn fan_unsaved_bar(&mut self, ui: &mut Ui) {
        let t = texts(self.lang);
        let f = t.fan;
        let (edited, saved) = match self.fan_target_mode {
            FanTargetMode::Cpu => (&self.cpu_fan_curve, &self.saved_cpu_curve),
            FanTargetMode::Hdd => (&self.hdd_fan_curve, &self.saved_hdd_curve),
        };
        let changes = changed_points(edited, saved);
        let dirty = changes > 0;
        let accent = if dirty { uc().amber } else { uc().green };
        let mut save = false;
        let mut revert = false;
        egui::Frame::new()
            .fill(accent.gamma_multiply(0.10))
            .stroke(Stroke::new(1.0_f32, accent.gamma_multiply(0.6)))
            .corner_radius(CornerRadius::same(6))
            .inner_margin(egui::Margin::symmetric(12, 8))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    let (r, _) = ui.allocate_exact_size(Vec2::new(4.0, 20.0), Sense::hover());
                    ui.painter().rect_filled(r, CornerRadius::same(2), accent);
                    let text = if dirty {
                        format!("{} · {changes} {}", f.unsaved_changes, f.points_word)
                    } else {
                        f.all_saved.to_string()
                    };
                    ui.label(
                        RichText::new(text)
                            .size(12.5)
                            .strong()
                            .color(uc().text_primary),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.add_enabled_ui(dirty, |ui| {
                            let r = aorus_action_button(
                                ui,
                                t.btn_save_fan_curve,
                                uc().btn_orange,
                                uc().orange,
                            );
                            save = r.clicked();
                            aorus_tooltip(
                                r,
                                t.btn_save_fan_curve,
                                t.btn_save_fan_curve_tip,
                                Some(CPU_FAN_CONF),
                            );
                            revert = ui.button(f.revert).clicked();
                        });
                    });
                });
            });
        if revert {
            let saved = self.saved_curve().to_vec();
            *self.active_curve_mut() = saved;
            self.selected_fan_preset = FanPreset::Custom;
            self.curve_anim = None;
        }
        if save {
            self.telemetry.manual_fan_override = None;
            let (path, label, pts) = match self.fan_target_mode {
                FanTargetMode::Cpu => (CPU_FAN_CONF, "CPU", self.cpu_fan_curve.clone()),
                FanTargetMode::Hdd => (HDD_FAN_CONF, "HDD", self.hdd_fan_curve.clone()),
            };
            let res = save_fan_curve(path, &pts, label);
            // The file is written even when the bus is off (the error explains it).
            let written = res.is_ok() || res.as_ref().is_err_and(|e| e.contains(" saved, but "));
            if written {
                match self.fan_target_mode {
                    FanTargetMode::Cpu => self.saved_cpu_curve = pts,
                    FanTargetMode::Hdd => self.saved_hdd_curve = pts,
                }
            }
            self.log_result(&format!("Save fan curve ({label})"), res);
        }
    }
}

// ---------------------------------------------------------------------- widgets

/// Fan housing with five blades rotated by `angle`; grey and still when not driven.
fn draw_fan_rotor(ui: &mut Ui, angle: f32, duty: u8, driven: bool) {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(110.0), Sense::hover());
    let p = ui.painter_at(rect);
    let c = rect.center();
    let r = rect.width() / 2.0 - 4.0;
    let active = driven && duty > 0;
    let accent = if active {
        uc().cyan
    } else {
        uc().text_secondary
    };

    p.circle_filled(c, r, uc().bg_subgroup);
    p.circle_stroke(c, r, Stroke::new(2.0_f32, uc().border_subtle));
    // Duty ring.
    if duty > 0 {
        let sweep = TAU * duty as f32 / 100.0;
        let steps = (duty as usize).max(2);
        let ring: Vec<Pos2> = (0..=steps)
            .map(|i| {
                let a = -std::f32::consts::FRAC_PI_2 + sweep * i as f32 / steps as f32;
                c + Vec2::angled(a) * (r - 1.0)
            })
            .collect();
        p.add(Shape::line(ring, Stroke::new(3.0_f32, accent)));
    }
    // Blades: curved quads around the hub.
    let blade_col = if active {
        accent.gamma_multiply(0.85)
    } else {
        uc().text_secondary.gamma_multiply(0.5)
    };
    for i in 0..5 {
        let a = angle + TAU * i as f32 / 5.0;
        let hub = 11.0;
        let tip = r - 10.0;
        let pts = vec![
            c + Vec2::angled(a - 0.18) * hub,
            c + Vec2::angled(a - 0.05) * (tip * 0.6),
            c + Vec2::angled(a + 0.12) * tip,
            c + Vec2::angled(a + 0.45) * (tip * 0.75),
            c + Vec2::angled(a + 0.42) * hub,
        ];
        p.add(Shape::convex_polygon(pts, blade_col, Stroke::NONE));
    }
    p.circle_filled(c, 10.0, uc().bg_card);
    p.circle_stroke(c, 10.0, Stroke::new(1.5_f32, accent));
    resp.on_hover_text(format!("{duty}%"));
}

/// 270° temperature arc with colored zones, the curve thresholds as ticks and the value.
pub(super) fn draw_temp_gauge(ui: &mut Ui, temp: f32, thresholds: &[f32], label: &str) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(130.0, 110.0), Sense::hover());
    let p = ui.painter_at(rect);
    let c = Pos2::new(rect.center().x, rect.top() + 58.0);
    let r = 48.0;
    let start = std::f32::consts::PI * 0.75;
    let sweep = std::f32::consts::PI * 1.5;
    let at = |t: f32| start + sweep * ((t - T_MIN) / (T_MAX - T_MIN)).clamp(0.0, 1.0);
    let arc = |from: f32, to: f32, col: Color32, w: f32| {
        let steps = 24;
        let pts: Vec<Pos2> = (0..=steps)
            .map(|i| c + Vec2::angled(lerp(at(from), at(to), i as f32 / steps as f32)) * r)
            .collect();
        p.add(Shape::line(pts, Stroke::new(w, col)));
    };
    // Track with faint zones, then the filled value.
    arc(T_MIN, 55.0, uc().green.gamma_multiply(0.25), 8.0);
    arc(55.0, 70.0, uc().amber.gamma_multiply(0.25), 8.0);
    arc(70.0, T_MAX, uc().red.gamma_multiply(0.25), 8.0);
    arc(T_MIN, temp.clamp(T_MIN, T_MAX), temp_color(temp), 8.0);
    for &th in thresholds {
        let a = at(th);
        p.line_segment(
            [
                c + Vec2::angled(a) * (r - 9.0),
                c + Vec2::angled(a) * (r + 6.0),
            ],
            Stroke::new(1.5_f32, uc().text_secondary),
        );
    }
    p.text(
        c,
        Align2::CENTER_CENTER,
        format!("{temp:.1}°"),
        FontId::new(22.0, FontFamily::Proportional),
        uc().text_primary,
    );
    p.text(
        Pos2::new(c.x, rect.bottom() - 6.0),
        Align2::CENTER_BOTTOM,
        label,
        FontId::new(11.5, FontFamily::Proportional),
        uc().text_secondary,
    );
}

/// Clickable card with the preset name and a thumbnail of its curve.
fn profile_card(
    ui: &mut Ui,
    preset: FanPreset,
    name: &str,
    selected: bool,
    width: f32,
) -> Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(width, 78.0), Sense::click());
    let p = ui.painter_at(rect);
    let hovered = resp.hovered();
    let accent = uc().orange;
    let fill = if selected {
        accent.gamma_multiply(0.18)
    } else if hovered {
        uc().bg_card_hover
    } else {
        uc().bg_subgroup
    };
    p.rect_filled(rect, CornerRadius::same(8), fill);
    p.rect_stroke(
        rect,
        CornerRadius::same(8),
        Stroke::new(
            if selected { 2.0_f32 } else { 1.0_f32 },
            if selected { accent } else { uc().border_subtle },
        ),
        StrokeKind::Inside,
    );
    p.text(
        Pos2::new(rect.left() + 10.0, rect.top() + 14.0),
        Align2::LEFT_CENTER,
        name,
        FontId::new(12.5, FontFamily::Proportional),
        uc().text_primary,
    );
    // Thumbnail: step curve like argononed applies it.
    let plot = Rect::from_min_max(
        Pos2::new(rect.left() + 10.0, rect.top() + 28.0),
        Pos2::new(rect.right() - 10.0, rect.bottom() - 10.0),
    );
    let to = |t: f32, s: f32| {
        Pos2::new(
            plot.left() + ((t - T_MIN) / (T_MAX - T_MIN)).clamp(0.0, 1.0) * plot.width(),
            plot.bottom() - (s / 100.0) * plot.height(),
        )
    };
    let mut pts = preset.points();
    pts.sort_by(|a, b| {
        a.temp_c
            .partial_cmp(&b.temp_c)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut line = vec![to(T_MIN, 0.0)];
    let mut level = 0.0;
    for pt in &pts {
        line.push(to(pt.temp_c, level));
        level = pt.speed_pct as f32;
        line.push(to(pt.temp_c, level));
    }
    line.push(to(T_MAX, level));
    p.add(Shape::line(
        line,
        Stroke::new(
            2.0_f32,
            if selected {
                accent
            } else {
                uc().text_secondary
            },
        ),
    ));
    if hovered {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

/// Round knob: drag around it (or scroll over it) to choose 0..100 %.
fn speed_knob(ui: &mut Ui, value: &mut u8, hint: &str) {
    let size = 150.0;
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(size), Sense::click_and_drag());
    let p = ui.painter_at(rect);
    let c = rect.center();
    let r = size / 2.0 - 10.0;
    let start = std::f32::consts::PI * 0.75;
    let sweep = std::f32::consts::PI * 1.5;

    if let Some(pos) = resp.interact_pointer_pos() {
        if resp.dragged() || resp.clicked() {
            let v = pos - c;
            // Angle measured from the start of the arc, clockwise, in 0..TAU.
            let mut a = v.y.atan2(v.x) - start;
            while a < 0.0 {
                a += TAU;
            }
            if a <= sweep {
                *value = ((a / sweep) * 100.0).round() as u8;
            } else if a < sweep + (TAU - sweep) / 2.0 {
                *value = 100;
            } else {
                *value = 0;
            }
        }
    }
    if resp.hovered() {
        let scroll = ui.input(|i| i.smooth_scroll_delta.y);
        if scroll.abs() > 0.5 {
            let step = if scroll > 0.0 { 1i16 } else { -1 };
            *value = (*value as i16 + step).clamp(0, 100) as u8;
        }
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
    }

    let arc = |to: f32, col: Color32, w: f32| {
        let steps = 48;
        let pts: Vec<Pos2> = (0..=steps)
            .map(|i| c + Vec2::angled(start + sweep * to * i as f32 / steps as f32) * r)
            .collect();
        p.add(Shape::line(pts, Stroke::new(w, col)));
    };
    arc(1.0, uc().bg_subgroup, 12.0);
    let frac = *value as f32 / 100.0;
    if frac > 0.0 {
        arc(frac, uc().orange, 12.0);
    }
    let handle = c + Vec2::angled(start + sweep * frac) * r;
    p.circle_filled(handle, 9.0, uc().bg_card);
    p.circle_stroke(handle, 9.0, Stroke::new(2.5_f32, uc().orange));
    p.text(
        c,
        Align2::CENTER_CENTER,
        format!("{value}%"),
        FontId::new(30.0, FontFamily::Proportional),
        uc().text_primary,
    );
    p.text(
        c + Vec2::new(0.0, 26.0),
        Align2::CENTER_CENTER,
        hint,
        FontId::new(11.0, FontFamily::Proportional),
        uc().text_secondary,
    );
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CurveEdit {
    None,
    Moved,
    Added,
    Removed,
}

/// Curve editor: filled step curve, draggable points (double-click adds, right-click removes),
/// hover crosshair, the trail of recent (temperature, duty) samples and the live operating point.
fn draw_curve_editor(
    ui: &mut Ui,
    points: &mut Vec<FanCurvePoint>,
    dragging: &mut Option<usize>,
    cur_temp: f32,
    trail: &[(f32, f32)],
    pulse: bool,
    f: &crate::i18n::FanTexts,
) -> CurveEdit {
    let desired = Vec2::new(ui.available_width(), 290.0);
    let (rect, resp) = ui.allocate_exact_size(desired, Sense::click_and_drag());
    let p = ui.painter_at(rect);
    let mut edit = CurveEdit::None;

    p.rect_filled(rect, CornerRadius::same(8), uc().bg_subgroup);
    p.rect_stroke(
        rect,
        CornerRadius::same(8),
        Stroke::new(1.0_f32, uc().border_subtle),
        StrokeKind::Inside,
    );
    let plot = Rect::from_min_max(
        Pos2::new(rect.left() + 46.0, rect.top() + 22.0),
        Pos2::new(rect.right() - 18.0, rect.bottom() - 30.0),
    );
    let to_screen = |t: f32, s: f32| {
        Pos2::new(
            plot.left() + ((t - T_MIN) / (T_MAX - T_MIN)).clamp(0.0, 1.0) * plot.width(),
            plot.bottom() - (s / 100.0).clamp(0.0, 1.0) * plot.height(),
        )
    };
    let from_screen = |pos: Pos2| -> (f32, u8) {
        let tx = ((pos.x - plot.left()) / plot.width()).clamp(0.0, 1.0);
        let sy = ((plot.bottom() - pos.y) / plot.height()).clamp(0.0, 1.0);
        (
            (T_MIN + tx * (T_MAX - T_MIN)).round(),
            (sy * 100.0).round() as u8,
        )
    };

    // Grid.
    for s in [0, 25, 50, 75, 100] {
        let y = to_screen(T_MIN, s as f32).y;
        p.line_segment(
            [Pos2::new(plot.left(), y), Pos2::new(plot.right(), y)],
            Stroke::new(1.0_f32, chart_grid()),
        );
        p.text(
            Pos2::new(plot.left() - 6.0, y),
            Align2::RIGHT_CENTER,
            format!("{s}%"),
            FontId::new(11.0, FontFamily::Monospace),
            uc().text_secondary,
        );
    }
    for t in [30, 40, 50, 60, 70, 80, 90] {
        let x = to_screen(t as f32, 0.0).x;
        p.line_segment(
            [Pos2::new(x, plot.top()), Pos2::new(x, plot.bottom())],
            Stroke::new(1.0_f32, chart_grid()),
        );
        p.text(
            Pos2::new(x, plot.bottom() + 14.0),
            Align2::CENTER_CENTER,
            format!("{t}°C"),
            FontId::new(11.0, FontFamily::Monospace),
            uc().text_secondary,
        );
    }

    points.sort_by(|a, b| {
        a.temp_c
            .partial_cmp(&b.temp_c)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let hit_point = |pos: Pos2, points: &[FanCurvePoint]| {
        points
            .iter()
            .enumerate()
            .map(|(i, pt)| (i, to_screen(pt.temp_c, pt.speed_pct as f32).distance(pos)))
            .filter(|(_, d)| *d < 14.0)
            .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(i, _)| i)
    };

    // Interaction.
    let hover = resp.hover_pos();
    if let Some(pos) = resp.interact_pointer_pos() {
        if resp.drag_started() {
            *dragging = hit_point(pos, points);
        }
        if resp.dragged() {
            if let Some(i) = *dragging {
                if i < points.len() {
                    let (t, s) = from_screen(pos);
                    points[i].temp_c = t;
                    points[i].speed_pct = snap_speed(s);
                    edit = CurveEdit::Moved;
                }
            }
        }
    }
    if resp.drag_stopped() {
        *dragging = None;
    }
    if resp.double_clicked() {
        if let Some(pos) = hover {
            if plot.contains(pos) && hit_point(pos, points).is_none() {
                let (t, s) = from_screen(pos);
                points.push(FanCurvePoint {
                    temp_c: t,
                    speed_pct: snap_speed(s),
                });
                edit = CurveEdit::Added;
            }
        }
    }
    if resp.secondary_clicked() && points.len() > 1 {
        if let Some(i) = hover.and_then(|pos| hit_point(pos, points)) {
            points.remove(i);
            edit = CurveEdit::Removed;
        }
    }
    points.sort_by(|a, b| {
        a.temp_c
            .partial_cmp(&b.temp_c)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Step curve (argononed keeps a speed until the next threshold), with a soft fill.
    let mut line = vec![to_screen(T_MIN, 0.0)];
    let mut level = 0.0;
    for pt in points.iter() {
        line.push(to_screen(pt.temp_c, level));
        level = pt.speed_pct as f32;
        line.push(to_screen(pt.temp_c, level));
    }
    line.push(to_screen(T_MAX, level));
    let accent = uc().orange;
    for w in line.windows(2) {
        // Fill each segment down to 0 % as a quad (convex), so the area follows the steps.
        let quad = vec![
            w[0],
            w[1],
            Pos2::new(w[1].x, plot.bottom()),
            Pos2::new(w[0].x, plot.bottom()),
        ];
        if (w[1].x - w[0].x).abs() > 0.5 {
            p.add(Shape::convex_polygon(
                quad,
                accent.gamma_multiply(0.12),
                Stroke::NONE,
            ));
        }
    }
    p.add(Shape::line(line, Stroke::new(2.5_f32, accent)));

    // Trail of recent samples (oldest faint).
    let n = trail.len().max(1);
    for (i, (t, d)) in trail.iter().enumerate() {
        let alpha = 0.08 + 0.5 * i as f32 / n as f32;
        p.circle_filled(to_screen(*t, *d), 2.5, uc().cyan.gamma_multiply(alpha));
    }

    // Points.
    let hovered_point = hover.and_then(|pos| hit_point(pos, points));
    for (i, pt) in points.iter().enumerate() {
        let sp = to_screen(pt.temp_c, pt.speed_pct as f32);
        let active = *dragging == Some(i) || hovered_point == Some(i);
        let r = if active { 8.5 } else { 6.5 };
        p.circle_filled(sp, r, uc().bg_card);
        p.circle_stroke(sp, r, Stroke::new(2.5_f32, accent));
        if active {
            p.text(
                sp + Vec2::new(0.0, -16.0),
                Align2::CENTER_CENTER,
                format!("{:.0}°C · {}%", pt.temp_c, pt.speed_pct),
                FontId::new(12.0, FontFamily::Proportional),
                uc().text_primary,
            );
        }
    }

    // Live operating point with a pulse.
    let now_duty = evaluate_fan_speed(cur_temp, points) as f32;
    let op = to_screen(cur_temp, now_duty);
    p.line_segment(
        [Pos2::new(op.x, plot.top()), Pos2::new(op.x, plot.bottom())],
        Stroke::new(1.0_f32, uc().cyan.gamma_multiply(0.5)),
    );
    if pulse {
        let phase = (ui.input(|i| i.time) as f32 * 2.0).fract();
        p.circle_stroke(
            op,
            6.0 + phase * 10.0,
            Stroke::new(2.0_f32, uc().cyan.gamma_multiply(1.0 - phase)),
        );
    }
    p.circle_filled(op, 6.0, uc().cyan);
    p.text(
        op + Vec2::new(10.0, 12.0),
        Align2::LEFT_CENTER,
        format!("{} {cur_temp:.0}°C · {now_duty:.0}%", f.now),
        FontId::new(11.5, FontFamily::Proportional),
        uc().cyan,
    );

    // Hover crosshair with the value the curve gives at that temperature.
    if let Some(pos) = hover {
        if plot.contains(pos) && dragging.is_none() && hovered_point.is_none() {
            let (t, _) = from_screen(pos);
            let d = evaluate_fan_speed(t, points) as f32;
            let at = to_screen(t, d);
            p.line_segment(
                [Pos2::new(at.x, plot.top()), Pos2::new(at.x, plot.bottom())],
                Stroke::new(1.0_f32, uc().text_secondary.gamma_multiply(0.5)),
            );
            p.circle_filled(at, 4.0, uc().text_primary);
            p.text(
                at + Vec2::new(8.0, -12.0),
                Align2::LEFT_CENTER,
                format!("{t:.0}°C ➡ {d:.0}%"),
                FontId::new(11.5, FontFamily::Proportional),
                uc().text_primary,
            );
        }
        if hovered_point.is_some() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
        }
    }
    edit
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pt(t: f32, s: u8) -> FanCurvePoint {
        FanCurvePoint {
            temp_c: t,
            speed_pct: s,
        }
    }

    #[test]
    fn counts_changed_points() {
        let saved = vec![pt(40.0, 30), pt(55.0, 60)];
        assert_eq!(changed_points(&saved, &saved), 0);
        assert_eq!(changed_points(&[pt(40.0, 35), pt(55.0, 60)], &saved), 1);
        assert_eq!(
            changed_points(&[pt(40.0, 30), pt(55.0, 60), pt(70.0, 100)], &saved),
            1
        );
        assert_eq!(changed_points(&[pt(55.0, 60), pt(40.0, 30)], &saved), 0);
    }

    #[test]
    fn dragged_speed_snaps_like_argononed() {
        assert_eq!(snap_speed(10), 0);
        assert_eq!(snap_speed(18), 25);
        assert_eq!(snap_speed(60), 60);
    }
}
