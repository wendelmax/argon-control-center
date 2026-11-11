//! Hardware access self-check (`argon-control-center --doctor`, Settings > About).
//!
//! Runs as the current user, so it reports what the application can actually use.

use std::ffi::CString;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct Check {
    /// What is checked.
    pub name: &'static str,
    /// What it enables in the application.
    pub enables: &'static str,
    pub ok: bool,
    /// How to fix it when it is not available.
    pub fix: &'static str,
}

const SETUP: &str = "sudo /usr/lib/argon-control-center/setup-permissions.sh $USER (or GPIO Lab > Unlock hardware), then log in again";

/// Read + write access for the current user.
fn rw(path: &str) -> bool {
    let Ok(c) = CString::new(path) else {
        return false;
    };
    // SAFETY: access() only reads the NUL-terminated path.
    unsafe { libc::access(c.as_ptr(), libc::R_OK | libc::W_OK) == 0 }
}

fn has_tool(name: &str) -> bool {
    std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).any(|d| d.join(name).is_file()))
        .unwrap_or(false)
}

pub fn run() -> Vec<Check> {
    vec![
        Check {
            name: "I2C bus (/dev/i2c-1)",
            enables: "case detection, fan override, I2C console",
            ok: rw("/dev/i2c-1"),
            fix: SETUP,
        },
        Check {
            name: "GPIO (/dev/gpiochip0)",
            enables: "GPIO lab",
            ok: rw("/dev/gpiochip0"),
            fix: SETUP,
        },
        Check {
            name: "Firmware mailbox (/dev/vcio)",
            enables: "GPU / core clocks, core voltage, throttling flags",
            ok: crate::gpu::firmware_available(),
            fix: "install vcgencmd (raspi-utils-core, libraspberrypi-bin or raspberrypi-utils), then run the permission setup",
        },
        Check {
            name: "V3D GPU statistics",
            enables: "GPU load",
            ok: crate::gpu::gpu_stats_path().is_some(),
            fix: "Linux 6.8 or newer with the v3d driver",
        },
        Check {
            name: "CMA memory statistics",
            enables: "GPU / display memory",
            ok: crate::gpu::cma_usage().is_some(),
            fix: "kernel with CMA enabled",
        },
        Check {
            name: "5 V supply sensor (rpi_volt)",
            enables: "under-voltage alarm",
            ok: crate::power::alarm_path().is_some(),
            fix: "Raspberry Pi kernel with the raspberrypi-hwmon driver",
        },
        Check {
            name: "Kernel log",
            enables: "under-voltage history since boot",
            ok: crate::power::undervoltage_events_since_boot().is_some(),
            fix: "allow reading the kernel log (kernel.dmesg_restrict=0 or systemd-journal group)",
        },
        Check {
            name: "Trial boot (tryboot)",
            enables: "safe overclock testing",
            ok: crate::tryboot::supported(),
            fix: "Raspberry Pi 4 with a 2021+ bootloader EEPROM, or Raspberry Pi 5",
        },
        Check {
            name: "Polkit (pkexec)",
            enables: "saving system files, service control",
            ok: Path::new("/usr/bin/pkexec").exists(),
            fix: "install pkexec / polkit",
        },
        Check {
            name: "i2c-tools",
            enables: "I2C scan",
            ok: has_tool("i2cdetect"),
            fix: "install i2c-tools",
        },
        Check {
            name: "pinctrl",
            enables: "GPIO analyzer",
            ok: has_tool("pinctrl"),
            fix: "install raspi-utils (Raspberry Pi OS)",
        },
    ]
}

/// Plain-text report for the command line.
pub fn report() -> String {
    let checks = run();
    let mut out = String::from("Argon Control Center — hardware access\n\n");
    for c in &checks {
        out.push_str(&format!(
            "  [{}] {:<32} {}\n",
            if c.ok { "ok" } else { "--" },
            c.name,
            c.enables
        ));
        if !c.ok {
            out.push_str(&format!("       fix: {}\n", c.fix));
        }
    }
    let missing = checks.iter().filter(|c| !c.ok).count();
    out.push_str(&format!(
        "\n{} of {} available.\n",
        checks.len() - missing,
        checks.len()
    ));
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn report_lists_every_check() {
        let r = super::report();
        assert!(r.contains("Firmware mailbox"));
        assert!(r.contains("available."));
    }
}
