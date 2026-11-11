//! Overclock page: live strip (clock, temperature, core voltage, firmware flags), profile
//! cards for the detected board, the safe-overclock steps, live and firmware settings, and the
//! stability test. The safety rules (power-supply guard, trial boot, stability) are unchanged.

use super::*;
use crate::board::OcProfile;
use crate::stability::Verdict;
use crate::tryboot::Status;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StepState {
    Done,
    Current,
    Pending,
    Warn,
    Failed,
}

impl StepState {
    fn color(self) -> Color32 {
        match self {
            StepState::Done => uc().green,
            StepState::Current => uc().cyan,
            StepState::Pending => uc().text_secondary,
            StepState::Warn => uc().amber,
            StepState::Failed => uc().red,
        }
    }
}

/// Clock, GPU clock and voltage a preset sets on this board.
fn preset_values(p: OcPreset, prof: &OcProfile) -> (u32, u32, i32) {
    match p {
        OcPreset::EcoPowerSave => (prof.eco_mhz, prof.gpu_stock_mhz, 0),
        OcPreset::StockBoost => (prof.stock_mhz, prof.gpu_stock_mhz, 0),
        OcPreset::OcGaming => (
            prof.oc_mhz,
            prof.gpu_oc_mhz,
            if prof.uses_ov_delta {
                prof.ov_delta_oc_uv
            } else {
                prof.ov_oc
            },
        ),
        OcPreset::OcExtreme | OcPreset::Custom => (
            prof.extreme_mhz,
            prof.gpu_extreme_mhz,
            if prof.uses_ov_delta {
                prof.ov_delta_extreme_uv
            } else {
                prof.ov_extreme
            },
        ),
    }
}

impl ArgonControlCenterApp {
    pub(super) fn draw_overclocking_tab(&mut self, ui: &mut Ui) {
        self.draw_power_guard_card(ui);
        self.oc_live_strip(ui);
        ui.add_space(10.0);
        self.oc_safety_steps(ui);
        self.draw_trial_card(ui);
        self.oc_profile_cards(ui);
        ui.add_space(12.0);
        responsive_two_columns(
            ui,
            self,
            |app, ui| app.draw_runtime_cpu_card(ui),
            |app, ui| app.draw_firmware_oc_card(ui),
        );
        ui.add_space(12.0);
        self.draw_stability_card(ui);
    }

    /// Applies a preset to the firmware settings and the matching live CPU settings.
    fn select_oc_preset(&mut self, p: OcPreset) {
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

    // ------------------------------------------------------------------ live strip

    fn oc_live_strip(&mut self, ui: &mut Ui) {
        let o = texts(self.lang).oc;
        let prof = crate::board::current().oc;
        let tel = &self.telemetry;
        let cur = tel.cpu_cur_freq_mhz as f32;
        let scale_max = prof
            .extreme_mhz
            .max(tel.cpu_hw_max_mhz)
            .max(self.boot_oc.arm_freq_mhz) as f32;
        let flags = tel.firmware.throttled;
        let capped = flags
            .map(|f| f.freq_capped_now() || f.throttled_now())
            .unwrap_or(false);
        let volts = tel.firmware.core_volts;
        let temp = tel.cpu_temp_c;
        let governor = tel.cpu_governor.clone();
        let (fmin, fmax) = (tel.cpu_min_freq_mhz, tel.cpu_max_freq_mhz);
        let temp_limit = self.boot_oc.temp_limit_c as f32;
        let ticks = [
            prof.stock_mhz as f32 / scale_max,
            self.boot_oc.arm_freq_mhz as f32 / scale_max,
        ];

        aorus_card_with_tip(ui, o.live, o.live_tip, uc().cyan, |ui| {
            ui.horizontal_wrapped(|ui| {
                // Spread the four items over the width when they fit; otherwise wrap.
                let gap = ((ui.available_width() - 6e+02) / 4.0).max(28.0);
                let half = gap / 2.0;
                ui.spacing_mut().item_spacing = Vec2::new(0.0, 10.0);
                ui.add_space(half);
                arc_gauge(
                    ui,
                    cur / scale_max,
                    &format!("{cur:.0}"),
                    "MHz",
                    o.cpu_clock,
                    if capped { uc().amber } else { uc().cyan },
                    &ticks,
                );
                ui.add_space(gap);
                super::fan::draw_temp_gauge(ui, temp, &[temp_limit], o.temperature);

                // Voltage and governor.
                ui.add_space(gap);
                let (rect, _) = ui.allocate_exact_size(Vec2::new(170.0, 110.0), Sense::hover());
                let p = ui.painter_at(rect);
                p.text(
                    Pos2::new(rect.left(), rect.top() + 32.0),
                    Align2::LEFT_CENTER,
                    volts
                        .map(|v| format!("{v:.3} V"))
                        .unwrap_or_else(|| "— V".into()),
                    FontId::new(28.0, FontFamily::Proportional),
                    uc().text_primary,
                );
                p.text(
                    Pos2::new(rect.left(), rect.top() + 60.0),
                    Align2::LEFT_CENTER,
                    o.core_voltage,
                    FontId::new(12.0, FontFamily::Proportional),
                    uc().text_secondary,
                );
                p.text(
                    Pos2::new(rect.left(), rect.top() + 86.0),
                    Align2::LEFT_CENTER,
                    format!("{governor} · {fmin}-{fmax} MHz"),
                    FontId::new(12.5, FontFamily::Proportional),
                    uc().heading,
                );

                // Firmware flags: now (red), since boot (amber), never (green).
                ui.add_space(gap);
                let (rect, resp) = ui.allocate_exact_size(Vec2::new(210.0, 110.0), Sense::hover());
                let p = ui.painter_at(rect);
                let rows: [(&str, Option<(bool, bool)>); 4] = [
                    (
                        o.flag_power,
                        flags.map(|f| (f.undervoltage_now(), f.undervoltage_since_boot())),
                    ),
                    (
                        o.flag_capped,
                        flags.map(|f| (f.freq_capped_now(), f.freq_capped_since_boot())),
                    ),
                    (
                        o.flag_throttled,
                        flags.map(|f| (f.throttled_now(), f.throttled_since_boot())),
                    ),
                    (
                        o.flag_temp,
                        flags.map(|f| (f.soft_temp_limit_now(), f.soft_temp_limit_since_boot())),
                    ),
                ];
                for (i, (name, state)) in rows.iter().enumerate() {
                    let y = rect.top() + 16.0 + i as f32 * 26.0;
                    let (col, value) = match state {
                        Some((true, _)) => (uc().red, o.state_now),
                        Some((false, true)) => (uc().amber, o.state_since_boot),
                        Some((false, false)) => (uc().green, o.state_ok),
                        None => (uc().text_secondary, "—"),
                    };
                    p.circle_filled(Pos2::new(rect.left() + 6.0, y), 5.0, col);
                    p.text(
                        Pos2::new(rect.left() + 18.0, y),
                        Align2::LEFT_CENTER,
                        *name,
                        FontId::new(13.0, FontFamily::Proportional),
                        uc().text_primary,
                    );
                    p.text(
                        Pos2::new(rect.left() + 128.0, y),
                        Align2::LEFT_CENTER,
                        value,
                        FontId::new(12.5, FontFamily::Proportional),
                        uc().text_secondary,
                    );
                }
                resp.on_hover_text("vcgencmd get_throttled");
            });
        });
    }

    // ------------------------------------------------------------------ safe overclock steps

    fn oc_safety_steps(&mut self, ui: &mut Ui) {
        let o = texts(self.lang).oc;
        let prof = crate::board::current().oc;
        let risky = self.boot_oc.exceeds_stock(&prof);
        let status = self.trial_status.clone();
        if !risky && status == Status::Idle {
            return;
        }
        let power = if !self.power_problem() {
            (StepState::Done, o.step_power_ok)
        } else if self.power_override {
            (StepState::Warn, o.step_power_override)
        } else {
            (StepState::Failed, o.step_power_blocked)
        };
        let supported = crate::tryboot::supported();
        let trial = match &status {
            _ if !supported => (StepState::Warn, o.step_trial_unsupported),
            Status::Idle => (StepState::Current, o.step_trial_prepare),
            Status::Ready(_) => (StepState::Current, o.step_trial_reboot),
            Status::InTrial(_) => (StepState::Done, o.step_trial_running),
            Status::Failed(_) | Status::NotKept(_) => (StepState::Failed, o.step_trial_failed),
        };
        let in_trial = matches!(status, Status::InTrial(_));
        let running = self.stability.as_ref().is_some_and(|s| s.is_running());
        let verdict = self.stability.as_ref().and_then(|s| s.snapshot().verdict);
        let stability = if !in_trial {
            (StepState::Pending, o.step_stab_after_trial)
        } else if running {
            (StepState::Current, o.step_stab_running)
        } else {
            match verdict {
                Some(Verdict::Stable) => (StepState::Done, o.step_stab_stable),
                Some(_) => (StepState::Failed, o.step_stab_failed),
                None => (StepState::Current, o.step_stab_start),
            }
        };
        let keep = if in_trial && verdict == Some(Verdict::Stable) {
            (StepState::Current, o.step_keep_ready)
        } else {
            (StepState::Pending, o.step_keep_wait)
        };
        let power_state = power.0;
        let steps = [
            (o.step_power, power),
            (
                o.step_trial,
                if power_state == StepState::Failed {
                    (StepState::Pending, trial.1)
                } else {
                    trial
                },
            ),
            (o.step_stability, stability),
            (o.step_keep, keep),
        ];

        aorus_card_with_tip(ui, o.steps_title, o.steps_tip, uc().orange, |ui| {
            let width = ui.available_width();
            let step_w = width / steps.len() as f32;
            let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 84.0), Sense::hover());
            let p = ui.painter_at(rect);
            let cy = rect.top() + 20.0;
            for (i, (name, (state, detail))) in steps.iter().enumerate() {
                let cx = rect.left() + step_w * (i as f32 + 0.5);
                if i + 1 < steps.len() {
                    let next_x = rect.left() + step_w * (i as f32 + 1.5);
                    let col = if *state == StepState::Done {
                        uc().green
                    } else {
                        uc().border_subtle
                    };
                    p.line_segment(
                        [Pos2::new(cx + 16.0, cy), Pos2::new(next_x - 16.0, cy)],
                        Stroke::new(2.0_f32, col),
                    );
                }
                let col = state.color();
                let filled = matches!(state, StepState::Done | StepState::Failed);
                if filled {
                    p.circle_filled(Pos2::new(cx, cy), 13.0, col);
                } else {
                    p.circle_filled(Pos2::new(cx, cy), 13.0, uc().bg_card);
                    p.circle_stroke(Pos2::new(cx, cy), 13.0, Stroke::new(2.0_f32, col));
                }
                let mark = match state {
                    StepState::Done => "✔".to_string(),
                    StepState::Failed => "✖".to_string(),
                    _ => (i + 1).to_string(),
                };
                p.text(
                    Pos2::new(cx, cy),
                    Align2::CENTER_CENTER,
                    mark,
                    FontId::new(13.0, FontFamily::Proportional),
                    if filled { Color32::WHITE } else { col },
                );
                p.text(
                    Pos2::new(cx, cy + 26.0),
                    Align2::CENTER_CENTER,
                    *name,
                    FontId::new(12.5, FontFamily::Proportional),
                    uc().text_primary,
                );
                let detail_galley = p.layout(
                    detail.to_string(),
                    FontId::new(11.0, FontFamily::Proportional),
                    if *state == StepState::Pending {
                        uc().text_secondary
                    } else {
                        col
                    },
                    step_w - 12.0,
                );
                let pos = Pos2::new(cx - detail_galley.size().x / 2.0, cy + 36.0);
                p.galley(pos, detail_galley, Color32::PLACEHOLDER);
            }
        });
        ui.add_space(10.0);
    }

    // ------------------------------------------------------------------ profile cards

    fn oc_profile_cards(&mut self, ui: &mut Ui) {
        let t = texts(self.lang);
        let o = t.oc;
        let prof = crate::board::current().oc;
        let blocked = self.power_blocks_overclock();
        let lang = self.lang;
        let mut picked: Option<OcPreset> = None;
        aorus_card(ui, t.card_oc_presets, uc().orange, |ui| {
            let presets = [
                (OcPreset::EcoPowerSave, t.oc_preset_eco_tip),
                (OcPreset::StockBoost, t.oc_preset_stock_tip),
                (OcPreset::OcGaming, t.oc_preset_gaming_tip),
                (OcPreset::OcExtreme, t.oc_preset_extreme_tip),
            ];
            let gap = 10.0;
            let per_row = if ui.available_width() >= 4.0 * 150.0 + 3.0 * gap {
                4
            } else {
                2
            };
            let card_w = (ui.available_width() - gap * (per_row as f32 - 1.0)) / per_row as f32;
            for row in presets.chunks(per_row) {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = gap;
                    for &(preset, tip) in row {
                        let (arm, gpu, ov) = preset_values(preset, &prof);
                        let above = arm > prof.stock_mhz;
                        let badge = match preset {
                            OcPreset::StockBoost => Some((o.badge_factory, uc().green)),
                            _ if above && blocked => Some((o.badge_blocked, uc().red)),
                            _ if above => Some((o.badge_trial, uc().amber)),
                            _ => None,
                        };
                        let volt = if prof.uses_ov_delta {
                            format!("+{} mV", ov / 1000)
                        } else {
                            format!("over_voltage {ov}")
                        };
                        let r = oc_card(
                            ui,
                            card_w,
                            &preset.label(lang),
                            &format!("{arm} MHz"),
                            &format!("GPU {gpu} MHz · {volt}"),
                            arm as f32 / prof.extreme_mhz.max(1) as f32,
                            prof.stock_mhz as f32 / prof.extreme_mhz.max(1) as f32,
                            self.selected_oc_preset == preset,
                            badge,
                        );
                        if r.clicked() {
                            picked = Some(preset);
                        }
                        aorus_tooltip(r, &preset.label(lang), tip, Some(get_boot_config_path()));
                    }
                });
                ui.add_space(gap);
            }
            if self.selected_oc_preset == OcPreset::Custom {
                ui.label(
                    RichText::new(o.custom_note)
                        .size(12.0)
                        .color(uc().text_secondary),
                );
            }
        });
        if let Some(p) = picked {
            self.select_oc_preset(p);
        }
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
                let (text, _) = crate::gpu::throttle_summary(th, self.lang);
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
}

/// 270° gauge: value 0..1, big text with a unit, a label below and optional ticks (0..1).
fn arc_gauge(
    ui: &mut Ui,
    frac: f32,
    value: &str,
    unit: &str,
    label: &str,
    color: Color32,
    ticks: &[f32],
) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(130.0, 110.0), Sense::hover());
    let p = ui.painter_at(rect);
    let c = Pos2::new(rect.center().x, rect.top() + 58.0);
    let r = 48.0;
    let start = std::f32::consts::PI * 0.75;
    let sweep = std::f32::consts::PI * 1.5;
    let arc = |to: f32, col: Color32| {
        let steps = 32;
        let pts: Vec<Pos2> = (0..=steps)
            .map(|i| c + Vec2::angled(start + sweep * to * i as f32 / steps as f32) * r)
            .collect();
        p.add(Shape::line(pts, Stroke::new(8.0_f32, col)));
    };
    arc(1.0, uc().bg_subgroup);
    if frac > 0.0 {
        arc(frac.clamp(0.0, 1.0), color);
    }
    for &tk in ticks {
        let a = start + sweep * tk.clamp(0.0, 1.0);
        p.line_segment(
            [
                c + Vec2::angled(a) * (r - 9.0),
                c + Vec2::angled(a) * (r + 6.0),
            ],
            Stroke::new(1.5_f32, uc().text_secondary),
        );
    }
    p.text(
        c + Vec2::new(0.0, -4.0),
        Align2::CENTER_CENTER,
        value,
        FontId::new(22.0, FontFamily::Proportional),
        uc().text_primary,
    );
    p.text(
        c + Vec2::new(0.0, 16.0),
        Align2::CENTER_CENTER,
        unit,
        FontId::new(11.0, FontFamily::Proportional),
        uc().text_secondary,
    );
    p.text(
        Pos2::new(c.x, rect.bottom() - 6.0),
        Align2::CENTER_BOTTOM,
        label,
        FontId::new(11.5, FontFamily::Proportional),
        uc().text_secondary,
    );
}

/// Profile card: name, clock, details, a bar against the board's maximum with a factory mark,
/// and an optional badge.
#[allow(clippy::too_many_arguments)]
fn oc_card(
    ui: &mut Ui,
    width: f32,
    name: &str,
    clock: &str,
    detail: &str,
    frac: f32,
    stock_frac: f32,
    selected: bool,
    badge: Option<(&str, Color32)>,
) -> Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(width, 104.0), Sense::click());
    let p = ui.painter_at(rect);
    let accent = uc().orange;
    let fill = if selected {
        accent.gamma_multiply(0.18)
    } else if resp.hovered() {
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
    let left = rect.left() + 12.0;
    p.text(
        Pos2::new(left, rect.top() + 16.0),
        Align2::LEFT_CENTER,
        name,
        FontId::new(12.5, FontFamily::Proportional),
        uc().text_secondary,
    );
    if let Some((text, col)) = badge {
        let font = FontId::new(10.5, FontFamily::Proportional);
        let g = p.layout_no_wrap(text.to_string(), font, col);
        let w = g.size().x + 10.0;
        let b = Rect::from_min_size(
            Pos2::new(rect.right() - 10.0 - w, rect.top() + 8.0),
            Vec2::new(w, 16.0),
        );
        p.rect_filled(b, CornerRadius::same(8), col.gamma_multiply(0.16));
        p.galley(
            Pos2::new(b.left() + 5.0, b.center().y - g.size().y / 2.0),
            g,
            Color32::PLACEHOLDER,
        );
    }
    p.text(
        Pos2::new(left, rect.top() + 42.0),
        Align2::LEFT_CENTER,
        clock,
        FontId::new(22.0, FontFamily::Proportional),
        uc().text_primary,
    );
    p.text(
        Pos2::new(left, rect.top() + 66.0),
        Align2::LEFT_CENTER,
        detail,
        FontId::new(11.0, FontFamily::Proportional),
        uc().text_secondary,
    );
    let bar = Rect::from_min_size(
        Pos2::new(left, rect.bottom() - 18.0),
        Vec2::new(rect.width() - 24.0, 6.0),
    );
    p.rect_filled(bar, CornerRadius::same(3), uc().bg_card);
    let filled = Rect::from_min_size(
        bar.min,
        Vec2::new(bar.width() * frac.clamp(0.0, 1.0), bar.height()),
    );
    let col = if frac > stock_frac + 0.001 {
        uc().amber
    } else {
        uc().green
    };
    p.rect_filled(filled, CornerRadius::same(3), col);
    let sx = bar.left() + bar.width() * stock_frac.clamp(0.0, 1.0);
    p.line_segment(
        [
            Pos2::new(sx, bar.top() - 3.0),
            Pos2::new(sx, bar.bottom() + 3.0),
        ],
        Stroke::new(1.5_f32, uc().text_secondary),
    );
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preset_values_follow_the_board_profile() {
        let prof = OcProfile::for_soc(crate::board::Soc::Bcm2711);
        assert_eq!(preset_values(OcPreset::StockBoost, &prof).0, prof.stock_mhz);
        assert_eq!(
            preset_values(OcPreset::OcExtreme, &prof).0,
            prof.extreme_mhz
        );
        assert!(preset_values(OcPreset::EcoPowerSave, &prof).0 < prof.stock_mhz);
    }
}
