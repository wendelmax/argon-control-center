//! 5 V supply monitoring.
//!
//! The Raspberry Pi firmware reports under-voltage through the `rpi_volt` hwmon device
//! (`in0_lcrit_alarm`) and the kernel logs `hwmon hwmonN: Undervoltage detected!` each time
//! it happens. Overclocking raises current draw, so a supply that already dips at stock clocks
//! is the most common reason an overclocked Pi stops booting.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

const KERNEL_MESSAGE: &str = "Undervoltage detected";

/// `in0_lcrit_alarm` of the hwmon device named `rpi_volt` (the index varies between boots).
pub fn alarm_path() -> Option<PathBuf> {
    let entries = fs::read_dir("/sys/class/hwmon").ok()?;
    for entry in entries.flatten() {
        let dir = entry.path();
        if fs::read_to_string(dir.join("name")).ok()?.trim() == "rpi_volt" {
            let alarm = dir.join("in0_lcrit_alarm");
            if alarm.exists() {
                return Some(alarm);
            }
        }
    }
    None
}

/// Current under-voltage state, or `None` when the board does not report it.
pub fn read_alarm() -> Option<bool> {
    let p = alarm_path()?;
    let v = fs::read_to_string(p).ok()?;
    Some(v.trim() != "0")
}

/// Number of under-voltage events logged by the kernel since boot. `None` when the kernel log
/// cannot be read (e.g. `kernel.dmesg_restrict=1` and no journal access).
pub fn undervoltage_events_since_boot() -> Option<u32> {
    let run = |cmd: &str, args: &[&str]| {
        Command::new(cmd)
            .args(args)
            .env("LC_ALL", "C")
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
    };
    run("dmesg", &[])
        .or_else(|| run("journalctl", &["-k", "-b", "--no-pager", "-q"]))
        .map(|log| count_events(&log))
}

pub fn count_events(kernel_log: &str) -> u32 {
    kernel_log
        .lines()
        .filter(|l| l.contains(KERNEL_MESSAGE))
        .count() as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_kernel_messages() {
        let log = "[ 31.8] hwmon hwmon1: Undervoltage detected!\n[ 39.8] hwmon hwmon1: Voltage normalised\n[ 50.1] hwmon hwmon1: Undervoltage detected!\n";
        assert_eq!(count_events(log), 2);
        assert_eq!(count_events("nothing here"), 0);
    }
}
