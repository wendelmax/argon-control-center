//! Control tabs of the terminal interface: Fan, CPU, GPIO and Services.
//!
//! They follow the same rules as the graphical interface: firmware settings above the factory
//! clock need a healthy power supply, a trial boot and a stable test before they are kept;
//! only GPIO pins that are already outputs are driven; every change is confirmed first.

use super::{boxed, kv, meter, tr, AMBER, BLUE, DIM, GREEN, RED};
use crate::argon::{evaluate_fan_speed, load_fan_curve, FanCurvePoint, FanPreset, CPU_FAN_CONF};
use crate::gpio_lab::{HeaderPinKind, RPI_40PIN_HEADER};
use crate::i18n::Language;
use crate::overclock::{BootOcConfig, OcPreset};
use crate::snapshot::Snapshot;
use crate::stability::{StabilityTest, Verdict};
use crate::tryboot::Status;
use ratatui::crossterm::event::KeyCode;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Bar, BarChart, BarGroup, Paragraph, Wrap};
use ratatui::Frame;
use std::fs;
use std::time::Duration;

const FAN_PRESETS: [FanPreset; 4] = [
    FanPreset::Silent,
    FanPreset::BalancedDefault,
    FanPreset::Performance,
    FanPreset::AlwaysOnTurbo,
];
const OC_PRESETS: [OcPreset; 4] = [
    OcPreset::EcoPowerSave,
    OcPreset::StockBoost,
    OcPreset::OcGaming,
    OcPreset::OcExtreme,
];
const STABILITY_MINUTES: u64 = 3;

/// A change the user confirms before it runs.
#[derive(Debug, Clone)]
pub enum Action {
    FanSave(Vec<FanCurvePoint>),
    FanManual(u8),
    FanAuto,
    EnableI2c,
    CpuLive {
        governor: String,
        min: u32,
        max: u32,
    },
    FirmwareSave(Box<BootOcConfig>),
    TrialPrepare(Box<BootOcConfig>),
    TrialReboot,
    TrialKeep,
    TrialDiscard,
    GpioDrive {
        bcm: u8,
        high: bool,
        direct: bool,
    },
    Service(&'static str),
}

impl Action {
    /// Text of the confirmation window.
    pub fn describe(&self, lang: Language) -> String {
        match self {
            Action::FanSave(_) => tr(
                lang,
                "Save the fan curve to /etc/argononed.conf and restart argononed?",
                "Salvar a curva em /etc/argononed.conf e reiniciar o argononed?",
            )
            .into(),
            Action::FanManual(p) => format!(
                "{} {p}%?",
                tr(lang, "Set the fan to a fixed speed of", "Fixar a ventoinha em")
            ),
            Action::FanAuto => tr(
                lang,
                "Return to the automatic curve (restart argononed)?",
                "Voltar à curva automática (reiniciar o argononed)?",
            )
            .into(),
            Action::EnableI2c => tr(
                lang,
                "Enable I2C now and in config.txt (backup kept) and restart argononed?",
                "Ativar o I2C agora e no config.txt (com backup) e reiniciar o argononed?",
            )
            .into(),
            Action::CpuLive { governor, min, max } => format!(
                "{} {governor}, {min}-{max} MHz? {}",
                tr(lang, "Apply governor", "Aplicar governor"),
                tr(lang, "(reset at reboot)", "(volta ao padrão no reboot)")
            ),
            Action::FirmwareSave(cfg) => format!(
                "{} {}. {}",
                tr(lang, "Save to config.txt:", "Salvar no config.txt:"),
                crate::tryboot::summarize(cfg),
                tr(lang, "Applies at the next boot.", "Vale no próximo boot.")
            ),
            Action::TrialPrepare(cfg) => format!(
                "{} {}. {}",
                tr(lang, "Prepare a trial boot:", "Preparar boot de teste:"),
                crate::tryboot::summarize(cfg),
                tr(
                    lang,
                    "config.txt is not changed; reboot into the trial next.",
                    "o config.txt não muda; depois reinicie no teste."
                )
            ),
            Action::TrialReboot => tr(
                lang,
                "Reboot now into the trial settings? If the Pi hangs, switch it off and on to return to the previous settings.",
                "Reiniciar agora nos ajustes de teste? Se o Pi travar, desligue e ligue para voltar aos ajustes anteriores.",
            )
            .into(),
            Action::TrialKeep => tr(
                lang,
                "Keep the trial settings in config.txt?",
                "Manter os ajustes de teste no config.txt?",
            )
            .into(),
            Action::TrialDiscard => tr(lang, "Discard the trial settings?", "Descartar os ajustes de teste?").into(),
            Action::GpioDrive { bcm, high, .. } => format!(
                "{} GPIO{bcm} {}?",
                tr(lang, "Drive", "Colocar"),
                if *high { "HIGH" } else { "LOW" }
            ),
            Action::Service(a) => format!("systemctl {a} argononed.service?"),
        }
    }

    /// Whether the action may ask for the password (the terminal is handed over to pkexec).
    pub fn needs_root(&self) -> bool {
        !matches!(self, Action::GpioDrive { direct: true, .. })
    }

    /// Runs the action. Returns a message for the status line.
    pub fn run(self, st: &mut State) -> Result<String, String> {
        match self {
            Action::FanSave(pts) => {
                let res = crate::argon::save_fan_curve(CPU_FAN_CONF, &pts, "CPU");
                if res.is_ok() || res.as_ref().is_err_and(|e| e.contains(" saved, but ")) {
                    st.saved_curve = pts;
                    st.manual_active = false;
                }
                res
            }
            Action::FanManual(p) => {
                let res = crate::argon::set_immediate_fan_speed(p);
                if res.is_ok() {
                    st.manual_active = true;
                }
                res
            }
            Action::FanAuto => {
                st.manual_active = false;
                crate::argon::control_argononed_service("restart")
            }
            Action::EnableI2c => crate::argon::enable_fan_bus(),
            Action::CpuLive { governor, min, max } => {
                let boost = st.boost;
                crate::overclock::apply_runtime_cpufreq(&governor, min, max, boost)
            }
            Action::FirmwareSave(cfg) => {
                let res = crate::overclock::save_boot_config_scoped(
                    &cfg,
                    crate::overclock::ConfigScope::Overclock,
                );
                if res.is_ok() {
                    st.boot = *cfg;
                }
                res
            }
            Action::TrialPrepare(cfg) => {
                let res = crate::tryboot::prepare(&cfg);
                st.trial = crate::tryboot::status();
                res
            }
            Action::TrialReboot => crate::tryboot::reboot_into_trial(),
            Action::TrialKeep => {
                let res = crate::tryboot::keep();
                if res.is_ok() {
                    st.boot = crate::overclock::load_boot_config();
                }
                st.trial = crate::tryboot::status();
                res
            }
            Action::TrialDiscard => {
                let res = crate::tryboot::discard();
                st.trial = crate::tryboot::status();
                res
            }
            Action::GpioDrive { bcm, high, direct } => {
                if direct {
                    crate::gpio_lab::drive_output_cli(&format!("{bcm}={}", u8::from(high)))
                } else {
                    let level = if high { "dh" } else { "dl" };
                    crate::privileged::run_root_script(&format!("pinctrl set {bcm} op {level}"))
                        .map(|_| format!("GPIO{bcm} {}", if high { "HIGH" } else { "LOW" }))
                }
            }
            Action::Service(a) => crate::argon::control_argononed_service(a),
        }
    }
}

/// What a key press asks the shell to do.
pub enum Outcome {
    None,
    Confirm(Action),
    Message(String, bool),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Row {
    FanPreset(usize),
    FanSave,
    FanManual,
    FanAuto,
    EnableI2c,
    CpuGovernor,
    CpuMin,
    CpuMax,
    CpuApply,
    CpuProfile(usize),
    Stability,
    TrialReboot,
    TrialKeep,
    TrialDiscard,
    Svc(&'static str),
}

/// State of the control tabs.
pub struct State {
    pub curve: Vec<FanCurvePoint>,
    pub saved_curve: Vec<FanCurvePoint>,
    pub manual: u8,
    pub manual_active: bool,
    pub governors: Vec<String>,
    pub governor: String,
    pub min_mhz: u32,
    pub max_mhz: u32,
    pub hw_min: u32,
    pub hw_max: u32,
    pub boost: bool,
    pub boot: BootOcConfig,
    pub trial: Status,
    pub stability: Option<StabilityTest>,
    pub gpio_sel: usize,
    pub service_enabled: Option<bool>,
}

fn read_sys(path: &str) -> Option<String> {
    fs::read_to_string(path).ok().map(|s| s.trim().to_string())
}

fn read_khz(path: &str) -> Option<u32> {
    read_sys(path)?.parse::<u32>().ok().map(|k| k / 1000)
}

fn service_enabled() -> Option<bool> {
    let out = std::process::Command::new("systemctl")
        .args(["is-enabled", "argononed.service"])
        .output()
        .ok()?;
    Some(String::from_utf8_lossy(&out.stdout).trim() == "enabled")
}

impl State {
    pub fn load() -> Self {
        const CPU: &str = "/sys/devices/system/cpu/cpu0/cpufreq";
        let curve = load_fan_curve(CPU_FAN_CONF, FanPreset::BalancedDefault.points());
        let hw_min = read_khz(&format!("{CPU}/cpuinfo_min_freq")).unwrap_or(600);
        let hw_max = read_khz(&format!("{CPU}/cpuinfo_max_freq")).unwrap_or(1800);
        State {
            saved_curve: curve.clone(),
            curve,
            manual: 50,
            manual_active: false,
            governors: read_sys(&format!("{CPU}/scaling_available_governors"))
                .map(|s| s.split_whitespace().map(String::from).collect())
                .unwrap_or_default(),
            governor: read_sys(&format!("{CPU}/scaling_governor")).unwrap_or_default(),
            min_mhz: read_khz(&format!("{CPU}/scaling_min_freq")).unwrap_or(hw_min),
            max_mhz: read_khz(&format!("{CPU}/scaling_max_freq")).unwrap_or(hw_max),
            hw_min,
            hw_max,
            boost: read_sys("/sys/devices/system/cpu/cpufreq/boost").as_deref() != Some("0"),
            boot: crate::overclock::load_boot_config(),
            trial: crate::tryboot::status(),
            stability: None,
            gpio_sel: 10, // physical pin 11, GPIO17
            service_enabled: service_enabled(),
        }
    }

    /// Called once per second.
    pub fn tick(&mut self) {
        self.trial = crate::tryboot::status();
    }

    pub fn after_action(&mut self) {
        self.service_enabled = service_enabled();
        self.trial = crate::tryboot::status();
    }

    fn unsaved(&self) -> bool {
        self.curve != self.saved_curve
    }

    fn stable(&self) -> bool {
        self.stability
            .as_ref()
            .and_then(|s| s.snapshot().verdict)
            .is_some_and(|v| v == Verdict::Stable)
    }

    fn rows(&self, tab: super::Tab, snap: &Snapshot) -> Vec<Row> {
        use super::Tab;
        match tab {
            Tab::Fan => {
                let mut r: Vec<Row> = (0..FAN_PRESETS.len()).map(Row::FanPreset).collect();
                r.extend([Row::FanSave, Row::FanManual]);
                if self.manual_active {
                    r.push(Row::FanAuto);
                }
                if !snap.fan_bus {
                    r.insert(0, Row::EnableI2c);
                }
                r
            }
            Tab::Cpu => {
                let mut r = vec![Row::CpuGovernor, Row::CpuMin, Row::CpuMax, Row::CpuApply];
                r.extend((0..OC_PRESETS.len()).map(Row::CpuProfile));
                r.push(Row::Stability);
                match self.trial {
                    Status::Ready(_) | Status::NotKept(_) => {
                        r.extend([Row::TrialReboot, Row::TrialDiscard])
                    }
                    Status::InTrial(_) => r.extend([Row::TrialKeep, Row::TrialDiscard]),
                    Status::Failed(_) => r.push(Row::TrialDiscard),
                    Status::Idle => {}
                }
                r
            }
            Tab::Services => ["start", "stop", "restart", "enable", "disable"]
                .into_iter()
                .map(Row::Svc)
                .collect(),
            _ => Vec::new(),
        }
    }

    /// Handles a key on a control tab. `sel` is the selected row (or pin on the GPIO tab).
    pub fn key(
        &mut self,
        tab: super::Tab,
        sel: &mut usize,
        code: KeyCode,
        snap: &Snapshot,
        lang: Language,
    ) -> Outcome {
        if tab == super::Tab::Gpio {
            return self.gpio_key(code, snap, lang);
        }
        let rows = self.rows(tab, snap);
        if rows.is_empty() {
            return Outcome::None;
        }
        match code {
            KeyCode::Up | KeyCode::Char('k') => {
                *sel = sel.checked_sub(1).unwrap_or(rows.len() - 1);
                return Outcome::None;
            }
            KeyCode::Down | KeyCode::Char('j') => {
                *sel = (*sel + 1) % rows.len();
                return Outcome::None;
            }
            _ => {}
        }
        *sel = (*sel).min(rows.len() - 1);
        let row = rows[*sel];
        let step = |left: bool| if left { -1i32 } else { 1 };
        match code {
            KeyCode::Left | KeyCode::Right | KeyCode::Char('h') | KeyCode::Char('l') => {
                let d = step(matches!(code, KeyCode::Left | KeyCode::Char('h')));
                match row {
                    Row::FanManual => {
                        self.manual = (self.manual as i32 + d * 5).clamp(0, 100) as u8;
                    }
                    Row::CpuGovernor if !self.governors.is_empty() => {
                        let n = self.governors.len() as i32;
                        let i = self
                            .governors
                            .iter()
                            .position(|g| *g == self.governor)
                            .unwrap_or(0) as i32;
                        self.governor = self.governors[((i + d).rem_euclid(n)) as usize].clone();
                    }
                    Row::CpuMin => {
                        self.min_mhz = (self.min_mhz as i32 + d * 100)
                            .clamp(self.hw_min as i32, self.max_mhz as i32)
                            as u32;
                    }
                    Row::CpuMax => {
                        self.max_mhz = (self.max_mhz as i32 + d * 100)
                            .clamp(self.min_mhz as i32, self.hw_max as i32)
                            as u32;
                    }
                    _ => {}
                }
                Outcome::None
            }
            KeyCode::Enter | KeyCode::Char(' ') => self.activate(row, snap, lang),
            _ => Outcome::None,
        }
    }

    fn activate(&mut self, row: Row, snap: &Snapshot, lang: Language) -> Outcome {
        match row {
            Row::FanPreset(i) => {
                self.curve = FAN_PRESETS[i].points();
                Outcome::Message(
                    tr(
                        lang,
                        "Profile loaded; choose Save to apply it.",
                        "Perfil carregado; escolha Salvar para aplicar.",
                    )
                    .into(),
                    false,
                )
            }
            Row::FanSave => Outcome::Confirm(Action::FanSave(self.curve.clone())),
            Row::FanManual => Outcome::Confirm(Action::FanManual(self.manual)),
            Row::FanAuto => Outcome::Confirm(Action::FanAuto),
            Row::EnableI2c => Outcome::Confirm(Action::EnableI2c),
            Row::CpuGovernor | Row::CpuMin | Row::CpuMax | Row::CpuApply => {
                Outcome::Confirm(Action::CpuLive {
                    governor: self.governor.clone(),
                    min: self.min_mhz,
                    max: self.max_mhz,
                })
            }
            Row::CpuProfile(i) => self.firmware_plan(OC_PRESETS[i], snap, lang),
            Row::Stability => {
                if let Some(s) = self.stability.as_ref().filter(|s| s.is_running()) {
                    s.stop();
                    return Outcome::None;
                }
                let context = match &self.trial {
                    Status::InTrial(s) => s.clone(),
                    _ => String::new(),
                };
                self.stability = Some(StabilityTest::start(
                    Duration::from_secs(STABILITY_MINUTES * 60),
                    context,
                ));
                Outcome::None
            }
            Row::TrialReboot => Outcome::Confirm(Action::TrialReboot),
            Row::TrialKeep => {
                if self.stable() {
                    Outcome::Confirm(Action::TrialKeep)
                } else {
                    Outcome::Message(
                        tr(lang, "Run the stability test first: only a stable result can be kept.", "Rode o teste de estabilidade antes: só um resultado estável pode ser mantido.").into(),
                        true,
                    )
                }
            }
            Row::TrialDiscard => Outcome::Confirm(Action::TrialDiscard),
            Row::Svc(a) => Outcome::Confirm(Action::Service(a)),
        }
    }

    /// Same rules as the firmware card of the graphical interface.
    fn firmware_plan(&self, preset: OcPreset, snap: &Snapshot, lang: Language) -> Outcome {
        let profile = crate::board::current().oc;
        let mut cfg = self.boot.clone();
        cfg.apply_preset_for(preset, &profile);
        if !cfg.exceeds_stock(&profile) {
            return Outcome::Confirm(Action::FirmwareSave(Box::new(cfg)));
        }
        let throttled = snap
            .power
            .throttled
            .as_deref()
            .and_then(|s| u32::from_str_radix(s.trim_start_matches("0x"), 16).ok())
            .map(crate::gpu::Throttled);
        if crate::power::problem_since_boot(throttled) {
            return Outcome::Message(
                tr(lang, "Blocked: the 5 V supply dipped since boot. Use a 5.1 V / 3 A supply before overclocking.", "Bloqueado: a fonte de 5 V caiu desde o boot. Use uma fonte de 5,1 V / 3 A antes do overclock.").into(),
                true,
            );
        }
        if crate::tryboot::supported() {
            Outcome::Confirm(Action::TrialPrepare(Box::new(cfg)))
        } else {
            Outcome::Message(
                tr(lang, "This board has no trial boot: use the graphical interface, which asks you to accept the risk.", "Esta placa não tem boot de teste: use a interface gráfica, que pede para aceitar o risco.").into(),
                true,
            )
        }
    }

    fn gpio_key(&mut self, code: KeyCode, snap: &Snapshot, lang: Language) -> Outcome {
        // Header as two rows: odd pins (1, 3 ... 39) and even pins (2, 4 ... 40).
        let s = self.gpio_sel;
        match code {
            KeyCode::Left | KeyCode::Char('h') => self.gpio_sel = if s >= 2 { s - 2 } else { s },
            KeyCode::Right | KeyCode::Char('l') => {
                self.gpio_sel = if s + 2 < 40 { s + 2 } else { s }
            }
            KeyCode::Up | KeyCode::Down | KeyCode::Char('k') | KeyCode::Char('j') => {
                self.gpio_sel = s ^ 1
            }
            KeyCode::Enter | KeyCode::Char(' ') => {
                let HeaderPinKind::Gpio(bcm) = RPI_40PIN_HEADER[s].kind else {
                    return Outcome::None;
                };
                let Some(gpio) = &snap.gpio else {
                    return Outcome::Message(
                        tr(
                            lang,
                            "No access to the GPIO pins (see --doctor).",
                            "Sem acesso aos pinos GPIO (veja --doctor).",
                        )
                        .into(),
                        true,
                    );
                };
                let Some(pin) = gpio.pins.iter().find(|p| p.bcm == bcm) else {
                    return Outcome::None;
                };
                if pin.mode != "OUT" {
                    return Outcome::Message(
                        format!(
                            "GPIO{bcm} {} {}; {}",
                            tr(lang, "is", "está em"),
                            pin.mode,
                            tr(
                                lang,
                                "only outputs are driven here.",
                                "só saídas são acionadas aqui."
                            )
                        ),
                        true,
                    );
                }
                return Outcome::Confirm(Action::GpioDrive {
                    bcm,
                    high: !pin.high,
                    direct: gpio.writable,
                });
            }
            _ => {}
        }
        Outcome::None
    }
}

// ---------------------------------------------------------------------------------------------
// Drawing
// ---------------------------------------------------------------------------------------------

fn row_line(
    selected: bool,
    label: String,
    value: String,
    color: ratatui::style::Color,
) -> Line<'static> {
    let marker = if selected { "> " } else { "  " };
    let style = if selected {
        Style::new().bg(DIM).add_modifier(Modifier::BOLD)
    } else {
        Style::new()
    };
    Line::from(vec![
        Span::styled(marker, style.fg(BLUE)),
        Span::styled(format!("{label:<24}"), style),
        Span::styled(value, style.fg(color)),
    ])
}

fn section(text: &str) -> Line<'static> {
    Line::from(Span::styled(
        text.to_string(),
        Style::new().fg(BLUE).add_modifier(Modifier::BOLD),
    ))
}

fn two_panes(area: Rect) -> [Rect; 2] {
    if area.width >= 90 {
        Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).areas(area)
    } else {
        Layout::vertical([Constraint::Percentage(45), Constraint::Percentage(55)]).areas(area)
    }
}

pub fn draw_fan(
    f: &mut Frame,
    area: Rect,
    st: &State,
    snap: &Snapshot,
    sel: usize,
    lang: Language,
) {
    let [left, right] = two_panes(area);
    // Curve as bars from 30 to 90 °C, with the current temperature highlighted.
    let block = boxed(tr(lang, "curve", "curva"), BLUE);
    let inner = block.inner(left);
    f.render_widget(block, left);
    let temps: Vec<u32> = (30..=90).step_by(5).collect();
    let near = temps
        .iter()
        .min_by_key(|t| (**t as f32 - snap.temp_c).abs() as u32)
        .copied()
        .unwrap_or(30);
    let bars: Vec<Bar> = temps
        .iter()
        .map(|t| {
            let v = evaluate_fan_speed(*t as f32, &st.curve) as u64;
            Bar::default()
                .value(v)
                .text_value(format!("{v}"))
                .label(Line::from(format!("{t}")))
                .style(Style::new().fg(if *t == near { AMBER } else { BLUE }))
        })
        .collect();
    let bar_w = ((inner.width.saturating_sub(temps.len() as u16)) / temps.len() as u16).clamp(1, 5);
    f.render_widget(
        BarChart::default()
            .data(BarGroup::default().bars(&bars))
            .bar_width(bar_w)
            .bar_gap(1)
            .max(100),
        inner,
    );

    let block = boxed(tr(lang, "fan", "ventoinha"), AMBER);
    let inner = block.inner(right);
    f.render_widget(block, right);
    let w = inner.width;
    let mut lines = vec![
        meter("temp", (snap.temp_c - 25.0) / 60.0, &snap.temp_display, w),
        meter(
            "duty",
            snap.fan_pct as f32 / 100.0,
            &format!("{}%", snap.fan_pct),
            w,
        ),
        kv(
            "mode",
            if st.manual_active {
                format!("manual {}%", st.manual)
            } else {
                tr(lang, "automatic curve", "curva automática").to_string()
            },
            GREEN,
        ),
        kv(
            "I2C",
            if snap.fan_bus {
                "ok"
            } else {
                tr(
                    lang,
                    "OFF: fan not controlled",
                    "DESLIGADO: ventoinha sem controle",
                )
            },
            if snap.fan_bus { GREEN } else { RED },
        ),
        Line::from(""),
    ];
    for (i, row) in st.rows(super::Tab::Fan, snap).into_iter().enumerate() {
        let selected = i == sel;
        lines.push(match row {
            Row::EnableI2c => row_line(
                selected,
                tr(lang, "Enable I2C", "Ativar I2C").into(),
                String::new(),
                RED,
            ),
            Row::FanPreset(p) => {
                let pts = FAN_PRESETS[p].points();
                let active = pts == st.curve;
                row_line(
                    selected,
                    FAN_PRESETS[p].label(lang).to_string(),
                    if active {
                        tr(lang, "loaded", "carregado").into()
                    } else {
                        String::new()
                    },
                    GREEN,
                )
            }
            Row::FanSave => row_line(
                selected,
                tr(lang, "Save curve", "Salvar curva").into(),
                if st.unsaved() {
                    tr(lang, "unsaved changes", "alterações não salvas").into()
                } else {
                    tr(lang, "saved", "salva").into()
                },
                if st.unsaved() { AMBER } else { GREEN },
            ),
            Row::FanManual => row_line(
                selected,
                tr(lang, "Manual speed", "Rotação manual").into(),
                format!("< {}% >", st.manual),
                AMBER,
            ),
            Row::FanAuto => row_line(
                selected,
                tr(lang, "Return to the curve", "Voltar à curva").into(),
                String::new(),
                GREEN,
            ),
            _ => Line::from(""),
        });
    }
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

pub fn draw_cpu(
    f: &mut Frame,
    area: Rect,
    st: &State,
    snap: &Snapshot,
    sel: usize,
    lang: Language,
) {
    let [left, right] = two_panes(area);
    let rows = st.rows(super::Tab::Cpu, snap);
    let profile = crate::board::current().oc;
    let line_for = |i: usize, row: Row| -> Line<'static> {
        let selected = i == sel;
        match row {
            Row::CpuGovernor => row_line(
                selected,
                "Governor".into(),
                format!("< {} >", st.governor),
                GREEN,
            ),
            Row::CpuMin => row_line(
                selected,
                tr(lang, "Minimum clock", "Clock mínimo").into(),
                format!("< {} MHz >", st.min_mhz),
                GREEN,
            ),
            Row::CpuMax => row_line(
                selected,
                tr(lang, "Maximum clock", "Clock máximo").into(),
                format!("< {} MHz >", st.max_mhz),
                GREEN,
            ),
            Row::CpuApply => row_line(
                selected,
                tr(lang, "Apply live settings", "Aplicar ajustes ao vivo").into(),
                String::new(),
                GREEN,
            ),
            Row::CpuProfile(p) => {
                let mut cfg = st.boot.clone();
                cfg.apply_preset_for(OC_PRESETS[p], &profile);
                let above = cfg.exceeds_stock(&profile);
                row_line(
                    selected,
                    OC_PRESETS[p].label(lang).to_string(),
                    if above {
                        tr(lang, "trial boot", "boot de teste").to_string()
                    } else if OC_PRESETS[p] == OcPreset::StockBoost {
                        tr(lang, "factory", "fábrica").to_string()
                    } else {
                        String::new()
                    },
                    if above { AMBER } else { GREEN },
                )
            }
            Row::Stability => {
                let running = st.stability.as_ref().is_some_and(|s| s.is_running());
                let value = match st.stability.as_ref().map(|s| s.snapshot()) {
                    Some(r) if running => format!("{:.0}%", r.progress() * 100.0),
                    Some(r) => match r.verdict {
                        Some(Verdict::Stable) => tr(lang, "stable", "estável").into(),
                        Some(_) => tr(lang, "not stable", "não estável").into(),
                        None => String::new(),
                    },
                    None => String::new(),
                };
                row_line(
                    selected,
                    if running {
                        tr(lang, "Stop stability test", "Parar teste").into()
                    } else {
                        format!(
                            "{} ({STABILITY_MINUTES} min)",
                            tr(lang, "Stability test", "Teste de estabilidade")
                        )
                    },
                    value,
                    if st.stable() { GREEN } else { AMBER },
                )
            }
            Row::TrialReboot => row_line(
                selected,
                tr(lang, "Reboot into the trial", "Reiniciar no teste").into(),
                String::new(),
                AMBER,
            ),
            Row::TrialKeep => row_line(
                selected,
                tr(lang, "Keep trial settings", "Manter ajustes de teste").into(),
                if st.stable() {
                    String::new()
                } else {
                    tr(lang, "needs a stable test", "precisa de teste estável").into()
                },
                if st.stable() { GREEN } else { DIM },
            ),
            Row::TrialDiscard => row_line(
                selected,
                tr(lang, "Discard trial", "Descartar teste").into(),
                String::new(),
                RED,
            ),
            _ => Line::from(""),
        }
    };

    let block = boxed(
        tr(lang, "live (until reboot)", "ao vivo (até o reboot)"),
        GREEN,
    );
    let inner = block.inner(left);
    f.render_widget(block, left);
    let w = inner.width;
    let mut lines = vec![
        meter(
            "clock",
            snap.cpu.mhz as f32 / st.hw_max.max(1) as f32,
            &format!("{} MHz", snap.cpu.mhz),
            w,
        ),
        meter("temp", (snap.temp_c - 25.0) / 60.0, &snap.temp_display, w),
        kv(
            "fw",
            snap.power.summary.clone(),
            if snap.power.problem { RED } else { GREEN },
        ),
        Line::from(""),
    ];
    for (i, row) in rows.iter().enumerate().filter(|(_, r)| {
        matches!(
            r,
            Row::CpuGovernor | Row::CpuMin | Row::CpuMax | Row::CpuApply
        )
    }) {
        lines.push(line_for(i, *row));
    }
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);

    let block = boxed(
        tr(lang, "firmware (config.txt)", "firmware (config.txt)"),
        AMBER,
    );
    let inner = block.inner(right);
    f.render_widget(block, right);
    let trial = match &st.trial {
        Status::Idle => tr(lang, "no trial in progress", "nenhum teste em andamento").to_string(),
        Status::Ready(s) => format!("{}: {s}", tr(lang, "trial ready", "teste pronto")),
        Status::InTrial(s) => format!("{}: {s}", tr(lang, "running the trial", "rodando o teste")),
        Status::Failed(s) | Status::NotKept(s) => format!(
            "{}: {s}",
            tr(lang, "trial did not finish", "teste não terminou")
        ),
    };
    let mut lines = vec![
        kv(
            "boot",
            format!(
                "{} MHz · over_voltage {}",
                st.boot.arm_freq_mhz, st.boot.over_voltage
            ),
            BLUE,
        ),
        kv("trial", trial, AMBER),
        Line::from(""),
        section(tr(lang, "Profiles", "Perfis")),
    ];
    for (i, row) in rows.iter().enumerate() {
        match row {
            Row::CpuProfile(_) => lines.push(line_for(i, *row)),
            Row::Stability => {
                lines.push(Line::from(""));
                lines.push(section(tr(lang, "Safe overclock", "Overclock seguro")));
                lines.push(line_for(i, *row));
            }
            Row::TrialReboot | Row::TrialKeep | Row::TrialDiscard => lines.push(line_for(i, *row)),
            _ => {}
        }
    }
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

pub fn draw_gpio(f: &mut Frame, area: Rect, st: &State, snap: &Snapshot, lang: Language) {
    let block = boxed("gpio", GREEN);
    let inner = block.inner(area);
    f.render_widget(block, area);
    let Some(gpio) = &snap.gpio else {
        f.render_widget(
            Paragraph::new(tr(
                lang,
                "No access to the GPIO pins. Run argon-control-center --doctor.",
                "Sem acesso aos pinos GPIO. Rode argon-control-center --doctor.",
            ))
            .fg(AMBER),
            inner,
        );
        return;
    };
    let cell = |phys: usize| -> Span<'static> {
        let def = &RPI_40PIN_HEADER[phys];
        let (glyph, color) = match def.kind {
            HeaderPinKind::Power3v3 => ("3V", AMBER),
            HeaderPinKind::Power5v => ("5V", RED),
            HeaderPinKind::Ground => ("--", DIM),
            HeaderPinKind::IdEeprom => ("ID", BLUE),
            HeaderPinKind::Gpio(bcm) => match gpio.pins.iter().find(|p| p.bcm == bcm) {
                Some(p) if p.high => ("##", GREEN),
                Some(_) => ("..", ratatui::style::Color::Gray),
                None => ("??", DIM),
            },
        };
        let mut style = Style::new().fg(color);
        if phys == st.gpio_sel {
            style = style
                .bg(BLUE)
                .fg(ratatui::style::Color::Black)
                .add_modifier(Modifier::BOLD);
        }
        Span::styled(format!("{glyph} "), style)
    };
    let even: Vec<Span> = (0..20).map(|i| cell(i * 2 + 1)).collect();
    let odd: Vec<Span> = (0..20).map(|i| cell(i * 2)).collect();
    let def = &RPI_40PIN_HEADER[st.gpio_sel];
    let detail = match def.kind {
        HeaderPinKind::Gpio(bcm) => match gpio.pins.iter().find(|p| p.bcm == bcm) {
            Some(p) => format!(
                "GPIO{bcm} · pin {}{} · {} · pull {} · {}",
                p.pin,
                if p.function.is_empty() {
                    String::new()
                } else {
                    format!(" ({})", p.function)
                },
                p.mode,
                p.pull,
                if p.high { "HIGH" } else { "LOW" }
            ),
            None => format!("GPIO{bcm}"),
        },
        _ => format!("pin {} · {}", def.phys_num, def.label),
    };
    let source = if gpio.writable {
        tr(lang, "live (pinctrl)", "ao vivo (pinctrl)")
    } else {
        tr(
            lang,
            "read-only service: driving asks for the password",
            "serviço só leitura: acionar pede a senha",
        )
    };
    let lines = vec![
        Line::from(tr(lang, "Header seen from above, pin 1 at the bottom left:", "Conector visto de cima, pino 1 embaixo à esquerda:")).fg(ratatui::style::Color::Gray),
        Line::from(""),
        Line::from(even),
        Line::from(odd),
        Line::from(""),
        Line::from(Span::styled(detail, Style::new().add_modifier(Modifier::BOLD))),
        kv("legend", "## HIGH  .. LOW  3V/5V power  -- GND  ID EEPROM", ratatui::style::Color::Gray),
        kv("source", source.to_string(), if gpio.writable { GREEN } else { AMBER }),
        Line::from(""),
        Line::from(tr(lang, "Arrows move · Enter toggles an output pin (inputs, I2C and UART are never changed)", "Setas movem · Enter inverte um pino de saída (entradas, I2C e UART nunca são alterados)")).fg(BLUE),
    ];
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

pub fn draw_services(
    f: &mut Frame,
    area: Rect,
    st: &State,
    snap: &Snapshot,
    sel: usize,
    lang: Language,
) {
    let block = boxed("argononed", AMBER);
    let inner = block.inner(area);
    f.render_widget(block, area);
    let mut lines = vec![
        kv(
            "state",
            if snap.daemon_active {
                tr(lang, "active", "ativo")
            } else {
                tr(lang, "stopped", "parado")
            },
            if snap.daemon_active { GREEN } else { RED },
        ),
        kv(
            "boot",
            match st.service_enabled {
                Some(true) => tr(lang, "starts at boot", "inicia no boot"),
                Some(false) => tr(lang, "does not start at boot", "não inicia no boot"),
                None => "—",
            },
            if st.service_enabled == Some(true) {
                GREEN
            } else {
                AMBER
            },
        ),
        Line::from(tr(
            lang,
            "argononed runs the fan curve, the power button and the OLED display.",
            "O argononed controla a curva da ventoinha, o botão power e o visor OLED.",
        ))
        .fg(ratatui::style::Color::Gray),
        Line::from(""),
    ];
    for (i, row) in st.rows(super::Tab::Services, snap).into_iter().enumerate() {
        if let Row::Svc(a) = row {
            let label = match a {
                "start" => tr(lang, "Start", "Iniciar"),
                "stop" => tr(lang, "Stop", "Parar"),
                "restart" => tr(lang, "Restart", "Reiniciar"),
                "enable" => tr(lang, "Start at boot", "Iniciar no boot"),
                _ => tr(lang, "Do not start at boot", "Não iniciar no boot"),
            };
            lines.push(row_line(i == sel, label.into(), String::new(), GREEN));
        }
    }
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}
