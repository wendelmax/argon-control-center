use crate::privileged::run_root_script;
use std::collections::VecDeque;
use std::fs;
use std::io::{Read, Write};
use std::os::unix::io::AsRawFd;
use std::process::Command;
use std::time::Instant;

pub const LOGIC_HISTORY_LEN: usize = 80;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PinDirection {
    Input,
    Output,
    Alt(u8),
    Unknown,
}

impl PinDirection {
    pub fn short_label(&self) -> &'static str {
        match self {
            PinDirection::Input => "IN",
            PinDirection::Output => "OUT",
            PinDirection::Alt(0) => "ALT0",
            PinDirection::Alt(1) => "ALT1",
            PinDirection::Alt(2) => "ALT2",
            PinDirection::Alt(3) => "ALT3",
            PinDirection::Alt(4) => "ALT4",
            PinDirection::Alt(5) => "ALT5",
            PinDirection::Alt(_) => "ALT",
            PinDirection::Unknown => "---",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PinPull {
    Up,
    Down,
    None,
}

impl PinPull {
    pub fn short_label(&self) -> &'static str {
        match self {
            PinPull::Up => "PU",
            PinPull::Down => "PD",
            PinPull::None => "NP",
        }
    }
}

#[derive(Debug, Clone)]
pub struct GpioPinState {
    pub bcm: u8,
    pub name: String,
    pub role_note: &'static str,
    pub direction: PinDirection,
    pub pull: PinPull,
    pub level_high: bool,
    pub rising_edges: u32,
    pub falling_edges: u32,
    pub history: VecDeque<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeaderPinKind {
    Power3v3,
    Power5v,
    Ground,
    IdEeprom,
    Gpio(u8),
}

#[derive(Debug, Clone, Copy)]
pub struct PhysicalPinDef {
    pub phys_num: u8,
    pub label: &'static str,
    pub kind: HeaderPinKind,
}

/// Standard 40-pin Raspberry Pi physical header definition (Pins 1..40).
pub const RPI_40PIN_HEADER: [PhysicalPinDef; 40] = [
    PhysicalPinDef {
        phys_num: 1,
        label: "3.3V PWR",
        kind: HeaderPinKind::Power3v3,
    },
    PhysicalPinDef {
        phys_num: 2,
        label: "5V PWR",
        kind: HeaderPinKind::Power5v,
    },
    PhysicalPinDef {
        phys_num: 3,
        label: "GPIO 2 (SDA1 I2C)",
        kind: HeaderPinKind::Gpio(2),
    },
    PhysicalPinDef {
        phys_num: 4,
        label: "5V PWR",
        kind: HeaderPinKind::Power5v,
    },
    PhysicalPinDef {
        phys_num: 5,
        label: "GPIO 3 (SCL1 I2C)",
        kind: HeaderPinKind::Gpio(3),
    },
    PhysicalPinDef {
        phys_num: 6,
        label: "GND",
        kind: HeaderPinKind::Ground,
    },
    PhysicalPinDef {
        phys_num: 7,
        label: "GPIO 4 (ARGON BTN)",
        kind: HeaderPinKind::Gpio(4),
    },
    PhysicalPinDef {
        phys_num: 8,
        label: "GPIO 14 (UART TX)",
        kind: HeaderPinKind::Gpio(14),
    },
    PhysicalPinDef {
        phys_num: 9,
        label: "GND",
        kind: HeaderPinKind::Ground,
    },
    PhysicalPinDef {
        phys_num: 10,
        label: "GPIO 15 (UART RX)",
        kind: HeaderPinKind::Gpio(15),
    },
    PhysicalPinDef {
        phys_num: 11,
        label: "GPIO 17",
        kind: HeaderPinKind::Gpio(17),
    },
    PhysicalPinDef {
        phys_num: 12,
        label: "GPIO 18 (PCM CLK)",
        kind: HeaderPinKind::Gpio(18),
    },
    PhysicalPinDef {
        phys_num: 13,
        label: "GPIO 27 (ARGON LID)",
        kind: HeaderPinKind::Gpio(27),
    },
    PhysicalPinDef {
        phys_num: 14,
        label: "GND",
        kind: HeaderPinKind::Ground,
    },
    PhysicalPinDef {
        phys_num: 15,
        label: "GPIO 22",
        kind: HeaderPinKind::Gpio(22),
    },
    PhysicalPinDef {
        phys_num: 16,
        label: "GPIO 23 (ARGON IR)",
        kind: HeaderPinKind::Gpio(23),
    },
    PhysicalPinDef {
        phys_num: 17,
        label: "3.3V PWR",
        kind: HeaderPinKind::Power3v3,
    },
    PhysicalPinDef {
        phys_num: 18,
        label: "GPIO 24",
        kind: HeaderPinKind::Gpio(24),
    },
    PhysicalPinDef {
        phys_num: 19,
        label: "GPIO 10 (SPI MOSI)",
        kind: HeaderPinKind::Gpio(10),
    },
    PhysicalPinDef {
        phys_num: 20,
        label: "GND",
        kind: HeaderPinKind::Ground,
    },
    PhysicalPinDef {
        phys_num: 21,
        label: "GPIO 9 (SPI MISO)",
        kind: HeaderPinKind::Gpio(9),
    },
    PhysicalPinDef {
        phys_num: 22,
        label: "GPIO 25",
        kind: HeaderPinKind::Gpio(25),
    },
    PhysicalPinDef {
        phys_num: 23,
        label: "GPIO 11 (SPI SCLK)",
        kind: HeaderPinKind::Gpio(11),
    },
    PhysicalPinDef {
        phys_num: 24,
        label: "GPIO 8 (SPI CE0)",
        kind: HeaderPinKind::Gpio(8),
    },
    PhysicalPinDef {
        phys_num: 25,
        label: "GND",
        kind: HeaderPinKind::Ground,
    },
    PhysicalPinDef {
        phys_num: 26,
        label: "GPIO 7 (SPI CE1)",
        kind: HeaderPinKind::Gpio(7),
    },
    PhysicalPinDef {
        phys_num: 27,
        label: "ID_SD (EEPROM)",
        kind: HeaderPinKind::IdEeprom,
    },
    PhysicalPinDef {
        phys_num: 28,
        label: "ID_SC (EEPROM)",
        kind: HeaderPinKind::IdEeprom,
    },
    PhysicalPinDef {
        phys_num: 29,
        label: "GPIO 5",
        kind: HeaderPinKind::Gpio(5),
    },
    PhysicalPinDef {
        phys_num: 30,
        label: "GND",
        kind: HeaderPinKind::Ground,
    },
    PhysicalPinDef {
        phys_num: 31,
        label: "GPIO 6",
        kind: HeaderPinKind::Gpio(6),
    },
    PhysicalPinDef {
        phys_num: 32,
        label: "GPIO 12 (PWM0)",
        kind: HeaderPinKind::Gpio(12),
    },
    PhysicalPinDef {
        phys_num: 33,
        label: "GPIO 13 (PWM1)",
        kind: HeaderPinKind::Gpio(13),
    },
    PhysicalPinDef {
        phys_num: 34,
        label: "GND",
        kind: HeaderPinKind::Ground,
    },
    PhysicalPinDef {
        phys_num: 35,
        label: "GPIO 19 (PCM FS)",
        kind: HeaderPinKind::Gpio(19),
    },
    PhysicalPinDef {
        phys_num: 36,
        label: "GPIO 16",
        kind: HeaderPinKind::Gpio(16),
    },
    PhysicalPinDef {
        phys_num: 37,
        label: "GPIO 26",
        kind: HeaderPinKind::Gpio(26),
    },
    PhysicalPinDef {
        phys_num: 38,
        label: "GPIO 20 (PCM DIN)",
        kind: HeaderPinKind::Gpio(20),
    },
    PhysicalPinDef {
        phys_num: 39,
        label: "GND",
        kind: HeaderPinKind::Ground,
    },
    PhysicalPinDef {
        phys_num: 40,
        label: "GPIO 21 (PCM DOUT)",
        kind: HeaderPinKind::Gpio(21),
    },
];

/// Where a console line comes from; each GPIO Lab tab shows its own source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogSource {
    System,
    Gpio,
    Serial,
    I2c,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataFormat {
    Ascii,
    Hex,
    Binary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineEnding {
    None,
    Newline,
    CarriageReturn,
    BothNlCr,
}

impl LineEnding {
    pub fn label(&self) -> &'static str {
        match self {
            LineEnding::None => "Sem Fim de Linha (None)",
            LineEnding::Newline => "Nova Linha (\\n - LF)",
            LineEnding::CarriageReturn => "Retorno de Carro (\\r - CR)",
            LineEnding::BothNlCr => "Ambos NL & CR (\\r\\n)",
        }
    }

    pub fn suffix(&self) -> &'static str {
        match self {
            LineEnding::None => "",
            LineEnding::Newline => "\n",
            LineEnding::CarriageReturn => "\r",
            LineEnding::BothNlCr => "\r\n",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ConsoleEntry {
    pub source: LogSource,
    pub timestamp: String,
    pub direction: &'static str, // "RX", "TX", "EV", "SYS", "ERR"
    pub text: String,
    pub raw_bytes: Vec<u8>,
}

pub struct GpioSerialLabState {
    pub pins: Vec<GpioPinState>,
    pub watched_bcm: Vec<u8>,
    pub selected_bcm: u8,
    pub hw_access_unlocked: bool,
    pub simulate_pulses: bool,
    pub sim_tick: u64,

    // Console state (Arduino Serial Monitor style)
    /// Log every edge of the analyzer channels.
    pub log_edges: bool,
    pub available_ports: Vec<String>,
    pub selected_port: String,
    pub baud_rate: u32,
    pub serial_connected: bool,
    pub serial_file: Option<fs::File>,
    pub data_format: DataFormat,
    pub line_ending: LineEnding,
    pub show_timestamps: bool,
    pub auto_scroll: bool,
    pub paused: bool,
    pub tx_input: String,
    /// Command line of the Log tab (help, scan, mcu, gpio, fan, high, low).
    pub cmd_input: String,
    pub rx_bytes_total: usize,
    pub tx_bytes_total: usize,
    pub entries: VecDeque<ConsoleEntry>,
    pub start_instant: Instant,
}

impl Default for GpioSerialLabState {
    fn default() -> Self {
        Self::new()
    }
}

impl GpioSerialLabState {
    pub fn new() -> Self {
        let mut pins = Vec::with_capacity(28);
        for bcm in 0..=27u8 {
            let (role_note, default_dir, default_pull, default_high) = match bcm {
                2 => (
                    "Argon I2C SDA1 (0x1a MCU / 0x3c OLED)",
                    PinDirection::Alt(0),
                    PinPull::Up,
                    true,
                ),
                3 => (
                    "Argon I2C SCL1 (0x1a MCU / 0x3c OLED)",
                    PinDirection::Alt(0),
                    PinPull::Up,
                    true,
                ),
                4 => (
                    "Argon Power Button Pulse Input (20-50ms)",
                    PinDirection::Input,
                    PinPull::Down,
                    false,
                ),
                14 => (
                    "UART0 TXD (Serial Console /dev/ttyS0)",
                    PinDirection::Alt(5),
                    PinPull::None,
                    true,
                ),
                15 => (
                    "UART0 RXD (Serial Console /dev/ttyS0)",
                    PinDirection::Alt(5),
                    PinPull::Up,
                    true,
                ),
                18 => (
                    "PCM_CLK / Hardware PWM0 (BLSTR DAC)",
                    PinDirection::Input,
                    PinPull::Down,
                    false,
                ),
                19 => (
                    "PCM_FS (BLSTR DAC I2S)",
                    PinDirection::Input,
                    PinPull::Down,
                    false,
                ),
                20 => (
                    "PCM_DIN (BLSTR DAC I2S)",
                    PinDirection::Input,
                    PinPull::Down,
                    false,
                ),
                21 => (
                    "PCM_DOUT (BLSTR DAC I2S)",
                    PinDirection::Input,
                    PinPull::Down,
                    false,
                ),
                23 => (
                    "Argon IR Receiver (NEC Protocol)",
                    PinDirection::Input,
                    PinPull::Up,
                    true,
                ),
                27 => (
                    "Argon EON Lid Sensor (Pull-Up)",
                    PinDirection::Input,
                    PinPull::Up,
                    true,
                ),
                _ => (
                    "General Purpose Digital I/O",
                    PinDirection::Input,
                    PinPull::Down,
                    false,
                ),
            };
            pins.push(GpioPinState {
                bcm,
                name: format!("GPIO{bcm}"),
                role_note,
                direction: default_dir,
                pull: default_pull,
                level_high: default_high,
                rising_edges: 0,
                falling_edges: 0,
                history: VecDeque::from(vec![if default_high { 1 } else { 0 }; LOGIC_HISTORY_LEN]),
            });
        }

        let available_ports = detect_serial_ports();
        let selected_port = available_ports
            .first()
            .cloned()
            .unwrap_or_else(|| "/dev/ttyS0".to_string());

        let mut s = Self {
            pins,
            watched_bcm: vec![2, 3, 4, 23],
            selected_bcm: 4,
            hw_access_unlocked: false,
            simulate_pulses: false,
            sim_tick: 0,
            log_edges: true,
            available_ports,
            selected_port,
            baud_rate: 115200,
            serial_connected: false,
            serial_file: None,
            data_format: DataFormat::Ascii,
            line_ending: LineEnding::Newline,
            show_timestamps: true,
            auto_scroll: true,
            paused: false,
            tx_input: String::new(),
            cmd_input: String::new(),
            rx_bytes_total: 0,
            tx_bytes_total: 0,
            entries: VecDeque::new(),
            start_instant: Instant::now(),
        };

        s.push_log(
            LogSource::System,
            "SYS",
            "GPIO Lab ready. Type help in the Log tab to list the commands.",
            b"",
        );
        s.poll_gpio_states();
        s
    }

    pub fn format_timestamp(&self) -> String {
        let elapsed = self.start_instant.elapsed();
        let secs = elapsed.as_secs();
        let ms = elapsed.subsec_millis();
        let m = (secs / 60) % 60;
        let s = secs % 60;
        format!("{m:02}:{s:02}.{ms:03}")
    }

    pub fn push_log(&mut self, source: LogSource, direction: &'static str, text: &str, raw: &[u8]) {
        if self.paused {
            return;
        }
        if self.entries.len() >= 300 {
            self.entries.pop_front();
        }
        self.entries.push_back(ConsoleEntry {
            source,
            timestamp: self.format_timestamp(),
            direction,
            text: text.to_string(),
            raw_bytes: raw.to_vec(),
        });
    }

    /// Grants the current user hardware access the safe way (one Polkit prompt):
    /// `i2c` / `gpio` / serial groups + udev rules + `i2c-dev` at boot — no setuid binaries and
    /// no world-writable device nodes. Uses the script shipped by the distro package when
    /// installed, otherwise an embedded copy fed through stdin. Group membership applies after
    /// the next login.
    pub fn unlock_hardware_access(&mut self) -> Result<String, String> {
        let user = current_username().ok_or("Could not determine the current user")?;
        let user_q = crate::privileged::shell_quote(&user);
        let packaged = "/usr/lib/argon-control-center/setup-permissions.sh";
        let res = if std::path::Path::new(packaged).exists() {
            run_root_script(&format!("/bin/sh {packaged} {user_q}"))?
        } else {
            let rules = crate::privileged::shell_quote(SETUP_RULES);
            crate::privileged::run_root_script_with_stdin(
                &format!("ARGONCC_RULES={rules} /bin/sh -s -- {user_q}"),
                Some(SETUP_SCRIPT.as_bytes()),
            )?
        };
        self.hw_access_unlocked = true;
        self.push_log(LogSource::System, "SYS", &res, res.as_bytes());
        self.poll_gpio_states();
        Ok(res)
    }

    /// Polls live GPIO states via `pinctrl get 0-27` (or simulates signal transitions if simulation mode is active).
    pub fn poll_gpio_states(&mut self) {
        self.sim_tick = self.sim_tick.wrapping_add(1);

        // Try reading real hardware from `pinctrl get 0-27`
        if let Ok(out) = Command::new("/usr/bin/pinctrl")
            .args(["get", "0-27"])
            .output()
        {
            if out.status.success() {
                self.hw_access_unlocked = true;
                let stdout = String::from_utf8_lossy(&out.stdout);
                let parsed = parse_pinctrl_output(&stdout);
                let mut edge_events = Vec::new();

                for (bcm, dir, pull, high) in parsed {
                    if let Some(pin) = self.pins.get_mut(bcm as usize) {
                        let prev = pin.level_high;
                        pin.direction = dir;
                        pin.pull = pull;
                        pin.level_high = high;

                        if high && !prev {
                            pin.rising_edges += 1;
                            if self.watched_bcm.contains(&bcm) {
                                edge_events.push((bcm, "⬆ RISING (0 -> 1 HIGH 3.3V)"));
                            }
                        } else if !high && prev {
                            pin.falling_edges += 1;
                            if self.watched_bcm.contains(&bcm) {
                                edge_events.push((bcm, "⬇ FALLING (1 -> 0 LOW 0.0V)"));
                            }
                        }

                        if pin.history.len() >= LOGIC_HISTORY_LEN {
                            pin.history.pop_front();
                        }
                        pin.history.push_back(if high { 1 } else { 0 });
                    }
                }

                if self.log_edges {
                    for (bcm, ev_desc) in edge_events {
                        let msg = format!("GPIO{bcm:02} | {ev_desc}");
                        self.push_log(LogSource::Gpio, "EV", &msg, msg.as_bytes());
                    }
                }
                return;
            }
        }

        // If simulation is enabled or hardware isn't unlocked yet, update history buffers
        let mut sim_events = Vec::new();
        for pin in &mut self.pins {
            let prev = pin.level_high;
            if self.simulate_pulses {
                match pin.bcm {
                    2 | 3 => {
                        // Simulate I2C clock/data bursts
                        pin.level_high = !(self.sim_tick + pin.bcm as u64).is_multiple_of(3);
                    }
                    4 => {
                        // Simulate periodic Argon power button pulse every 8 ticks
                        pin.level_high = self.sim_tick.is_multiple_of(8);
                    }
                    23 => {
                        // Simulate IR receiver active-low pulse burst
                        pin.level_high = !self.sim_tick.is_multiple_of(5);
                    }
                    _ => {}
                }
            }
            if pin.level_high && !prev {
                pin.rising_edges += 1;
                if self.watched_bcm.contains(&pin.bcm) {
                    sim_events.push((pin.bcm, "⬆ RISING (0 -> 1 HIGH 3.3V) [SIM]"));
                }
            } else if !pin.level_high && prev {
                pin.falling_edges += 1;
                if self.watched_bcm.contains(&pin.bcm) {
                    sim_events.push((pin.bcm, "⬇ FALLING (1 -> 0 LOW 0.0V) [SIM]"));
                }
            }
            if pin.history.len() >= LOGIC_HISTORY_LEN {
                pin.history.pop_front();
            }
            pin.history.push_back(if pin.level_high { 1 } else { 0 });
        }

        if self.log_edges {
            for (bcm, ev_desc) in sim_events {
                let msg = format!("GPIO{bcm:02} | {ev_desc}");
                self.push_log(LogSource::Gpio, "EV", &msg, msg.as_bytes());
            }
        }
    }

    /// Sets a GPIO pin output state (`HIGH` or `LOW`) or direction using `pinctrl`.
    pub fn set_gpio_output(&mut self, bcm: u8, high: bool) -> Result<String, String> {
        let lvl = if high { "dh" } else { "dl" };
        let cmd = format!("/usr/bin/pinctrl set {bcm} op {lvl}");
        let res = if self.hw_access_unlocked {
            let out = Command::new("/usr/bin/pinctrl")
                .args(["set", &bcm.to_string(), "op", lvl])
                .output()
                .map_err(|e| e.to_string())?;
            if out.status.success() {
                Ok(format!(
                    "GPIO{bcm} set to OUTPUT {}",
                    if high { "HIGH (3.3V)" } else { "LOW (0V)" }
                ))
            } else {
                run_root_script(&cmd)
            }
        } else {
            run_root_script(&cmd)
        };

        if let Some(pin) = self.pins.get_mut(bcm as usize) {
            pin.direction = PinDirection::Output;
            pin.level_high = high;
        }
        let msg = match &res {
            Ok(m) => m.clone(),
            Err(e) => format!("GPIO{bcm} updated locally ({e})"),
        };
        self.push_log(LogSource::Gpio, "TX", &msg, msg.as_bytes());
        res
    }

    pub fn set_gpio_input(&mut self, bcm: u8, pull: PinPull) -> Result<String, String> {
        let p_arg = match pull {
            PinPull::Up => "pu",
            PinPull::Down => "pd",
            PinPull::None => "pn",
        };
        let cmd = format!("/usr/bin/pinctrl set {bcm} ip {p_arg}");
        let res = if self.hw_access_unlocked {
            let out = Command::new("/usr/bin/pinctrl")
                .args(["set", &bcm.to_string(), "ip", p_arg])
                .output()
                .map_err(|e| e.to_string())?;
            if out.status.success() {
                Ok(format!("GPIO{bcm} set to INPUT ({})", pull.short_label()))
            } else {
                run_root_script(&cmd)
            }
        } else {
            run_root_script(&cmd)
        };

        if let Some(pin) = self.pins.get_mut(bcm as usize) {
            pin.direction = PinDirection::Input;
            pin.pull = pull;
        }
        let msg = match &res {
            Ok(m) => m.clone(),
            Err(e) => format!("GPIO{bcm} updated locally ({e})"),
        };
        self.push_log(LogSource::Gpio, "TX", &msg, msg.as_bytes());
        res
    }

    // =========================================================================
    // SERIAL PORT (UART) NON-BLOCKING ENGINE
    // =========================================================================
    pub fn toggle_serial_connection(&mut self) {
        if self.serial_connected {
            self.serial_file = None;
            self.serial_connected = false;
            let msg = format!("{} closed.", self.selected_port);
            self.push_log(LogSource::Serial, "SYS", &msg, b"");
            return;
        }

        match open_nonblocking_serial(&self.selected_port, self.baud_rate) {
            Ok(file) => {
                self.serial_file = Some(file);
                self.serial_connected = true;
                let msg = format!(
                    "Connected to {} at {} baud (8N1).",
                    self.selected_port, self.baud_rate
                );
                self.push_log(LogSource::Serial, "SYS", &msg, b"");
            }
            Err(err) => {
                let msg = format!("Failed to open {}: {err}", self.selected_port);
                self.push_log(LogSource::Serial, "ERR", &msg, b"");
            }
        }
    }

    pub fn poll_serial_rx(&mut self) {
        if !self.serial_connected {
            return;
        }
        let mut buf = [0u8; 512];
        let mut read_len = 0;
        if let Some(file) = &mut self.serial_file {
            if let Ok(n) = file.read(&mut buf) {
                read_len = n;
            }
        }
        if read_len > 0 {
            let slice = &buf[..read_len];
            self.rx_bytes_total += read_len;
            let text = String::from_utf8_lossy(slice)
                .trim_end_matches(&['\r', '\n'][..])
                .to_string();
            self.push_log(LogSource::Serial, "RX", &text, slice);
        }
    }

    /// Sends the serial input line (with the selected line ending) to the open port.
    pub fn send_serial(&mut self) {
        let raw_cmd = self.tx_input.trim_end_matches(['\r', '\n']).to_string();
        if raw_cmd.is_empty() {
            return;
        }
        self.tx_input.clear();
        let payload = format!("{}{}", raw_cmd, self.line_ending.suffix());
        let bytes = payload.into_bytes();
        let result = match self.serial_file.as_mut() {
            Some(file) => file.write_all(&bytes).and_then(|_| file.flush()),
            None => {
                self.push_log(
                    LogSource::Serial,
                    "ERR",
                    "The serial port is closed. Connect first.",
                    b"",
                );
                return;
            }
        };
        match result {
            Ok(()) => {
                self.tx_bytes_total += bytes.len();
                self.push_log(LogSource::Serial, "TX", &raw_cmd, &bytes);
            }
            Err(e) => self.push_log(
                LogSource::Serial,
                "ERR",
                &format!("Serial TX error: {e}"),
                b"",
            ),
        }
    }

    /// Runs the command line of the Log tab.
    pub fn run_command(&mut self) {
        let cmd = self.cmd_input.trim().to_string();
        if cmd.is_empty() {
            return;
        }
        self.cmd_input.clear();
        self.push_log(LogSource::System, "TX", &format!("> {cmd}"), cmd.as_bytes());
        self.execute_interactive_hw_cmd(&cmd);
    }

    /// Changes only the pull resistor, keeping the pin function.
    pub fn set_gpio_pull(&mut self, bcm: u8, pull: PinPull) -> Result<String, String> {
        let p_arg = match pull {
            PinPull::Up => "pu",
            PinPull::Down => "pd",
            PinPull::None => "pn",
        };
        let cmd = format!("/usr/bin/pinctrl set {bcm} {p_arg}");
        let direct = self.hw_access_unlocked
            && Command::new("/usr/bin/pinctrl")
                .args(["set", &bcm.to_string(), p_arg])
                .output()
                .is_ok_and(|o| o.status.success());
        let res = if direct {
            Ok(format!("GPIO{bcm} pull {}", pull.short_label()))
        } else {
            run_root_script(&cmd)
        };
        if res.is_ok() {
            if let Some(pin) = self.pins.get_mut(bcm as usize) {
                pin.pull = pull;
            }
        }
        let (dir, msg) = match &res {
            Ok(m) => ("TX", m.clone()),
            Err(e) => ("ERR", format!("GPIO{bcm} pull not changed: {e}")),
        };
        self.push_log(LogSource::Gpio, dir, &msg, b"");
        res
    }

    pub fn reset_edge_counters(&mut self, bcm: u8) {
        if let Some(pin) = self.pins.get_mut(bcm as usize) {
            pin.rising_edges = 0;
            pin.falling_edges = 0;
        }
    }

    pub fn execute_interactive_hw_cmd(&mut self, cmd: &str) {
        let parts: Vec<&str> = cmd.split_whitespace().collect();
        if parts.is_empty() {
            return;
        }
        match parts[0].to_lowercase().as_str() {
            "help" | "?" => {
                let help = "Available commands: 'scan' (i2cdetect), 'mcu' (dump Argon 0x1a), 'gpio' (read GPIO 0-27), 'fan <0-100>', 'high <bcm>', 'low <bcm>'";
                self.push_log(LogSource::System, "SYS", help, help.as_bytes());
            }
            "scan" | "i2cdetect" => {
                self.probe_i2c_bus();
            }
            "mcu" | "argon" => {
                self.probe_argon_mcu_registers();
            }
            "gpio" | "pins" => {
                self.poll_gpio_states();
                let summary: Vec<String> = self
                    .pins
                    .iter()
                    .take(16)
                    .map(|p| {
                        format!(
                            "GPIO{:02}({}):{}",
                            p.bcm,
                            p.direction.short_label(),
                            if p.level_high { "1" } else { "0" }
                        )
                    })
                    .collect();
                let line = summary.join("  ");
                self.push_log(LogSource::Gpio, "RX", &line, line.as_bytes());
            }
            "fan" if parts.len() >= 2 => {
                if let Ok(spd) = parts[1].parse::<u8>() {
                    match crate::argon::set_immediate_fan_speed(spd) {
                        Ok(m) => self.push_log(
                            LogSource::I2c,
                            "RX",
                            &format!("I2C 0x1a [0x80] <= {spd}% ({m})"),
                            b"",
                        ),
                        Err(e) => self.push_log(LogSource::I2c, "ERR", &e, b""),
                    }
                }
            }
            "high" if parts.len() >= 2 => {
                if let Ok(bcm) = parts[1].parse::<u8>() {
                    let _ = self.set_gpio_output(bcm, true);
                }
            }
            "low" if parts.len() >= 2 => {
                if let Ok(bcm) = parts[1].parse::<u8>() {
                    let _ = self.set_gpio_output(bcm, false);
                }
            }
            _ => {
                self.push_log(LogSource::System,
                    "SYS",
                    "Type 'help' to list diagnostic commands (scan, mcu, gpio, fan <0-100>, high <bcm>, low <bcm>).",
                    b"",
                );
            }
        }
    }

    pub fn probe_i2c_bus(&mut self) {
        let out = Command::new("/usr/sbin/i2cdetect")
            .args(["-y", "1"])
            .output();
        match out {
            Ok(o) if o.status.success() => {
                let text = String::from_utf8_lossy(&o.stdout);
                for line in text.lines() {
                    self.push_log(LogSource::I2c, "RX", line, line.as_bytes());
                }
            }
            _ => {
                if let Ok(res) = run_root_script("/usr/sbin/i2cdetect -y 1") {
                    for line in res.lines() {
                        self.push_log(LogSource::I2c, "RX", line, line.as_bytes());
                    }
                } else {
                    self.push_log(LogSource::I2c,
                        "ERR",
                        "No access to /dev/i2c-1. Enable I2C in Hardware, or use Unlock hardware, then log in again.",
                        b"",
                    );
                }
            }
        }
    }

    pub fn probe_argon_mcu_registers(&mut self) {
        let py = "python3 -c 'import sys; sys.path.append(\"/etc/argon\"); from argonregister import *; b=argonregister_initializebusobj(); \
                  print(f\"MCU 0x1A | Reg 0x80 (Duty): {argonregister_getbyte(b,0x80)}% | Reg 0x81 (FW): 0x{argonregister_getbyte(b,0x81):02X} | Reg 0x86 (Ctrl): 0x{argonregister_getbyte(b,0x86):02X}\")'";
        match run_root_script(py) {
            Ok(line) => {
                self.rx_bytes_total += 4;
                self.push_log(LogSource::I2c, "RX", &line, line.as_bytes());
            }
            Err(err) => {
                self.push_log(LogSource::I2c, "ERR", &err, err.as_bytes());
            }
        }
    }
}

/// Parses output lines of `/usr/bin/pinctrl get 0-27`, e.g.:
/// ` 2: a0    pu | hi // GPIO2 = SDA1`
/// ` 4: ip    pd | lo // GPIO4 = input`
pub fn parse_pinctrl_output(output: &str) -> Vec<(u8, PinDirection, PinPull, bool)> {
    let mut results = Vec::new();
    for line in output.lines() {
        let trimmed = line.trim();
        let Some((num_str, rest)) = trimmed.split_once(':') else {
            continue;
        };
        let Ok(bcm) = num_str.trim().parse::<u8>() else {
            continue;
        };
        let Some((left_pipe, right_pipe)) = rest.split_once('|') else {
            continue;
        };

        let tokens: Vec<&str> = left_pipe.split_whitespace().collect();
        let dir = match tokens.first().copied().unwrap_or("") {
            "ip" => PinDirection::Input,
            "op" => PinDirection::Output,
            "a0" => PinDirection::Alt(0),
            "a1" => PinDirection::Alt(1),
            "a2" => PinDirection::Alt(2),
            "a3" => PinDirection::Alt(3),
            "a4" => PinDirection::Alt(4),
            "a5" => PinDirection::Alt(5),
            _ => PinDirection::Unknown,
        };
        let pull = match tokens.get(1).copied().unwrap_or("") {
            "pu" => PinPull::Up,
            "pd" => PinPull::Down,
            _ => PinPull::None,
        };

        let level_token = right_pipe.split_whitespace().next().unwrap_or("lo");
        let high = level_token == "hi" || level_token == "1";

        results.push((bcm, dir, pull, high));
    }
    results
}

pub fn format_console_entry_payload(entry: &ConsoleEntry, fmt: DataFormat) -> String {
    match fmt {
        DataFormat::Ascii => entry.text.clone(),
        DataFormat::Hex => {
            let bytes = if entry.raw_bytes.is_empty() {
                entry.text.as_bytes()
            } else {
                &entry.raw_bytes
            };
            bytes
                .iter()
                .map(|b| format!("{b:02X}"))
                .collect::<Vec<_>>()
                .join(" ")
        }
        DataFormat::Binary => {
            let bytes = if entry.raw_bytes.is_empty() {
                entry.text.as_bytes()
            } else {
                &entry.raw_bytes
            };
            bytes
                .iter()
                .take(12)
                .map(|b| format!("{b:08b}"))
                .collect::<Vec<_>>()
                .join(" ")
        }
    }
}

fn detect_serial_ports() -> Vec<String> {
    let mut ports = Vec::new();
    for candidate in [
        "/dev/ttyS0",
        "/dev/ttyAMA0",
        "/dev/ttyAMA10",
        "/dev/ttyUSB0",
        "/dev/ttyUSB1",
        "/dev/ttyACM0",
        "/dev/ttyACM1",
    ] {
        if std::path::Path::new(candidate).exists() {
            ports.push(candidate.to_string());
        }
    }
    if ports.is_empty() {
        ports.push("/dev/ttyS0".to_string());
    }
    ports
}

fn open_nonblocking_serial(path: &str, baud: u32) -> Result<fs::File, String> {
    use std::os::unix::fs::OpenOptionsExt;
    let file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(libc::O_NOCTTY | libc::O_NONBLOCK)
        .open(path)
        .map_err(|e| e.to_string())?;

    let fd = file.as_raw_fd();
    unsafe {
        let mut tio: libc::termios = std::mem::zeroed();
        if libc::tcgetattr(fd, &mut tio) == 0 {
            let speed = match baud {
                9600 => libc::B9600,
                19200 => libc::B19200,
                38400 => libc::B38400,
                57600 => libc::B57600,
                230400 => libc::B230400,
                460800 => libc::B460800,
                921600 => libc::B921600,
                _ => libc::B115200,
            };
            libc::cfmakeraw(&mut tio);
            libc::cfsetispeed(&mut tio, speed);
            libc::cfsetospeed(&mut tio, speed);
            tio.c_cflag |= libc::CLOCAL | libc::CREAD;
            libc::tcsetattr(fd, libc::TCSANOW, &tio);
        }
    }
    Ok(file)
}

const SETUP_SCRIPT: &str = include_str!("../packaging/common/setup-permissions.sh");
const SETUP_RULES: &str = include_str!("../packaging/common/60-argon-control-center.rules");

/// Login name of the real user running the app (from the passwd database, not `$USER`).
fn current_username() -> Option<String> {
    // SAFETY: getpwuid returns a pointer into static storage valid until the next call; we copy
    // the name out immediately and the UI thread is the only caller.
    unsafe {
        let pw = libc::getpwuid(libc::getuid());
        if pw.is_null() || (*pw).pw_name.is_null() {
            return None;
        }
        std::ffi::CStr::from_ptr((*pw).pw_name)
            .to_str()
            .ok()
            .map(str::to_owned)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_pinctrl_output() {
        let sample = "\
 2: a0    pu | hi // GPIO2 = SDA1
 3: a0    pu | hi // GPIO3 = SCL1
 4: ip    pd | lo // GPIO4 = input
14: a5    pn | hi // GPIO14 = TXD1
23: op dh pu | hi // GPIO23 = output
";
        let parsed = parse_pinctrl_output(sample);
        assert_eq!(parsed.len(), 5);
        assert_eq!(parsed[0], (2, PinDirection::Alt(0), PinPull::Up, true));
        assert_eq!(parsed[2], (4, PinDirection::Input, PinPull::Down, false));
        assert_eq!(parsed[3], (14, PinDirection::Alt(5), PinPull::None, true));
    }
}
