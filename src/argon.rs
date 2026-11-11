use crate::i18n::Language;
use crate::privileged::{run_root_script, write_privileged_file};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::process::Command;

pub const CPU_FAN_CONF: &str = "/etc/argononed.conf";
pub const HDD_FAN_CONF: &str = "/etc/argononed-hdd.conf";
pub const UNIT_CONF: &str = "/etc/argonunits.conf";
pub const OLED_CONF: &str = "/etc/argoneonoled.conf";

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FanCurvePoint {
    pub temp_c: f32,
    pub speed_pct: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FanPreset {
    Silent,
    BalancedDefault,
    Performance,
    AlwaysOnTurbo,
    Custom,
}

impl FanPreset {
    pub fn label(&self, lang: Language) -> &'static str {
        match (self, lang) {
            (FanPreset::Silent, Language::PtBr) => "Silencioso",
            (FanPreset::Silent, Language::EnUs) => "Silent",
            (FanPreset::BalancedDefault, Language::PtBr) => "Padrão",
            (FanPreset::BalancedDefault, Language::EnUs) => "Standard",
            (FanPreset::Performance, Language::PtBr) => "Desempenho",
            (FanPreset::Performance, Language::EnUs) => "Performance",
            (FanPreset::AlwaysOnTurbo, Language::PtBr) => "Máxima (100%)",
            (FanPreset::AlwaysOnTurbo, Language::EnUs) => "Full speed (100%)",
            (FanPreset::Custom, Language::PtBr) => "Personalizada",
            (FanPreset::Custom, Language::EnUs) => "Custom",
        }
    }

    pub fn points(&self) -> Vec<FanCurvePoint> {
        match self {
            FanPreset::Silent => vec![
                FanCurvePoint {
                    temp_c: 60.0,
                    speed_pct: 30,
                },
                FanCurvePoint {
                    temp_c: 68.0,
                    speed_pct: 55,
                },
                FanCurvePoint {
                    temp_c: 75.0,
                    speed_pct: 100,
                },
            ],
            FanPreset::BalancedDefault => vec![
                FanCurvePoint {
                    temp_c: 55.0,
                    speed_pct: 30,
                },
                FanCurvePoint {
                    temp_c: 60.0,
                    speed_pct: 55,
                },
                FanCurvePoint {
                    temp_c: 65.0,
                    speed_pct: 100,
                },
            ],
            FanPreset::Performance => vec![
                FanCurvePoint {
                    temp_c: 42.0,
                    speed_pct: 35,
                },
                FanCurvePoint {
                    temp_c: 50.0,
                    speed_pct: 65,
                },
                FanCurvePoint {
                    temp_c: 58.0,
                    speed_pct: 85,
                },
                FanCurvePoint {
                    temp_c: 63.0,
                    speed_pct: 100,
                },
            ],
            FanPreset::AlwaysOnTurbo => vec![FanCurvePoint {
                temp_c: 1.0,
                speed_pct: 100,
            }],
            FanPreset::Custom => vec![
                FanCurvePoint {
                    temp_c: 50.0,
                    speed_pct: 30,
                },
                FanCurvePoint {
                    temp_c: 60.0,
                    speed_pct: 60,
                },
                FanCurvePoint {
                    temp_c: 65.0,
                    speed_pct: 100,
                },
            ],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TempUnit {
    Celsius,
    Fahrenheit,
}

impl TempUnit {
    pub fn symbol(&self) -> &'static str {
        match self {
            TempUnit::Celsius => "°C",
            TempUnit::Fahrenheit => "°F",
        }
    }

    pub fn format_temp(&self, temp_c: f32) -> String {
        match self {
            TempUnit::Celsius => format!("{temp_c:.1} °C"),
            TempUnit::Fahrenheit => format!("{:.1} °F", temp_c * 9.0 / 5.0 + 32.0),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OledConfig {
    pub enabled: bool,
    pub switch_duration_sec: u32,
    pub screensaver_sec: u32,
    pub screens: Vec<String>,
}

impl Default for OledConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            switch_duration_sec: 30,
            screensaver_sec: 120,
            screens: vec![
                "clock".to_string(),
                "cpu".to_string(),
                "storage".to_string(),
                "ram".to_string(),
                "temp".to_string(),
                "ip".to_string(),
            ],
        }
    }
}

/// Parses an Argon fan config file (`/etc/argononed.conf` or `/etc/argononed-hdd.conf`).
pub fn parse_fan_config_str(content: &str) -> Vec<FanCurvePoint> {
    let mut points = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if let Some((t_str, s_str)) = trimmed.split_once('=') {
            if let (Ok(temp_c), Ok(speed_f)) =
                (t_str.trim().parse::<f32>(), s_str.trim().parse::<f32>())
            {
                if (0.0..=110.0).contains(&temp_c) && (0.0..=100.0).contains(&speed_f) {
                    points.push(FanCurvePoint {
                        temp_c,
                        speed_pct: speed_f.round().clamp(0.0, 100.0) as u8,
                    });
                }
            }
        }
    }
    points.sort_by(|a, b| {
        a.temp_c
            .partial_cmp(&b.temp_c)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    points
}

pub fn load_fan_curve(path: &str, default_points: Vec<FanCurvePoint>) -> Vec<FanCurvePoint> {
    if let Ok(content) = fs::read_to_string(path) {
        let parsed = parse_fan_config_str(&content);
        if !parsed.is_empty() {
            return parsed;
        }
    }
    default_points
}

pub fn serialize_fan_curve(points: &[FanCurvePoint], label: &str) -> String {
    let mut sorted = points.to_vec();
    sorted.sort_by(|a, b| {
        a.temp_c
            .partial_cmp(&b.temp_c)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut out = String::new();
    out.push_str("#\n");
    out.push_str(&format!("# Argon Fan Speed Configuration {label}\n"));
    out.push_str("# Managed by Argon Control Center\n");
    out.push_str("# Min Temp=Fan Speed\n");
    out.push_str("#\n");
    for pt in sorted {
        let speed = if pt.speed_pct == 0 {
            0
        } else {
            pt.speed_pct.clamp(25, 100)
        };
        if (pt.temp_c - pt.temp_c.round()).abs() < 0.05 {
            out.push_str(&format!("{}={}\n", pt.temp_c.round() as i32, speed));
        } else {
            out.push_str(&format!("{:.1}={}\n", pt.temp_c, speed));
        }
    }
    out
}

/// I2C bus of the Argon fan controller (MCU at 0x1a). It only exists with
/// `dtparam=i2c_arm=on` in config.txt.
pub const ARGON_I2C_BUS: &str = "/dev/i2c-1";

const FAN_BUS_OFF: &str =
    "I2C is off, so the Argon fan controller cannot be reached. Use Enable I2C on the Fan page.";

/// True when the Argon fan controller's I2C bus exists.
pub fn fan_bus_available() -> bool {
    Path::new(ARGON_I2C_BUS).exists()
}

/// Whether the fan bus matters here: an Argon case or the Argon daemon is present.
pub fn fan_bus_expected() -> bool {
    crate::board::current().case != crate::board::ArgonCase::None
        || Path::new("/etc/argon").exists()
}

pub fn save_fan_curve(path: &str, points: &[FanCurvePoint], label: &str) -> Result<String, String> {
    let content = serialize_fan_curve(points, label);
    let saved = write_privileged_file(path, &content, Some("systemctl restart argononed.service"))?;
    if fan_bus_available() {
        Ok(saved)
    } else {
        Err(format!("{path} saved, but {FAN_BUS_OFF}"))
    }
}

/// Enables the I2C bus now and at every boot, then restarts argononed so it opens the bus.
/// config.txt gets `dtparam=i2c_arm=on` (previous file kept as config.txt.argoncc.bak).
pub fn enable_fan_bus() -> Result<String, String> {
    let cfg = crate::overclock::get_boot_config_path();
    let bus = ARGON_I2C_BUS;
    let script = format!(
        r#"set -e
{edit}
if command -v dtparam >/dev/null 2>&1; then dtparam i2c_arm=on || true; fi
modprobe i2c-dev 2>/dev/null || true
for _ in 1 2 3 4 5 6 7 8 9 10; do [ -e {bus} ] && break; sleep 0.2; done
systemctl try-restart argononed.service 2>/dev/null || true
if [ -e {bus} ]; then echo "I2C enabled ({bus}); argononed restarted"; else echo "I2C enabled in $cfg; reboot to apply"; fi
"#,
        edit = enable_i2c_config_script(cfg),
    );
    run_root_script(&script).map(|out| out.trim().to_string())
}

/// Shell commands that make `cfg` contain `dtparam=i2c_arm=on`: uncomment it, or append it
/// under `[all]`. The file is backed up first and left alone when I2C is already enabled.
fn enable_i2c_config_script(cfg: &str) -> String {
    format!(
        r#"cfg='{cfg}'
if ! grep -qE '^[[:space:]]*dtparam=i2c_arm=(on|1)[[:space:]]*$' "$cfg"; then
    cp -p "$cfg" "$cfg.argoncc.bak"
    if grep -qE '^[[:space:]]*#[[:space:]]*dtparam=i2c_arm=(on|1)' "$cfg"; then
        sed -i -E 's/^[[:space:]]*#[[:space:]]*dtparam=i2c_arm=(on|1).*/dtparam=i2c_arm=on/' "$cfg"
    else
        printf '\n[all]\ndtparam=i2c_arm=on\n' >> "$cfg"
    fi
fi"#
    )
}

/// Evaluates the Argon step-function fan curve for a given temperature (`argononed.py` `get_fanspeed` logic).
pub fn evaluate_fan_speed(temp_c: f32, points: &[FanCurvePoint]) -> u8 {
    let mut best_speed = 0u8;
    let mut sorted = points.to_vec();
    sorted.sort_by(|a, b| {
        b.temp_c
            .partial_cmp(&a.temp_c)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    for pt in sorted {
        if temp_c >= pt.temp_c {
            best_speed = if pt.speed_pct == 0 {
                0
            } else {
                pt.speed_pct.clamp(25, 100)
            };
            break;
        }
    }
    best_speed
}

/// Sets the fan speed immediately on the Argon MCU via `/etc/argon/argonregister.py` or I2C.
pub fn set_immediate_fan_speed(speed_pct: u8) -> Result<String, String> {
    if !fan_bus_available() {
        return Err(FAN_BUS_OFF.to_string());
    }
    let clamped = speed_pct.min(100);
    let py_cmd = format!(
        "python3 -c 'import sys; sys.path.append(\"/etc/argon\"); from argonregister import *; b=argonregister_initializebusobj(); argonregister_setfanspeed(b, {clamped})'"
    );
    run_root_script(&py_cmd)
}

pub fn load_temp_unit() -> TempUnit {
    if let Ok(content) = fs::read_to_string(UNIT_CONF) {
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with('#') || trimmed.is_empty() {
                continue;
            }
            if let Some((k, v)) = trimmed.split_once('=') {
                if k.trim() == "temperature" {
                    let val = v.trim().trim_matches('"');
                    if val.eq_ignore_ascii_case("F") {
                        return TempUnit::Fahrenheit;
                    }
                }
            }
        }
    }
    TempUnit::Celsius
}

pub fn save_temp_unit(unit: TempUnit) -> Result<String, String> {
    let u = match unit {
        TempUnit::Celsius => "C",
        TempUnit::Fahrenheit => "F",
    };
    let content = format!("#\n# Argon Unit Configuration\n#\ntemperature={u}\n");
    write_privileged_file(
        UNIT_CONF,
        &content,
        Some("systemctl restart argononed.service"),
    )
}

pub fn load_oled_config() -> OledConfig {
    let mut cfg = OledConfig::default();
    if let Ok(content) = fs::read_to_string(OLED_CONF) {
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            if let Some((k, v)) = trimmed.split_once('=') {
                let key = k.trim();
                let val = v.trim().trim_matches('"');
                match key {
                    "switchduration" => {
                        if let Ok(n) = val.parse::<u32>() {
                            cfg.switch_duration_sec = n;
                        }
                    }
                    "screensaver" => {
                        if let Ok(n) = val.parse::<u32>() {
                            cfg.screensaver_sec = n;
                        }
                    }
                    "screenlist" => {
                        cfg.screens = val.split_whitespace().map(|s| s.to_string()).collect();
                    }
                    "enabled" => {
                        cfg.enabled = !val.eq_ignore_ascii_case("N");
                    }
                    _ => {}
                }
            }
        }
    }
    cfg
}

pub fn save_oled_config(cfg: &OledConfig) -> Result<String, String> {
    let enabled_str = if cfg.enabled { "Y" } else { "N" };
    let screens_str = cfg.screens.join(" ");
    let content = format!(
        "#\n# Argon OLED Configuration\n#\nswitchduration={}\nscreensaver={}\nscreenlist=\"{}\"\nenabled=\"{}\"\n",
        cfg.switch_duration_sec, cfg.screensaver_sec, screens_str, enabled_str
    );
    write_privileged_file(
        OLED_CONF,
        &content,
        Some("systemctl restart argononed.service"),
    )
}

pub fn is_oled_hardware_installed() -> bool {
    Path::new("/etc/argon/argoneonoled.py").exists()
}

pub fn get_argononed_service_status() -> (bool, String) {
    if let Ok(out) = Command::new("systemctl")
        .args(["is-active", "argononed.service"])
        .output()
    {
        let status = String::from_utf8_lossy(&out.stdout).trim().to_string();
        let active = status == "active";
        return (active, status);
    }
    (false, "unknown".to_string())
}

pub fn get_argononed_boot_enabled() -> bool {
    if let Ok(out) = Command::new("systemctl")
        .args(["is-enabled", "argononed.service"])
        .output()
    {
        let status = String::from_utf8_lossy(&out.stdout).trim().to_string();
        return status == "enabled";
    }
    false
}

pub fn control_argononed_service(action: &str) -> Result<String, String> {
    match action {
        "start" | "stop" | "restart" | "enable" | "disable" => {
            run_root_script(&format!("systemctl {action} argononed.service"))
        }
        _ => Err(format!("Invalid service action: {action}")),
    }
}

pub fn apply_argon_eeprom_psu_current() -> Result<String, String> {
    if Path::new("/etc/argon/argonone-eepromconfig.py").exists() {
        run_root_script("/usr/bin/python3 /etc/argon/argonone-eepromconfig.py")
    } else {
        Err("Script /etc/argon/argonone-eepromconfig.py not found.".to_string())
    }
}

pub fn reset_argon_ir_power_button() -> Result<String, String> {
    if Path::new("/etc/argon/argonirdecoder").exists() {
        run_root_script("/usr/bin/python3 /etc/argon/argonirdecoder resetpower")
    } else {
        // Program default Argon IR power code directly via I2C register 0x82 if decoder not yet downloaded
        run_root_script("/etc/argon/argonone-ir <<< $'Y\\n1\\n1\\n' || true")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_i2c_edit(name: &str, content: &str) -> (String, bool) {
        let dir = std::env::temp_dir().join(format!("argoncc-i2c-{}-{name}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let cfg = dir.join("config.txt");
        fs::write(&cfg, content).unwrap();
        let status = Command::new("bash")
            .arg("-c")
            .arg(enable_i2c_config_script(cfg.to_str().unwrap()))
            .status()
            .unwrap();
        assert!(status.success());
        let out = fs::read_to_string(&cfg).unwrap();
        let backup = dir.join("config.txt.argoncc.bak").exists();
        fs::remove_dir_all(&dir).ok();
        (out, backup)
    }

    #[test]
    fn enabling_i2c_uncomments_appends_or_keeps() {
        let (out, backup) = run_i2c_edit("commented", "#dtparam=i2c_arm=on\ndtparam=audio=on\n");
        assert_eq!(out, "dtparam=i2c_arm=on\ndtparam=audio=on\n");
        assert!(backup);

        let (out, backup) = run_i2c_edit("missing", "dtparam=audio=on\n");
        assert_eq!(out, "dtparam=audio=on\n\n[all]\ndtparam=i2c_arm=on\n");
        assert!(backup);

        let (out, backup) = run_i2c_edit("enabled", "dtparam=i2c_arm=on\n");
        assert_eq!(out, "dtparam=i2c_arm=on\n");
        assert!(!backup);
    }

    #[test]
    fn test_parse_and_evaluate_fan_curve() {
        let sample = "#\n# Argon Fan Speed Configuration CPU\n#\n55=30\n60=60\n65=100\n";
        let pts = parse_fan_config_str(sample);
        assert_eq!(pts.len(), 3);
        assert_eq!(pts[0].temp_c, 55.0);
        assert_eq!(pts[0].speed_pct, 30);

        assert_eq!(evaluate_fan_speed(45.0, &pts), 0);
        assert_eq!(evaluate_fan_speed(55.0, &pts), 30);
        assert_eq!(evaluate_fan_speed(59.9, &pts), 30);
        assert_eq!(evaluate_fan_speed(60.0, &pts), 60);
        assert_eq!(evaluate_fan_speed(72.0, &pts), 100);
    }
}
