use crate::i18n::Language;
use crate::privileged::{run_root_script, write_privileged_file};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

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
    OcGaming2000,
    OcExtreme2147,
    Custom,
}

impl OcPreset {
    pub fn label(&self, lang: Language) -> &'static str {
        match (self, lang) {
            (OcPreset::EcoPowerSave, Language::PtBr) => "ECO MODE (1500 MHz / Baixo Consumo)",
            (OcPreset::EcoPowerSave, Language::EnUs) => "ECO MODE (1500 MHz / Power Save)",
            (OcPreset::StockBoost, Language::PtBr) => "STOCK BOOST (1800 MHz Padrão RPi4)",
            (OcPreset::StockBoost, Language::EnUs) => "STOCK BOOST (1800 MHz Default RPi4)",
            (OcPreset::OcGaming2000, Language::PtBr) => "AORUS OC GAMING (2000 MHz / 650 GPU)",
            (OcPreset::OcGaming2000, Language::EnUs) => "AORUS OC GAMING (2000 MHz / 650 GPU)",
            (OcPreset::OcExtreme2147, Language::PtBr) => "AORUS EXTREME (2147 MHz / 750 GPU)",
            (OcPreset::OcExtreme2147, Language::EnUs) => "AORUS EXTREME (2147 MHz / 750 GPU)",
            (OcPreset::Custom, Language::PtBr) => "CUSTOM TUNING (Ajuste Manual)",
            (OcPreset::Custom, Language::EnUs) => "CUSTOM TUNING (Manual Setup)",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BootOcConfig {
    pub arm_freq_mhz: u32,
    pub gpu_freq_mhz: u32,
    pub over_voltage: i32,
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
        Self {
            arm_freq_mhz: 1800,
            gpu_freq_mhz: 500,
            over_voltage: 0,
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
        match preset {
            OcPreset::EcoPowerSave => {
                self.arm_freq_mhz = 1500;
                self.gpu_freq_mhz = 500;
                self.over_voltage = 0;
                self.arm_boost = false;
                self.force_turbo = false;
                self.temp_limit_c = 80;
            }
            OcPreset::StockBoost => {
                self.arm_freq_mhz = 1800;
                self.gpu_freq_mhz = 500;
                self.over_voltage = 0;
                self.arm_boost = true;
                self.force_turbo = false;
                self.temp_limit_c = 85;
            }
            OcPreset::OcGaming2000 => {
                self.arm_freq_mhz = 2000;
                self.gpu_freq_mhz = 650;
                self.over_voltage = 4;
                self.arm_boost = true;
                self.force_turbo = false;
                self.temp_limit_c = 85;
            }
            OcPreset::OcExtreme2147 => {
                self.arm_freq_mhz = 2147;
                self.gpu_freq_mhz = 750;
                self.over_voltage = 6;
                self.arm_boost = true;
                self.force_turbo = false;
                self.temp_limit_c = 85;
            }
            OcPreset::Custom => {}
        }
    }

    /// Approximate core voltage in Volts for display (`0.88V` base + `0.025V * over_voltage`).
    pub fn estimated_core_voltage(&self) -> f32 {
        (1.10 + (self.over_voltage as f32) * 0.025).clamp(0.80, 1.40)
    }
}

pub fn parse_boot_config_str(content: &str) -> BootOcConfig {
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
        ..BootOcConfig::default()
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

    if cfg.arm_boost && cfg.arm_freq_mhz == 1500 {
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

/// Updates `config.txt` preserving user comments & sections while updating Hardware Toggles and the AORUS Overclocking block.
pub fn update_boot_config_str(original: &str, cfg: &BootOcConfig) -> String {
    let managed_oc_keys = [
        "arm_freq",
        "gpu_freq",
        "over_voltage",
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
        if trimmed == "# --- BEGIN ARGON AORUS CONTROL CENTER ---" {
            in_managed_block = true;
            continue;
        }
        if trimmed == "# --- END ARGON AORUS CONTROL CENTER ---" {
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
    out_lines.push("# --- BEGIN ARGON AORUS CONTROL CENTER ---".to_string());
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
    if cfg.arm_freq_mhz != 1800 || !cfg.arm_boost {
        out_lines.push(format!("arm_freq={}", cfg.arm_freq_mhz));
    }
    if cfg.gpu_freq_mhz != 500 {
        out_lines.push(format!("gpu_freq={}", cfg.gpu_freq_mhz));
    }
    if cfg.over_voltage != 0 {
        out_lines.push(format!("over_voltage={}", cfg.over_voltage));
    }
    if cfg.force_turbo {
        out_lines.push("force_turbo=1".to_string());
    }
    if cfg.temp_limit_c != 85 {
        out_lines.push(format!("temp_limit={}", cfg.temp_limit_c));
    }
    out_lines.push("# --- END ARGON AORUS CONTROL CENTER ---".to_string());

    out_lines.join("\n") + "\n"
}

pub fn save_boot_config(cfg: &BootOcConfig) -> Result<String, String> {
    let path = get_boot_config_path();
    let original = fs::read_to_string(path).unwrap_or_default();
    let updated = update_boot_config_str(&original, cfg);
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

    #[test]
    fn test_boot_config_roundtrip() {
        let orig = "dtparam=i2c_arm=on\n#dtparam=spi=on\ndtparam=audio=on\ndtoverlay=vc4-kms-v3d\narm_boost=1\n[all]\nenable_uart=1\n";
        let mut cfg = parse_boot_config_str(orig);
        assert!(cfg.i2c_arm);
        assert!(!cfg.spi);
        assert!(cfg.arm_boost);
        assert_eq!(cfg.arm_freq_mhz, 1800);

        cfg.apply_preset(OcPreset::OcGaming2000);
        cfg.spi = true;
        cfg.argon_blstr_dac = true;

        let new_str = update_boot_config_str(orig, &cfg);
        let reparsed = parse_boot_config_str(&new_str);
        assert_eq!(reparsed.arm_freq_mhz, 2000);
        assert_eq!(reparsed.gpu_freq_mhz, 650);
        assert_eq!(reparsed.over_voltage, 4);
        assert!(reparsed.spi);
        assert!(reparsed.argon_blstr_dac);
    }
}
