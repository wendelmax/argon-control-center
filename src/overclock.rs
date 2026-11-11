use crate::board::OcProfile;
use crate::i18n::Language;
use crate::privileged::{run_root_script, write_privileged_file};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// Markers of the block this application manages in `config.txt`.
pub const BLOCK_BEGIN: &str = "# --- BEGIN ARGON CONTROL CENTER ---";
pub const BLOCK_END: &str = "# --- END ARGON CONTROL CENTER ---";
/// Markers written by earlier versions; still recognised so the block is replaced, not duplicated.
const LEGACY_BLOCK_BEGIN: &str = "# --- BEGIN ARGON AORUS CONTROL CENTER ---";
const LEGACY_BLOCK_END: &str = "# --- END ARGON AORUS CONTROL CENTER ---";

fn is_block_begin(line: &str) -> bool {
    line == BLOCK_BEGIN || line == LEGACY_BLOCK_BEGIN
}

fn is_block_end(line: &str) -> bool {
    line == BLOCK_END || line == LEGACY_BLOCK_END
}

pub fn get_boot_config_path() -> &'static str {
    if Path::new("/boot/firmware/config.txt").exists() {
        "/boot/firmware/config.txt"
    } else {
        "/boot/config.txt"
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OcPreset {
    EcoPowerSave,
    StockBoost,
    OcGaming,
    OcExtreme,
    Custom,
}

impl OcPreset {
    /// Label with the frequencies of the detected board's SoC profile.
    pub fn label(&self, lang: Language) -> String {
        self.label_for(lang, &crate::board::current().oc)
    }

    pub fn label_for(&self, lang: Language, p: &OcProfile) -> String {
        let pt = lang == Language::PtBr;
        match self {
            OcPreset::EcoPowerSave => format!("Eco · {} MHz", p.eco_mhz),
            OcPreset::StockBoost => format!(
                "{} · {} MHz",
                if pt { "Padrão" } else { "Stock" },
                p.stock_mhz
            ),
            OcPreset::OcGaming => format!("OC · {} MHz", p.oc_mhz),
            OcPreset::OcExtreme => format!(
                "{} · {} MHz",
                if pt { "Extremo" } else { "Extreme" },
                p.extreme_mhz
            ),
            OcPreset::Custom => (if pt { "Manual" } else { "Custom" }).to_string(),
        }
    }

    /// Compact pill label, e.g. `🚀 2.0G`.
    pub fn short_label(&self) -> String {
        let p = &crate::board::current().oc;
        let ghz = |mhz: u32| {
            let g = mhz as f32 / 1000.0;
            if (g * 10.0).fract() == 0.0 {
                format!("{g:.1}G")
            } else {
                format!("{g:.2}G")
            }
        };
        match self {
            OcPreset::EcoPowerSave => format!("🌱 {}", ghz(p.eco_mhz)),
            OcPreset::StockBoost => format!("⚖ {}", ghz(p.stock_mhz)),
            OcPreset::OcGaming => format!("🚀 {}", ghz(p.oc_mhz)),
            OcPreset::OcExtreme => format!("🔥 {}", ghz(p.extreme_mhz)),
            OcPreset::Custom => "Custom".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BootOcConfig {
    pub arm_freq_mhz: u32,
    pub gpu_freq_mhz: u32,
    pub over_voltage: i32,
    /// Pi 5 (BCM2712) adaptive voltage offset in µV (`over_voltage_delta`).
    pub over_voltage_delta_uv: i32,
    pub arm_boost: bool,
    pub force_turbo: bool,
    pub temp_limit_c: u32,

    // Hardware & Argon feature toggles in config.txt
    pub i2c_arm: bool,
    pub spi: bool,
    pub i2s: bool,
    pub onboard_audio: bool,
    pub enable_uart: bool,
    pub vc4_kms_v3d: bool,
    pub camera_auto_detect: bool,
    pub display_auto_detect: bool,
    pub argon_blstr_dac: bool,
    pub argon_ir_receiver: bool,
}

impl Default for BootOcConfig {
    fn default() -> Self {
        Self::default_for(&crate::board::current().oc)
    }
}

impl BootOcConfig {
    pub fn default_for(p: &OcProfile) -> Self {
        Self {
            arm_freq_mhz: p.stock_mhz,
            gpu_freq_mhz: p.gpu_stock_mhz,
            over_voltage: 0,
            over_voltage_delta_uv: 0,
            arm_boost: true,
            force_turbo: false,
            temp_limit_c: 85,
            i2c_arm: true,
            spi: false,
            i2s: false,
            onboard_audio: true,
            enable_uart: true,
            vc4_kms_v3d: true,
            camera_auto_detect: true,
            display_auto_detect: true,
            argon_blstr_dac: false,
            argon_ir_receiver: false,
        }
    }
}

impl BootOcConfig {
    pub fn apply_preset(&mut self, preset: OcPreset) {
        self.apply_preset_for(preset, &crate::board::current().oc);
    }

    pub fn apply_preset_for(&mut self, preset: OcPreset, p: &OcProfile) {
        let (arm, gpu, ov, ov_delta, boost, temp) = match preset {
            OcPreset::EcoPowerSave => (p.eco_mhz, p.gpu_stock_mhz, 0, 0, false, 80),
            OcPreset::StockBoost => (p.stock_mhz, p.gpu_stock_mhz, 0, 0, true, 85),
            OcPreset::OcGaming => (p.oc_mhz, p.gpu_oc_mhz, p.ov_oc, p.ov_delta_oc_uv, true, 85),
            OcPreset::OcExtreme => (
                p.extreme_mhz,
                p.gpu_extreme_mhz,
                p.ov_extreme,
                p.ov_delta_extreme_uv,
                true,
                85,
            ),
            OcPreset::Custom => return,
        };
        self.arm_freq_mhz = arm;
        self.gpu_freq_mhz = gpu;
        self.over_voltage = ov;
        self.over_voltage_delta_uv = ov_delta;
        self.arm_boost = boost;
        self.force_turbo = false;
        self.temp_limit_c = temp;
    }

    /// Approximate core voltage in Volts for display (`0.88V` base + `0.025V * over_voltage`).
    pub fn estimated_core_voltage(&self) -> f32 {
        (1.10 + (self.over_voltage as f32) * 0.025).clamp(0.80, 1.40)
    }
}

pub fn parse_boot_config_str(content: &str) -> BootOcConfig {
    parse_boot_config_str_for(content, &crate::board::current().oc)
}

pub fn parse_boot_config_str_for(content: &str, profile: &OcProfile) -> BootOcConfig {
    let mut cfg = BootOcConfig {
        i2c_arm: false,
        spi: false,
        i2s: false,
        onboard_audio: false,
        enable_uart: false,
        vc4_kms_v3d: false,
        camera_auto_detect: false,
        display_auto_detect: false,
        argon_blstr_dac: false,
        argon_ir_receiver: false,
        arm_boost: false,
        ..BootOcConfig::default_for(profile)
    };

    for raw_line in content.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        // Strip inline comments
        let clean = line.split('#').next().unwrap_or(line).trim();
        if clean == "dtparam=i2c_arm=on" || clean == "dtparam=i2c_arm=1" {
            cfg.i2c_arm = true;
        } else if clean == "dtparam=spi=on" || clean == "dtparam=spi=1" {
            cfg.spi = true;
        } else if clean == "dtparam=i2s=on" || clean == "dtparam=i2s=1" {
            cfg.i2s = true;
        } else if clean == "dtparam=audio=on" || clean == "dtparam=audio=1" {
            cfg.onboard_audio = true;
        } else if clean.starts_with("dtoverlay=vc4-kms-v3d") {
            cfg.vc4_kms_v3d = true;
        } else if clean == "dtoverlay=hifiberry-dacplus,slave" {
            cfg.argon_blstr_dac = true;
        } else if clean.starts_with("dtoverlay=gpio-ir") {
            cfg.argon_ir_receiver = true;
        } else if let Some((k, v)) = clean.split_once('=') {
            let key = k.trim();
            let val = v.trim();
            match key {
                "arm_freq" => {
                    if let Ok(n) = val.parse::<u32>() {
                        cfg.arm_freq_mhz = n;
                    }
                }
                "gpu_freq" | "v3d_freq" => {
                    if let Ok(n) = val.parse::<u32>() {
                        cfg.gpu_freq_mhz = n;
                    }
                }
                "over_voltage" => {
                    if let Ok(n) = val.parse::<i32>() {
                        cfg.over_voltage = n;
                    }
                }
                "over_voltage_delta" => {
                    if let Ok(n) = val.parse::<i32>() {
                        cfg.over_voltage_delta_uv = n;
                    }
                }
                "arm_boost" => {
                    cfg.arm_boost = val == "1" || val.eq_ignore_ascii_case("on");
                }
                "force_turbo" => {
                    cfg.force_turbo = val == "1" || val.eq_ignore_ascii_case("on");
                }
                "temp_limit" => {
                    if let Ok(n) = val.parse::<u32>() {
                        cfg.temp_limit_c = n;
                    }
                }
                "enable_uart" => {
                    cfg.enable_uart = val == "1" || val.eq_ignore_ascii_case("on");
                }
                "camera_auto_detect" => {
                    cfg.camera_auto_detect = val == "1";
                }
                "display_auto_detect" => {
                    cfg.display_auto_detect = val == "1";
                }
                _ => {}
            }
        }
    }

    // Pi 4: firmware default without arm_freq is 1500 MHz, arm_boost raises it to 1800 MHz.
    if profile.stock_mhz == 1800 && cfg.arm_boost && cfg.arm_freq_mhz == 1500 {
        cfg.arm_freq_mhz = 1800;
    }

    cfg
}

pub fn load_boot_config() -> BootOcConfig {
    let path = get_boot_config_path();
    if let Ok(content) = fs::read_to_string(path) {
        return parse_boot_config_str(&content);
    }
    BootOcConfig::default()
}

/// Updates `config.txt` preserving user comments & sections while updating hardware toggles and the managed overclocking block.
pub fn update_boot_config_str(original: &str, cfg: &BootOcConfig) -> String {
    update_boot_config_str_for(original, cfg, &crate::board::current().oc)
}

pub fn update_boot_config_str_for(
    original: &str,
    cfg: &BootOcConfig,
    profile: &OcProfile,
) -> String {
    let managed_oc_keys = [
        "arm_freq",
        "gpu_freq",
        "over_voltage",
        "over_voltage_delta",
        "force_turbo",
        "temp_limit",
    ];

    let mut out_lines: Vec<String> = Vec::new();
    let mut seen_i2c = false;
    let mut seen_spi = false;
    let mut seen_i2s = false;
    let mut seen_audio = false;
    let mut seen_uart = false;
    let mut seen_vc4 = false;
    let mut seen_cam = false;
    let mut seen_disp = false;
    let mut seen_arm_boost = false;

    let mut in_managed_block = false;

    for raw_line in original.lines() {
        let trimmed = raw_line.trim();
        if is_block_begin(trimmed) {
            in_managed_block = true;
            continue;
        }
        if is_block_end(trimmed) {
            in_managed_block = false;
            continue;
        }
        if in_managed_block {
            continue;
        }

        let uncommented = trimmed.trim_start_matches('#').trim();
        if uncommented == "dtparam=i2c_arm=on" {
            seen_i2c = true;
            out_lines.push(if cfg.i2c_arm {
                "dtparam=i2c_arm=on".to_string()
            } else {
                "#dtparam=i2c_arm=on".to_string()
            });
            continue;
        }
        if uncommented == "dtparam=spi=on" {
            seen_spi = true;
            out_lines.push(if cfg.spi {
                "dtparam=spi=on".to_string()
            } else {
                "#dtparam=spi=on".to_string()
            });
            continue;
        }
        if uncommented == "dtparam=i2s=on" {
            seen_i2s = true;
            out_lines.push(if cfg.i2s {
                "dtparam=i2s=on".to_string()
            } else {
                "#dtparam=i2s=on".to_string()
            });
            continue;
        }
        if uncommented == "dtparam=audio=on" {
            seen_audio = true;
            out_lines.push(if cfg.onboard_audio {
                "dtparam=audio=on".to_string()
            } else {
                "#dtparam=audio=on".to_string()
            });
            continue;
        }
        if uncommented == "dtoverlay=vc4-kms-v3d" {
            seen_vc4 = true;
            out_lines.push(if cfg.vc4_kms_v3d {
                "dtoverlay=vc4-kms-v3d".to_string()
            } else {
                "#dtoverlay=vc4-kms-v3d".to_string()
            });
            continue;
        }
        if uncommented == "dtoverlay=hifiberry-dacplus,slave"
            || uncommented.starts_with("dtoverlay=gpio-ir")
        {
            // Will be emitted cleanly in the managed block if enabled
            continue;
        }

        if !trimmed.starts_with('#') {
            if let Some((k, _)) = trimmed.split_once('=') {
                let key = k.trim();
                if managed_oc_keys.contains(&key) {
                    continue;
                }
                match key {
                    "arm_boost" => {
                        seen_arm_boost = true;
                        out_lines.push(format!("arm_boost={}", if cfg.arm_boost { 1 } else { 0 }));
                        continue;
                    }
                    "enable_uart" => {
                        seen_uart = true;
                        out_lines.push(format!(
                            "enable_uart={}",
                            if cfg.enable_uart { 1 } else { 0 }
                        ));
                        continue;
                    }
                    "camera_auto_detect" => {
                        seen_cam = true;
                        out_lines.push(format!(
                            "camera_auto_detect={}",
                            if cfg.camera_auto_detect { 1 } else { 0 }
                        ));
                        continue;
                    }
                    "display_auto_detect" => {
                        seen_disp = true;
                        out_lines.push(format!(
                            "display_auto_detect={}",
                            if cfg.display_auto_detect { 1 } else { 0 }
                        ));
                        continue;
                    }
                    _ => {}
                }
            }
        }

        out_lines.push(raw_line.to_string());
    }

    // Append managed section at the end (under [all])
    out_lines.push(BLOCK_BEGIN.to_string());
    if !seen_i2c && cfg.i2c_arm {
        out_lines.push("dtparam=i2c_arm=on".to_string());
    }
    if !seen_spi && cfg.spi {
        out_lines.push("dtparam=spi=on".to_string());
    }
    if !seen_i2s && cfg.i2s {
        out_lines.push("dtparam=i2s=on".to_string());
    }
    if !seen_audio && cfg.onboard_audio {
        out_lines.push("dtparam=audio=on".to_string());
    }
    if !seen_vc4 && cfg.vc4_kms_v3d {
        out_lines.push("dtoverlay=vc4-kms-v3d".to_string());
    }
    if !seen_cam {
        out_lines.push(format!(
            "camera_auto_detect={}",
            if cfg.camera_auto_detect { 1 } else { 0 }
        ));
    }
    if !seen_disp {
        out_lines.push(format!(
            "display_auto_detect={}",
            if cfg.display_auto_detect { 1 } else { 0 }
        ));
    }
    if !seen_uart {
        out_lines.push(format!(
            "enable_uart={}",
            if cfg.enable_uart { 1 } else { 0 }
        ));
    }
    if !seen_arm_boost {
        out_lines.push(format!("arm_boost={}", if cfg.arm_boost { 1 } else { 0 }));
    }

    if cfg.argon_blstr_dac {
        out_lines.push("dtoverlay=hifiberry-dacplus,slave".to_string());
    }
    if cfg.argon_ir_receiver {
        out_lines.push("dtoverlay=gpio-ir,gpio_pin=23".to_string());
    }

    // Overclocking parameters
    if cfg.arm_freq_mhz != profile.stock_mhz || !cfg.arm_boost {
        out_lines.push(format!("arm_freq={}", cfg.arm_freq_mhz));
    }
    if cfg.gpu_freq_mhz != profile.gpu_stock_mhz {
        out_lines.push(format!("gpu_freq={}", cfg.gpu_freq_mhz));
    }
    if cfg.over_voltage != 0 {
        out_lines.push(format!("over_voltage={}", cfg.over_voltage));
    }
    if cfg.over_voltage_delta_uv != 0 {
        out_lines.push(format!("over_voltage_delta={}", cfg.over_voltage_delta_uv));
    }
    if cfg.force_turbo {
        out_lines.push("force_turbo=1".to_string());
    }
    if cfg.temp_limit_c != 85 {
        out_lines.push(format!("temp_limit={}", cfg.temp_limit_c));
    }
    out_lines.push(BLOCK_END.to_string());

    out_lines.join("\n") + "\n"
}

impl BootOcConfig {
    /// Copies only the clock / voltage fields from `from`.
    pub fn with_oc_from(mut self, from: &BootOcConfig) -> Self {
        self.arm_freq_mhz = from.arm_freq_mhz;
        self.gpu_freq_mhz = from.gpu_freq_mhz;
        self.over_voltage = from.over_voltage;
        self.over_voltage_delta_uv = from.over_voltage_delta_uv;
        self.arm_boost = from.arm_boost;
        self.force_turbo = from.force_turbo;
        self.temp_limit_c = from.temp_limit_c;
        self
    }

    /// Copies only the hardware interface / overlay toggles from `from`.
    pub fn with_features_from(mut self, from: &BootOcConfig) -> Self {
        self.i2c_arm = from.i2c_arm;
        self.spi = from.spi;
        self.i2s = from.i2s;
        self.onboard_audio = from.onboard_audio;
        self.enable_uart = from.enable_uart;
        self.vc4_kms_v3d = from.vc4_kms_v3d;
        self.camera_auto_detect = from.camera_auto_detect;
        self.display_auto_detect = from.display_auto_detect;
        self.argon_blstr_dac = from.argon_blstr_dac;
        self.argon_ir_receiver = from.argon_ir_receiver;
        self
    }

    /// True when clocks or voltages go beyond the board's factory settings, i.e. the change can
    /// make the system unstable or unbootable.
    pub fn exceeds_stock(&self, p: &OcProfile) -> bool {
        self.arm_freq_mhz > p.stock_mhz
            || self.gpu_freq_mhz > p.gpu_stock_mhz
            || self.over_voltage > 0
            || self.over_voltage_delta_uv > 0
            || self.force_turbo
    }
}

/// Which part of `config.txt` a save touches. Each page only writes its own fields, merged
/// into what is currently in the file, so unsaved edits from another page never leak in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigScope {
    Overclock,
    Features,
}

/// Returns the new `config.txt` content for `scope`, starting from `current_file`.
pub fn render_scoped_config(
    current_file: &str,
    ui_cfg: &BootOcConfig,
    scope: ConfigScope,
) -> String {
    render_scoped_config_for(current_file, ui_cfg, scope, &crate::board::current().oc)
}

pub fn render_scoped_config_for(
    current_file: &str,
    ui_cfg: &BootOcConfig,
    scope: ConfigScope,
    profile: &OcProfile,
) -> String {
    let on_disk = parse_boot_config_str_for(current_file, profile);
    let merged = match scope {
        ConfigScope::Overclock => on_disk.with_oc_from(ui_cfg),
        ConfigScope::Features => on_disk.with_features_from(ui_cfg),
    };
    update_boot_config_str_for(current_file, &merged, profile)
}

/// Writes only the fields of `scope` to `config.txt` (backup kept as `config.txt.argoncc.bak`).
pub fn save_boot_config_scoped(
    ui_cfg: &BootOcConfig,
    scope: ConfigScope,
) -> Result<String, String> {
    let path = get_boot_config_path();
    let original = fs::read_to_string(path).unwrap_or_default();
    let updated = render_scoped_config(&original, ui_cfg, scope);
    write_privileged_file(path, &updated, None)
}

/// Applies runtime CPU governor and min/max frequencies immediately across all cores via sysfs.
pub fn apply_runtime_cpufreq(
    governor: &str,
    min_freq_mhz: u32,
    max_freq_mhz: u32,
    boost: bool,
) -> Result<String, String> {
    let min_khz = min_freq_mhz * 1000;
    let max_khz = max_freq_mhz.max(min_freq_mhz) * 1000;
    let boost_val = if boost { 1 } else { 0 };

    let script = format!(
        "for cpu in /sys/devices/system/cpu/cpu[0-9]*/cpufreq; do \
            if [ -w \"$cpu/scaling_governor\" ] || [ \"$(id -u)\" -eq 0 ]; then \
                echo '{governor}' > \"$cpu/scaling_governor\" 2>/dev/null || true; \
                echo '{min_khz}' > \"$cpu/scaling_min_freq\" 2>/dev/null || true; \
                echo '{max_khz}' > \"$cpu/scaling_max_freq\" 2>/dev/null || true; \
            fi; \
         done; \
         if [ -e /sys/devices/system/cpu/cpufreq/boost ]; then \
            echo '{boost_val}' > /sys/devices/system/cpu/cpufreq/boost 2>/dev/null || true; \
         elif [ -e /sys/devices/system/cpu/cpu0/cpufreq/boost ]; then \
            echo '{boost_val}' > /sys/devices/system/cpu/cpu0/cpufreq/boost 2>/dev/null || true; \
         fi; \
         echo 'CPU Governor ({governor}) e frequências ({min_freq_mhz}-{max_freq_mhz} MHz) aplicados em tempo real!'"
    );
    run_root_script(&script)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::Soc;

    #[test]
    fn test_boot_config_roundtrip() {
        let pi4 = OcProfile::for_soc(Soc::Bcm2711);
        let orig = "dtparam=i2c_arm=on\n#dtparam=spi=on\ndtparam=audio=on\ndtoverlay=vc4-kms-v3d\narm_boost=1\n[all]\nenable_uart=1\n";
        let mut cfg = parse_boot_config_str_for(orig, &pi4);
        assert!(cfg.i2c_arm);
        assert!(!cfg.spi);
        assert!(cfg.arm_boost);
        assert_eq!(cfg.arm_freq_mhz, 1800);

        cfg.apply_preset_for(OcPreset::OcGaming, &pi4);
        cfg.spi = true;
        cfg.argon_blstr_dac = true;

        let new_str = update_boot_config_str_for(orig, &cfg, &pi4);
        let reparsed = parse_boot_config_str_for(&new_str, &pi4);
        assert_eq!(reparsed.arm_freq_mhz, 2000);
        assert_eq!(reparsed.gpu_freq_mhz, 650);
        assert_eq!(reparsed.over_voltage, 4);
        assert!(reparsed.spi);
        assert!(reparsed.argon_blstr_dac);
    }

    #[test]
    fn features_save_never_writes_unsaved_overclock() {
        let pi4 = OcProfile::for_soc(Soc::Bcm2711);
        let file = "#dtparam=i2c_arm=on\narm_boost=1\n";
        // The user clicked an OC preset somewhere (in memory only) and then enabled I2C.
        let mut ui_cfg = parse_boot_config_str_for(file, &pi4);
        ui_cfg.apply_preset_for(OcPreset::OcGaming, &pi4);
        ui_cfg.i2c_arm = true;

        let out = render_scoped_config_for(file, &ui_cfg, ConfigScope::Features, &pi4);
        assert!(out.contains("dtparam=i2c_arm=on"));
        assert!(!out.contains("arm_freq=2000"), "{out}");
        assert!(!out.contains("over_voltage="), "{out}");

        let out = render_scoped_config_for(file, &ui_cfg, ConfigScope::Overclock, &pi4);
        assert!(out.contains("arm_freq=2000"));
        assert!(
            out.contains("#dtparam=i2c_arm=on"),
            "OC save must not enable I2C: {out}"
        );
    }

    #[test]
    fn replaces_block_written_with_legacy_marker() {
        let pi4 = OcProfile::for_soc(Soc::Bcm2711);
        let old = "arm_boost=1\n# --- BEGIN ARGON AORUS CONTROL CENTER ---\narm_freq=2000\nover_voltage=4\n# --- END ARGON AORUS CONTROL CENTER ---\n";
        let cfg = BootOcConfig::default_for(&pi4);
        let out = update_boot_config_str_for(old, &cfg, &pi4);
        assert!(!out.contains("AORUS"), "{out}");
        assert!(
            !out.contains("arm_freq=2000"),
            "old block must be replaced: {out}"
        );
        assert_eq!(out.matches(BLOCK_BEGIN).count(), 1);
    }

    #[test]
    fn exceeds_stock_detects_risky_changes() {
        let pi4 = OcProfile::for_soc(Soc::Bcm2711);
        let mut c = BootOcConfig::default_for(&pi4);
        assert!(!c.exceeds_stock(&pi4));
        c.apply_preset_for(OcPreset::EcoPowerSave, &pi4);
        assert!(!c.exceeds_stock(&pi4));
        c.apply_preset_for(OcPreset::OcGaming, &pi4);
        assert!(c.exceeds_stock(&pi4));
    }

    #[test]
    fn test_pi5_profile_roundtrip() {
        let pi5 = OcProfile::for_soc(Soc::Bcm2712);
        let orig = "dtparam=i2c_arm=on\n";
        let mut cfg = parse_boot_config_str_for(orig, &pi5);
        assert_eq!(cfg.arm_freq_mhz, 2400);
        cfg.apply_preset_for(OcPreset::OcExtreme, &pi5);
        let new_str = update_boot_config_str_for(orig, &cfg, &pi5);
        assert!(new_str.contains("arm_freq=3000"));
        assert!(new_str.contains("over_voltage_delta=100000"));
        assert!(!new_str.contains("over_voltage=")); // Pi 5 keeps adaptive voltage
        let reparsed = parse_boot_config_str_for(&new_str, &pi5);
        assert_eq!(reparsed.over_voltage_delta_uv, 100_000);
        assert_eq!(reparsed.gpu_freq_mhz, 1000);
    }
}
