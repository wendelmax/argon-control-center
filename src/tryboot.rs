//! One-shot trial boot for firmware overclocking (Raspberry Pi 4 and 5 bootloader "tryboot").
//!
//! The candidate configuration is written to `tryboot.txt` next to `config.txt`, and the Pi is
//! rebooted with the `tryboot` flag. The firmware reads `tryboot.txt` for that single boot only:
//! if the system hangs, a power cycle boots the untouched `config.txt` again. When the trial
//! boot works, the user can keep it (copied into `config.txt`) or discard it.
//!
//! A small marker file in the settings directory remembers the trial across reboots so the app
//! can tell the user what happened.

use crate::overclock::{get_boot_config_path, render_scoped_config, BootOcConfig, ConfigScope};
use crate::privileged::{run_root_script, shell_quote, write_privileged_file};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

const BOOTLOADER_TRYBOOT: &str = "/proc/device-tree/chosen/bootloader/tryboot";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Stage {
    /// `tryboot.txt` written, reboot not requested yet.
    Prepared,
    /// Reboot into the trial requested.
    Rebooting,
    /// The trial configuration booted and the app has seen it running.
    Running,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Marker {
    pub stage: Stage,
    /// Human-readable summary of the trial settings, e.g. "2000 MHz · GPU 650 · over_voltage 4".
    pub summary: String,
}

/// What the app should tell the user about the trial boot state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    /// No trial in progress.
    Idle,
    /// `tryboot.txt` is ready; reboot to test it.
    Ready(String),
    /// Currently running the trial configuration.
    InTrial(String),
    /// A trial reboot was requested but the normal configuration booted: the trial did not
    /// come up (or was interrupted by a power cycle).
    Failed(String),
    /// The trial booted earlier, but the system is now back on the normal configuration without
    /// it having been kept (crash during testing, or a normal reboot).
    NotKept(String),
}

/// The bootloader exposes the tryboot flag (Raspberry Pi 4 with a 2021+ EEPROM, Raspberry Pi 5).
pub fn supported() -> bool {
    Path::new(BOOTLOADER_TRYBOOT).exists()
}

/// True while running a trial boot.
pub fn in_trial() -> bool {
    fs::read(BOOTLOADER_TRYBOOT)
        .map(|b| b.iter().any(|&x| x != 0))
        .unwrap_or(false)
}

pub fn tryboot_path() -> PathBuf {
    Path::new(get_boot_config_path()).with_file_name("tryboot.txt")
}

fn marker_path() -> Option<PathBuf> {
    crate::settings::path().map(|p| p.with_file_name("tryboot-trial.json"))
}

fn read_marker() -> Option<Marker> {
    let s = fs::read_to_string(marker_path()?).ok()?;
    serde_json::from_str(&s).ok()
}

fn write_marker(m: Option<&Marker>) {
    let Some(p) = marker_path() else { return };
    match m {
        Some(m) => {
            if let Some(dir) = p.parent() {
                let _ = fs::create_dir_all(dir);
            }
            if let Ok(json) = serde_json::to_string_pretty(m) {
                let _ = fs::write(p, json);
            }
        }
        None => {
            let _ = fs::remove_file(p);
        }
    }
}

pub fn summarize(cfg: &BootOcConfig) -> String {
    let mut s = format!("{} MHz · GPU {} MHz", cfg.arm_freq_mhz, cfg.gpu_freq_mhz);
    if cfg.over_voltage != 0 {
        s.push_str(&format!(" · over_voltage {}", cfg.over_voltage));
    }
    if cfg.over_voltage_delta_uv != 0 {
        s.push_str(&format!(
            " · over_voltage_delta {}",
            cfg.over_voltage_delta_uv
        ));
    }
    if cfg.force_turbo {
        s.push_str(" · force_turbo");
    }
    s
}

/// Works out the trial state from the marker, the bootloader flag and `tryboot.txt`, and
/// advances the marker (Rebooting -> Running) when the trial is seen running.
pub fn status() -> Status {
    let marker = read_marker();
    let pending = tryboot_path().exists();
    if in_trial() {
        let summary = marker.map(|m| m.summary).unwrap_or_default();
        write_marker(Some(&Marker {
            stage: Stage::Running,
            summary: summary.clone(),
        }));
        return Status::InTrial(summary);
    }
    match marker {
        Some(m) if m.stage == Stage::Rebooting => Status::Failed(m.summary),
        Some(m) if m.stage == Stage::Running => Status::NotKept(m.summary),
        Some(m) if pending => Status::Ready(m.summary),
        _ if pending => Status::Ready(String::new()),
        _ => Status::Idle,
    }
}

/// Writes the overclock fields of `ui_cfg` into `tryboot.txt` (based on the current
/// `config.txt`, so everything else stays identical).
pub fn prepare(ui_cfg: &BootOcConfig) -> Result<String, String> {
    let current = fs::read_to_string(get_boot_config_path())
        .map_err(|e| format!("Cannot read {}: {e}", get_boot_config_path()))?;
    let trial = render_scoped_config(&current, ui_cfg, ConfigScope::Overclock);
    let path = tryboot_path();
    let res = write_privileged_file(&path.to_string_lossy(), &trial, None)?;
    write_marker(Some(&Marker {
        stage: Stage::Prepared,
        summary: summarize(ui_cfg),
    }));
    Ok(res)
}

/// Reboots once into `tryboot.txt`.
pub fn reboot_into_trial() -> Result<String, String> {
    if !tryboot_path().exists() {
        return Err("tryboot.txt has not been prepared".into());
    }
    let summary = read_marker().map(|m| m.summary).unwrap_or_default();
    write_marker(Some(&Marker {
        stage: Stage::Rebooting,
        summary,
    }));
    run_root_script("sync; systemctl reboot --reboot-argument='0 tryboot'")
}

/// Makes the running trial permanent: `tryboot.txt` becomes `config.txt` (the previous
/// `config.txt` is kept as `config.txt.argoncc.bak`).
pub fn keep() -> Result<String, String> {
    let trial =
        fs::read_to_string(tryboot_path()).map_err(|e| format!("Cannot read tryboot.txt: {e}"))?;
    let res = write_privileged_file(get_boot_config_path(), &trial, None)?;
    remove_tryboot_file()?;
    write_marker(None);
    Ok(res)
}

/// Removes the trial configuration and forgets the trial.
pub fn discard() -> Result<String, String> {
    let res = if tryboot_path().exists() {
        remove_tryboot_file()?
    } else {
        "No trial configuration to remove.".into()
    };
    write_marker(None);
    Ok(res)
}

fn remove_tryboot_file() -> Result<String, String> {
    run_root_script(&format!(
        "rm -f {}",
        shell_quote(&tryboot_path().to_string_lossy())
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::{OcProfile, Soc};
    use crate::overclock::OcPreset;

    #[test]
    fn summary_lists_risky_fields() {
        let pi4 = OcProfile::for_soc(Soc::Bcm2711);
        let mut c = BootOcConfig::default_for(&pi4);
        c.apply_preset_for(OcPreset::OcGaming, &pi4);
        assert_eq!(summarize(&c), "2000 MHz · GPU 650 MHz · over_voltage 4");
    }

    #[test]
    fn tryboot_file_sits_next_to_config() {
        assert_eq!(tryboot_path().file_name().unwrap(), "tryboot.txt");
    }
}
