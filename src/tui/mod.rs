//! Terminal user interface, used when there is no graphical session (no Wayland / X11
//! display) or with `--tui`: a btop-style dashboard plus Fan, CPU, GPIO and Services tabs with
//! the same safety rules as the graphical interface.
//!
//! Data comes from [`crate::snapshot`], the same source as `--json` and the desktop widgets.

mod controls;

use crate::i18n::Language;
use crate::snapshot::{self, Snapshot};
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Clear, Paragraph, Sparkline, Wrap};
use ratatui::{DefaultTerminal, Frame};
use std::time::{Duration, Instant};

const TICK: Duration = Duration::from_millis(1000);
const HISTORY: usize = 120;

const BLUE: Color = Color::Rgb(61, 174, 233);
const GREEN: Color = Color::Rgb(39, 174, 96);
const AMBER: Color = Color::Rgb(243, 156, 18);
const RED: Color = Color::Rgb(218, 68, 83);
const PURPLE: Color = Color::Rgb(155, 89, 182);
const DIM: Color = Color::Rgb(70, 74, 80);

/// English or Portuguese text.
fn tr(lang: Language, en: &'static str, pt: &'static str) -> &'static str {
    match lang {
        Language::EnUs => en,
        Language::PtBr => pt,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tab {
    Dashboard,
    Fan,
    Cpu,
    Gpio,
    Services,
}

impl Tab {
    const ALL: [Tab; 5] = [Tab::Dashboard, Tab::Fan, Tab::Cpu, Tab::Gpio, Tab::Services];

    fn index(self) -> usize {
        Self::ALL.iter().position(|t| *t == self).unwrap_or(0)
    }

    fn title(self, lang: Language) -> &'static str {
        match self {
            Tab::Dashboard => tr(lang, "Dashboard", "Painel"),
            Tab::Fan => tr(lang, "Fan", "Ventoinha"),
            Tab::Cpu => "CPU",
            Tab::Gpio => "GPIO",
            Tab::Services => tr(lang, "Services", "Serviços"),
        }
    }
}

struct App {
    snap: Snapshot,
    cpu_hist: Vec<u64>,
    rx_hist: Vec<u64>,
    lang: Language,
    show_help: bool,
    tab: Tab,
    /// Selected row of each tab.
    sel: [usize; 5],
    ctl: controls::State,
    confirm: Option<controls::Action>,
    message: Option<(String, bool, Instant)>,
}

impl App {
    fn new(snap: Snapshot, lang: Language) -> Self {
        App {
            snap,
            cpu_hist: Vec::new(),
            rx_hist: Vec::new(),
            lang,
            show_help: false,
            tab: Tab::Dashboard,
            sel: [0; 5],
            ctl: controls::State::load(),
            confirm: None,
            message: None,
        }
    }

    fn refresh(&mut self) {
        self.snap = snapshot::take();
        push(&mut self.cpu_hist, self.snap.cpu.total.round() as u64);
        push(&mut self.rx_hist, self.snap.net.rx_kbps.round() as u64);
        self.ctl.tick();
    }

    fn say(&mut self, text: String, error: bool) {
        self.message = Some((text, error, Instant::now()));
    }
}

/// Leaves the alternate screen so pkexec can ask for the password in the terminal, runs `f`
/// and restores the interface.
fn suspended<T>(
    terminal: &mut DefaultTerminal,
    lang: Language,
    f: impl FnOnce() -> T,
) -> std::io::Result<T> {
    ratatui::restore();
    println!(
        "\n{}",
        tr(
            lang,
            "Argon Control Center: the system may ask for your password.",
            "Argon Control Center: o sistema pode pedir sua senha."
        )
    );
    let out = f();
    *terminal = ratatui::try_init()?;
    terminal.clear()?;
    Ok(out)
}

fn push(v: &mut Vec<u64>, x: u64) {
    if v.len() >= HISTORY {
        v.remove(0);
    }
    v.push(x);
}

/// Runs the TUI until `q` / `Esc`. The terminal is restored on exit and on panic.
pub fn run() -> std::io::Result<()> {
    let mut terminal = ratatui::try_init()?;
    let result = event_loop(&mut terminal);
    ratatui::restore();
    result
}

fn event_loop(terminal: &mut DefaultTerminal) -> std::io::Result<()> {
    // Prime the rate cache so the first frame already shows real percentages.
    snapshot::take();
    std::thread::sleep(Duration::from_millis(250));
    let mut app = App::new(snapshot::take(), crate::settings::load().language);
    let mut last = Instant::now();
    loop {
        terminal.draw(|f| draw(f, &app))?;
        let timeout = TICK.saturating_sub(last.elapsed());
        if event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press && handle_key(terminal, &mut app, key.code)? {
                    return Ok(());
                }
            }
        }
        if last.elapsed() >= TICK {
            app.refresh();
            last = Instant::now();
        }
    }
}

/// Returns true to quit.
fn handle_key(
    terminal: &mut DefaultTerminal,
    app: &mut App,
    code: KeyCode,
) -> std::io::Result<bool> {
    // Confirmation window: y / Enter runs the action, anything else cancels it.
    if let Some(action) = app.confirm.take() {
        if matches!(
            code,
            KeyCode::Char('y') | KeyCode::Char('s') | KeyCode::Enter
        ) {
            let lang = app.lang;
            let res = if action.needs_root() {
                suspended(terminal, lang, || action.run(&mut app.ctl))?
            } else {
                action.run(&mut app.ctl)
            };
            app.ctl.after_action();
            app.refresh();
            match res {
                Ok(msg) => app.say(msg.lines().last().unwrap_or("OK").to_string(), false),
                Err(e) => app.say(e, true),
            }
        } else {
            app.say(tr(app.lang, "Cancelled.", "Cancelado.").into(), false);
        }
        return Ok(false);
    }
    if app.show_help {
        app.show_help = false;
        return Ok(false);
    }
    match code {
        KeyCode::Char('q') | KeyCode::Esc => return Ok(true),
        KeyCode::Char('?') => app.show_help = true,
        KeyCode::Char(c @ '1'..='5') => app.tab = Tab::ALL[c as usize - '1' as usize],
        KeyCode::Tab => app.tab = Tab::ALL[(app.tab.index() + 1) % Tab::ALL.len()],
        KeyCode::BackTab => {
            app.tab = Tab::ALL[(app.tab.index() + Tab::ALL.len() - 1) % Tab::ALL.len()]
        }
        _ if app.tab != Tab::Dashboard => {
            let i = app.tab.index();
            let mut sel = app.sel[i];
            let outcome = app.ctl.key(app.tab, &mut sel, code, &app.snap, app.lang);
            app.sel[i] = sel;
            match outcome {
                controls::Outcome::Confirm(a) => app.confirm = Some(a),
                controls::Outcome::Message(m, err) => app.say(m, err),
                controls::Outcome::None => {}
            }
        }
        _ => {}
    }
    Ok(false)
}

// ---------------------------------------------------------------------------------------------
// Widgets
// ---------------------------------------------------------------------------------------------

/// green (0) -> amber (0.5) -> red (1), interpolated.
fn gradient(pos: f32) -> Color {
    let lerp = |a: Color, b: Color, t: f32| match (a, b) {
        (Color::Rgb(r1, g1, b1), Color::Rgb(r2, g2, b2)) => {
            let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
            Color::Rgb(m(r1, r2), m(g1, g2), m(b1, b2))
        }
        _ => a,
    };
    let p = pos.clamp(0.0, 1.0);
    if p < 0.5 {
        lerp(GREEN, AMBER, p * 2.0)
    } else {
        lerp(AMBER, RED, (p - 0.5) * 2.0)
    }
}

/// btop-style meter line: `label ■■■■■■■■····· value`, blocks coloured by position.
fn meter(label: &str, frac: f32, value: &str, width: u16) -> Line<'static> {
    let label_w = 7usize;
    let value_w = value.chars().count().max(9);
    let bar_w = (width as usize)
        .saturating_sub(label_w + value_w + 2)
        .max(4);
    let filled = (frac.clamp(0.0, 1.0) * bar_w as f32).round() as usize;
    let mut spans = vec![Span::styled(
        format!("{label:<label_w$}"),
        Style::new().fg(BLUE).add_modifier(Modifier::BOLD),
    )];
    for i in 0..bar_w {
        let color = if i < filled {
            gradient(i as f32 / (bar_w.max(2) - 1) as f32)
        } else {
            DIM
        };
        spans.push(Span::styled("■", Style::new().fg(color)));
    }
    spans.push(Span::raw(format!(" {value:>value_w$}")));
    Line::from(spans)
}

fn kv(key: &str, value: impl Into<String>, color: Color) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("{key:<7}"),
            Style::new().fg(BLUE).add_modifier(Modifier::BOLD),
        ),
        Span::styled(value.into(), Style::new().fg(color)),
    ])
}

fn boxed(title: &str, color: Color) -> Block<'static> {
    Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(color))
        .title(Span::styled(
            format!(" {title} "),
            Style::new().fg(color).add_modifier(Modifier::BOLD),
        ))
}

fn uptime(s: u64) -> String {
    let (d, h, m) = (s / 86400, (s % 86400) / 3600, (s % 3600) / 60);
    if d > 0 {
        format!("{d}d {h:02}h {m:02}m")
    } else {
        format!("{h}h {m:02}m")
    }
}

// ---------------------------------------------------------------------------------------------
// Layout
// ---------------------------------------------------------------------------------------------

fn draw(f: &mut Frame, app: &App) {
    let [tabs, main, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .areas(f.area());
    draw_tabs(f, tabs, app);
    match app.tab {
        Tab::Dashboard => draw_dashboard(f, main, app),
        Tab::Fan => controls::draw_fan(f, main, &app.ctl, &app.snap, app.sel[1], app.lang),
        Tab::Cpu => controls::draw_cpu(f, main, &app.ctl, &app.snap, app.sel[2], app.lang),
        Tab::Gpio => controls::draw_gpio(f, main, &app.ctl, &app.snap, app.lang),
        Tab::Services => {
            controls::draw_services(f, main, &app.ctl, &app.snap, app.sel[4], app.lang)
        }
    }
    draw_footer(f, footer, app);
    if let Some(action) = &app.confirm {
        draw_confirm(f, &action.describe(app.lang), app.lang);
    }
    if app.show_help {
        draw_help(f, app.lang);
    }
}

fn draw_tabs(f: &mut Frame, area: Rect, app: &App) {
    let mut spans = vec![Span::raw(" ")];
    for (i, t) in Tab::ALL.iter().enumerate() {
        let style = if *t == app.tab {
            Style::new()
                .fg(Color::Black)
                .bg(BLUE)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::new().fg(Color::Gray)
        };
        spans.push(Span::styled(
            format!(" {} {} ", i + 1, t.title(app.lang)),
            style,
        ));
        spans.push(Span::raw(" "));
    }
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn draw_footer(f: &mut Frame, area: Rect, app: &App) {
    let s = &app.snap;
    let keys = match (app.lang, app.tab) {
        (Language::PtBr, Tab::Dashboard) => "1-5 abas · ? ajuda · q sair",
        (Language::EnUs, Tab::Dashboard) => "1-5 tabs · ? help · q quit",
        (Language::PtBr, _) => "setas escolhem · Enter aplica · ? ajuda · q sair",
        (Language::EnUs, _) => "arrows select · Enter applies · ? help · q quit",
    };
    let recent = app
        .message
        .as_ref()
        .filter(|(_, _, at)| at.elapsed() < Duration::from_secs(8));
    let [left, right] = Layout::horizontal([
        Constraint::Min(1),
        Constraint::Length(keys.chars().count() as u16 + 2),
    ])
    .areas(area);
    match recent {
        Some((text, err, _)) => f.render_widget(
            Paragraph::new(format!(" {text}")).fg(if *err { RED } else { GREEN }),
            left,
        ),
        None => {
            let info = format!(
                " {} · {} · {} · up {} · load {:.2} {:.2} {:.2}",
                s.board,
                s.system.os,
                s.system.kernel,
                uptime(s.system.uptime_s),
                s.system.load[0],
                s.system.load[1],
                s.system.load[2]
            );
            f.render_widget(Paragraph::new(info).fg(Color::Gray), left);
        }
    }
    f.render_widget(Paragraph::new(keys).fg(BLUE).right_aligned(), right);
}

fn draw_confirm(f: &mut Frame, text: &str, lang: Language) {
    let width = (f.area().width.saturating_sub(8)).clamp(30, 72);
    let body = vec![
        Line::from(text.to_string()),
        Line::from(""),
        Line::from(tr(
            lang,
            "y / Enter: confirm     n / Esc: cancel",
            "s / Enter: confirmar     n / Esc: cancelar",
        ))
        .fg(BLUE),
    ];
    let lines_needed = (text.chars().count() as u16 / width.saturating_sub(4).max(1)) + 5;
    let [area] = Layout::horizontal([Constraint::Length(width)])
        .flex(Flex::Center)
        .areas(f.area());
    let [area] = Layout::vertical([Constraint::Length(lines_needed)])
        .flex(Flex::Center)
        .areas(area);
    f.render_widget(Clear, area);
    f.render_widget(
        Paragraph::new(body)
            .wrap(Wrap { trim: true })
            .block(boxed(tr(lang, "confirm", "confirmar"), AMBER)),
        area,
    );
}

fn draw_dashboard(f: &mut Frame, main: Rect, app: &App) {
    let s = &app.snap;
    let cores = s.cpu.cores.len() as u16;
    let cpu_h = cores + 7;
    let mem_h = 5 + s.disks.len() as u16;
    let gpu_h = 6;
    let net_h = 7;
    let argon_h = 7;

    if main.width >= 100 {
        let [left, right] =
            Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
                .areas(main);
        let [cpu, gpu] =
            Layout::vertical([Constraint::Length(cpu_h), Constraint::Length(gpu_h)]).areas(left);
        let [mem, net, argon] = Layout::vertical([
            Constraint::Length(mem_h),
            Constraint::Length(net_h),
            Constraint::Length(argon_h),
        ])
        .areas(right);
        draw_cpu(f, cpu, app);
        draw_gpu(f, gpu, s);
        draw_mem(f, mem, s);
        draw_net(f, net, app);
        draw_argon(f, argon, s);
    } else {
        let [cpu, mem, net, gpu, argon] = Layout::vertical([
            Constraint::Length(cpu_h),
            Constraint::Length(mem_h),
            Constraint::Length(net_h),
            Constraint::Length(gpu_h),
            Constraint::Length(argon_h),
        ])
        .areas(main);
        draw_cpu(f, cpu, app);
        draw_mem(f, mem, s);
        draw_net(f, net, app);
        draw_gpu(f, gpu, s);
        draw_argon(f, argon, s);
    }
}

fn draw_cpu(f: &mut Frame, area: Rect, app: &App) {
    let s = &app.snap;
    let block = boxed("cpu", BLUE);
    let inner = block.inner(area);
    f.render_widget(block, area);
    let rows = s.cpu.cores.len() as u16 + 2;
    let [text, graph] =
        Layout::vertical([Constraint::Length(rows), Constraint::Min(1)]).areas(inner);
    let w = text.width;
    let mut lines = vec![meter(
        "total",
        s.cpu.total / 100.0,
        &format!("{:.1}%", s.cpu.total),
        w,
    )];
    for (i, c) in s.cpu.cores.iter().enumerate() {
        lines.push(meter(&format!("cpu{i}"), c / 100.0, &format!("{c:.1}%"), w));
    }
    lines.push(kv(
        "clock",
        format!("{} MHz  {}", s.cpu.mhz, s.cpu.governor),
        Color::White,
    ));
    f.render_widget(Paragraph::new(lines), text);
    f.render_widget(
        Sparkline::default()
            .data(&app.cpu_hist)
            .max(100)
            .style(Style::new().fg(BLUE)),
        graph,
    );
}

fn draw_mem(f: &mut Frame, area: Rect, s: &Snapshot) {
    let block = boxed("mem", PURPLE);
    let inner = block.inner(area);
    f.render_widget(block, area);
    let w = inner.width;
    let gb = |mb: u64| mb as f32 / 1024.0;
    let mut lines = vec![
        meter(
            "mem",
            s.mem.pct / 100.0,
            &format!("{:.1}/{:.1}G", gb(s.mem.used_mb), gb(s.mem.total_mb)),
            w,
        ),
        meter(
            "swap",
            s.swap.pct / 100.0,
            &format!("{}/{}M", s.swap.used_mb, s.swap.total_mb),
            w,
        ),
        Line::from(Span::styled("disks", Style::new().fg(Color::Gray))),
    ];
    for d in &s.disks {
        let label: String = if d.mount_point == "/" {
            "/".into()
        } else {
            d.mount_point
                .rsplit('/')
                .next()
                .unwrap_or(&d.mount_point)
                .chars()
                .take(6)
                .collect()
        };
        lines.push(meter(
            &label,
            d.usage_pct / 100.0,
            &if d.total_gb < 10.0 {
                format!("{:.1}/{:.1}G", d.used_gb, d.total_gb)
            } else {
                format!("{:.0}/{:.0}G", d.used_gb, d.total_gb)
            },
            w,
        ));
    }
    f.render_widget(Paragraph::new(lines), inner);
}

fn draw_net(f: &mut Frame, area: Rect, app: &App) {
    let s = &app.snap;
    let block = boxed("net", GREEN);
    let inner = block.inner(area);
    f.render_widget(block, area);
    let peak = app.rx_hist.iter().copied().max().unwrap_or(0).max(100) as f32;
    let [text, graph] = Layout::vertical([Constraint::Length(2), Constraint::Min(1)]).areas(inner);
    let w = text.width;
    f.render_widget(
        Paragraph::new(vec![
            meter(
                "⬇ rx",
                s.net.rx_kbps / peak,
                &format!("{:.1} KB/s", s.net.rx_kbps),
                w,
            ),
            meter(
                "⬆ tx",
                s.net.tx_kbps / peak,
                &format!("{:.1} KB/s", s.net.tx_kbps),
                w,
            ),
        ]),
        text,
    );
    f.render_widget(
        Sparkline::default()
            .data(&app.rx_hist)
            .max(peak as u64)
            .style(Style::new().fg(GREEN)),
        graph,
    );
}

fn draw_gpu(f: &mut Frame, area: Rect, s: &Snapshot) {
    let block = boxed("gpu", GREEN);
    let inner = block.inner(area);
    f.render_widget(block, area);
    let w = inner.width;
    let mut lines = Vec::new();
    if let Some(r) = s.gpu.render {
        lines.push(meter("render", r / 100.0, &format!("{r:.1}%"), w));
    }
    if let Some(b) = s.gpu.bin {
        lines.push(meter("geom", b / 100.0, &format!("{b:.1}%"), w));
    }
    if let (Some(u), Some(t)) = (s.gpu.cma_used_mb, s.gpu.cma_total_mb) {
        lines.push(meter(
            "CMA",
            u as f32 / t.max(1) as f32,
            &format!("{u}/{t}M"),
            w,
        ));
    }
    if let Some(m) = s.gpu.v3d_mhz {
        // The firmware gates the V3D clock to 0 while the GPU is idle.
        let text = if m == 0 {
            "idle".to_string()
        } else {
            format!("{m} MHz")
        };
        lines.push(kv("v3d", text, Color::White));
    }
    if lines.is_empty() {
        lines.push(Line::from("—"));
    }
    f.render_widget(Paragraph::new(lines), inner);
}

fn draw_argon(f: &mut Frame, area: Rect, s: &Snapshot) {
    let block = boxed("argon", AMBER);
    let inner = block.inner(area);
    f.render_widget(block, area);
    let w = inner.width;
    let ok_bad = |ok: bool| if ok { GREEN } else { RED };
    let mut lines = vec![
        meter("temp", (s.temp_c - 25.0) / 60.0, &s.temp_display, w),
        meter(
            "fan",
            s.fan_pct as f32 / 100.0,
            &format!("{}%", s.fan_pct),
            w,
        ),
        kv(
            "5V",
            if s.power.undervoltage_now {
                "UNDER-VOLTAGE"
            } else {
                "OK"
            },
            ok_bad(!s.power.undervoltage_now),
        ),
        kv(
            "daemon",
            if s.daemon_active {
                "argononed active"
            } else {
                "argononed stopped"
            },
            ok_bad(s.daemon_active),
        ),
    ];
    if let Some(flags) = &s.power.throttled {
        lines.push(kv(
            "fw",
            format!("{} ({flags})", s.power.summary),
            ok_bad(!s.power.problem),
        ));
    }
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), inner);
}

fn draw_help(f: &mut Frame, lang: Language) {
    let text = match lang {
        Language::PtBr => vec![
            "Modo terminal do Argon Control Center.",
            "",
            "1-5 / Tab   abas: Painel, Ventoinha, CPU, GPIO, Serviços",
            "setas       escolhem a linha e ajustam valores (< >)",
            "Enter       aplica; toda mudança pede confirmação (s / n)",
            "?           esta ajuda          q / Esc   sair",
            "",
            "Mudanças com root mostram o pedido de senha do sistema no terminal.",
            "Overclock acima do padrão: fonte saudável, boot de teste e teste estável.",
            "Abre automaticamente quando não há sessão gráfica (Wayland/X11).",
            "",
            "Linha de comando:",
            "  --status        telemetria e hardware detectado",
            "  --json          leitura em JSON para scripts",
            "  --doctor        acesso ao hardware do seu usuário",
            "  --stress 180    teste de estabilidade",
        ],
        Language::EnUs => vec![
            "Argon Control Center terminal mode.",
            "",
            "1-5 / Tab   tabs: Dashboard, Fan, CPU, GPIO, Services",
            "arrows      select a row and adjust values (< >)",
            "Enter       applies; every change asks for confirmation (y / n)",
            "?           this help          q / Esc   quit",
            "",
            "Changes that need root show the system password prompt in the terminal.",
            "Overclock above factory: healthy power supply, trial boot and a stable test.",
            "Opens automatically when there is no graphical session (Wayland/X11).",
            "",
            "Command line:",
            "  --status        telemetry and detected hardware",
            "  --json          JSON snapshot for scripts",
            "  --doctor        hardware access for your user",
            "  --stress 180    stability test",
        ],
    };
    let width = text.iter().map(|l| l.chars().count()).max().unwrap_or(40) as u16 + 4;
    let height = text.len() as u16 + 2;
    let [area] = Layout::horizontal([Constraint::Length(width)])
        .flex(Flex::Center)
        .areas(f.area());
    let [area] = Layout::vertical([Constraint::Length(height)])
        .flex(Flex::Center)
        .areas(area);
    f.render_widget(Clear, area);
    f.render_widget(
        Paragraph::new(text.into_iter().map(Line::from).collect::<Vec<_>>())
            .block(boxed("help", BLUE)),
        area,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    #[test]
    fn gradient_endpoints() {
        assert_eq!(gradient(0.0), GREEN);
        assert_eq!(gradient(1.0), RED);
        assert_eq!(gradient(0.5), AMBER);
    }

    #[test]
    fn meter_fills_proportionally() {
        let line = meter("cpu0", 0.5, "50.0%", 40);
        let filled = line
            .spans
            .iter()
            .filter(|s| s.content == "■" && s.style.fg != Some(DIM))
            .count();
        let total = line.spans.iter().filter(|s| s.content == "■").count();
        assert_eq!(filled * 2, total);
    }

    #[test]
    fn renders_wide_and_narrow_without_panicking() {
        let mut app = App::new(snapshot::take(), Language::EnUs);
        app.cpu_hist = vec![10, 50, 90];
        app.rx_hist = vec![1, 2, 3];
        for tab in Tab::ALL {
            app.tab = tab;
            for (w, h) in [(140, 45), (80, 60), (40, 20)] {
                let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
                t.draw(|f| draw(f, &app)).unwrap();
                let text: String = t
                    .backend()
                    .buffer()
                    .content()
                    .iter()
                    .map(|c| c.symbol())
                    .collect();
                assert!(text.contains("Dashboard"), "{tab:?} {w}x{h}");
            }
        }
        app.confirm = Some(controls::Action::Service("restart"));
        app.show_help = true;
        let mut t = Terminal::new(TestBackend::new(100, 30)).unwrap();
        t.draw(|f| draw(f, &app)).unwrap();
    }
}

#[cfg(test)]
mod preview {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    /// `cargo test tui::preview -- --ignored --nocapture` prints a 120x40 frame.
    #[test]
    #[ignore]
    fn print_frame() {
        snapshot::take();
        std::thread::sleep(Duration::from_millis(300));
        let mut app = App::new(snapshot::take(), Language::EnUs);
        app.cpu_hist = (0..60).map(|i| (i * 7 % 100) as u64).collect();
        app.rx_hist = (0..60).map(|i| (i * 13 % 90) as u64).collect();
        app.tab = match std::env::var("TUI_TAB").as_deref() {
            Ok("fan") => Tab::Fan,
            Ok("cpu") => Tab::Cpu,
            Ok("gpio") => Tab::Gpio,
            Ok("services") => Tab::Services,
            _ => Tab::Dashboard,
        };
        let mut t = Terminal::new(TestBackend::new(120, 40)).unwrap();
        t.draw(|f| draw(f, &app)).unwrap();
        let buf = t.backend().buffer();
        for y in 0..buf.area.height {
            let line: String = (0..buf.area.width)
                .map(|x| buf[(x, y)].symbol().to_string())
                .collect();
            println!("{}", line.trim_end());
        }
    }
}
