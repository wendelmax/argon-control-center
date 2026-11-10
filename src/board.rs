//! Automatic board & Argon case detection.
//!
//! * **Raspberry Pi**: `/proc/device-tree/model`, `compatible` and the revision code
//!   (`/proc/cpuinfo` or `/proc/device-tree/system/linux,revision`) → family, SoC, RAM.
//! * **Argon case**: installed Argon scripts (`/etc/argon/*`), I2C probe of the case MCU
//!   (`0x1a`), OLED (`0x3c`), RTC (`0x51`, Argon EON) and BLSTR DAC (`0x4d`), combined with the
//!   Pi family (Pi 5 → Argon ONE V3).
//! * **Overclock profile**: frequency / voltage presets and slider limits per SoC
//!   (BCM2837, BCM2711, BCM2712).

use std::fs;
use std::path::Path;
use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Soc {
    Bcm2835,
    Bcm2836,
    Bcm2837,
    Bcm2711,
    Bcm2712,
    Unknown,
}

impl Soc {
    pub fn name(&self) -> &'static str {
        match self {
            Soc::Bcm2835 => "BCM2835",
            Soc::Bcm2836 => "BCM2836",
            Soc::Bcm2837 => "BCM2837",
            Soc::Bcm2711 => "BCM2711",
            Soc::Bcm2712 => "BCM2712",
            Soc::Unknown => "ARM SoC",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PiFamily {
    Pi3,
    Pi4,
    Pi400,
    Cm4,
    Pi5,
    Pi500,
    Cm5,
    Zero2,
    OtherPi,
    NotRaspberryPi,
}

impl PiFamily {
    pub fn short(&self) -> &'static str {
        match self {
            PiFamily::Pi3 => "Pi 3",
            PiFamily::Pi4 => "Pi 4",
            PiFamily::Pi400 => "Pi 400",
            PiFamily::Cm4 => "CM4",
            PiFamily::Pi5 => "Pi 5",
            PiFamily::Pi500 => "Pi 500",
            PiFamily::Cm5 => "CM5",
            PiFamily::Zero2 => "Zero 2 W",
            PiFamily::OtherPi => "Raspberry Pi",
            PiFamily::NotRaspberryPi => "Linux",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArgonCase {
    /// Argon ONE / ONE M.2 for Raspberry Pi 4 (MCU at 0x1a).
    OnePi4,
    /// Argon ONE V3 / V3 M.2 for Raspberry Pi 5.
    OneV3,
    /// Argon EON NAS (Pi 4, OLED 0x3c + RTC 0x51).
    Eon,
    /// Argon software installed but the case variant could not be confirmed.
    UnknownArgon,
    None,
}

impl ArgonCase {
    pub fn name(&self) -> &'static str {
        match self {
            ArgonCase::OnePi4 => "Argon ONE (Pi 4)",
            ArgonCase::OneV3 => "Argon ONE V3 (Pi 5)",
            ArgonCase::Eon => "Argon EON",
            ArgonCase::UnknownArgon => "Argon (?)",
            ArgonCase::None => "—",
        }
    }
}

/// Result of probing one I2C address.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Probe {
    Present,
    Absent,
    /// `/dev/i2c-1` not accessible (needs the `i2c` group or root).
    NoAccess,
}

/// Frequency / voltage presets and safe slider ranges for a SoC.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OcProfile {
    pub eco_mhz: u32,
    pub stock_mhz: u32,
    pub oc_mhz: u32,
    pub extreme_mhz: u32,
    pub gpu_stock_mhz: u32,
    pub gpu_oc_mhz: u32,
    pub gpu_extreme_mhz: u32,
    /// `over_voltage` steps (BCM2837/2711).
    pub ov_oc: i32,
    pub ov_extreme: i32,
    /// `over_voltage_delta` in µV (BCM2712 / Pi 5 uses adaptive voltage).
    pub ov_delta_oc_uv: i32,
    pub ov_delta_extreme_uv: i32,
    pub arm_range: (u32, u32),
    pub gpu_range: (u32, u32),
    pub uses_ov_delta: bool,
}

impl OcProfile {
    pub fn for_soc(soc: Soc) -> Self {
        match soc {
            Soc::Bcm2712 => OcProfile {
                eco_mhz: 1800,
                stock_mhz: 2400,
                oc_mhz: 2800,
                extreme_mhz: 3000,
                gpu_stock_mhz: 910,
                gpu_oc_mhz: 960,
                gpu_extreme_mhz: 1000,
                ov_oc: 0,
                ov_extreme: 0,
                ov_delta_oc_uv: 50_000,
                ov_delta_extreme_uv: 100_000,
                arm_range: (1500, 3000),
                gpu_range: (500, 1100),
                uses_ov_delta: true,
            },
            Soc::Bcm2837 | Soc::Bcm2836 | Soc::Bcm2835 => OcProfile {
                eco_mhz: 1000,
                stock_mhz: 1400,
                oc_mhz: 1450,
                extreme_mhz: 1500,
                gpu_stock_mhz: 400,
                gpu_oc_mhz: 450,
                gpu_extreme_mhz: 500,
                ov_oc: 2,
                ov_extreme: 4,
                ov_delta_oc_uv: 0,
                ov_delta_extreme_uv: 0,
                arm_range: (700, 1600),
                gpu_range: (250, 550),
                uses_ov_delta: false,
            },
            // BCM2711 (Pi 4 / 400 / CM4) — also the default for unknown hardware.
            _ => OcProfile {
                eco_mhz: 1500,
                stock_mhz: 1800,
                oc_mhz: 2000,
                extreme_mhz: 2147,
                gpu_stock_mhz: 500,
                gpu_oc_mhz: 650,
                gpu_extreme_mhz: 750,
                ov_oc: 4,
                ov_extreme: 6,
                ov_delta_oc_uv: 0,
                ov_delta_extreme_uv: 0,
                arm_range: (1200, 2200),
                gpu_range: (400, 750),
                uses_ov_delta: false,
            },
        }
    }
}

#[derive(Debug, Clone)]
pub struct BoardInfo {
    pub model: String,
    pub family: PiFamily,
    pub soc: Soc,
    pub revision: Option<u32>,
    pub ram_mb: Option<u32>,
    pub case: ArgonCase,
    pub argon_scripts: bool,
    pub fan_mcu: Probe,
    pub oled: Probe,
    pub rtc: Probe,
    pub dac: Probe,
    pub oc: OcProfile,
}

impl BoardInfo {
    pub fn detect() -> Self {
        let model = read_trimmed("/proc/device-tree/model")
            .unwrap_or_else(|| "Raspberry Pi / ARM64 System".to_string());
        let compatible = fs::read("/proc/device-tree/compatible")
            .map(|b| String::from_utf8_lossy(&b).replace('\0', ","))
            .unwrap_or_default();
        let cpuinfo = fs::read_to_string("/proc/cpuinfo").unwrap_or_default();
        let revision = read_revision(&cpuinfo);

        let scripts = detect_argon_scripts();
        let (fan_mcu, oled, rtc, dac) = probe_i2c_bus1();

        Self::from_parts(
            &model,
            &compatible,
            revision,
            scripts,
            fan_mcu,
            oled,
            rtc,
            dac,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn from_parts(
        model: &str,
        compatible: &str,
        revision: Option<u32>,
        scripts: ArgonScripts,
        fan_mcu: Probe,
        oled: Probe,
        rtc: Probe,
        dac: Probe,
    ) -> Self {
        let decoded = revision.and_then(decode_revision);
        let family = decoded
            .map(|d| d.0)
            .unwrap_or_else(|| family_from_text(model, compatible));
        let soc = decoded
            .map(|d| d.1)
            .filter(|s| *s != Soc::Unknown)
            .unwrap_or_else(|| soc_from_compatible(compatible, family));
        let ram_mb = decoded.and_then(|d| d.2);

        let pi5_class = matches!(family, PiFamily::Pi5 | PiFamily::Pi500 | PiFamily::Cm5);
        let mcu = fan_mcu == Probe::Present;
        let case = if scripts.eon || (rtc == Probe::Present && oled == Probe::Present && !pi5_class)
        {
            ArgonCase::Eon
        } else if (mcu || scripts.one) && pi5_class {
            ArgonCase::OneV3
        } else if (mcu || scripts.one) && matches!(family, PiFamily::Pi4 | PiFamily::Cm4) {
            ArgonCase::OnePi4
        } else if mcu || scripts.one {
            ArgonCase::UnknownArgon
        } else {
            ArgonCase::None
        };

        BoardInfo {
            model: model.to_string(),
            family,
            soc,
            revision,
            ram_mb,
            case,
            argon_scripts: scripts.one || scripts.eon,
            fan_mcu,
            oled,
            rtc,
            dac,
            oc: OcProfile::for_soc(soc),
        }
    }

    /// One-line summary, e.g. `Pi 4 · BCM2711 · 8 GB · Argon ONE (Pi 4)`.
    pub fn summary(&self) -> String {
        let mut s = format!("{} · {}", self.family.short(), self.soc.name());
        if let Some(mb) = self.ram_mb {
            if mb >= 1024 {
                s.push_str(&format!(" · {} GB", mb / 1024));
            } else {
                s.push_str(&format!(" · {mb} MB"));
            }
        }
        if self.case != ArgonCase::None {
            s.push_str(&format!(" · {}", self.case.name()));
        }
        s
    }

    /// Multi-line diagnostic for tooltips.
    pub fn details(&self) -> String {
        let p = |x: Probe| match x {
            Probe::Present => "✔",
            Probe::Absent => "✘",
            Probe::NoAccess => "? (sem acesso / no access /dev/i2c-1)",
        };
        format!(
            "{}\nRevision: {}\nSoC: {}  RAM: {}\nCase: {}\nArgon scripts: {}\nI2C 0x1a MCU: {}\nI2C 0x3c OLED: {}\nI2C 0x51 RTC: {}\nI2C 0x4d DAC: {}\nOC: stock {} MHz · max {} MHz",
            self.model,
            self.revision.map(|r| format!("{r:06x}")).unwrap_or_else(|| "—".into()),
            self.soc.name(),
            self.ram_mb.map(|m| format!("{m} MB")).unwrap_or_else(|| "—".into()),
            self.case.name(),
            if self.argon_scripts { "✔" } else { "✘" },
            p(self.fan_mcu),
            p(self.oled),
            p(self.rtc),
            p(self.dac),
            self.oc.stock_mhz,
            self.oc.extreme_mhz,
        )
    }
}

static BOARD: OnceLock<BoardInfo> = OnceLock::new();

/// Board detected once at startup (cheap afterwards).
pub fn current() -> &'static BoardInfo {
    BOARD.get_or_init(BoardInfo::detect)
}

// ---------------------------------------------------------------------------------------------

fn read_trimmed(path: &str) -> Option<String> {
    let s = fs::read_to_string(path).ok()?;
    let s = s.trim_matches('\0').trim().to_string();
    (!s.is_empty()).then_some(s)
}

fn read_revision(cpuinfo: &str) -> Option<u32> {
    for line in cpuinfo.lines() {
        if let Some((k, v)) = line.split_once(':') {
            if k.trim() == "Revision" {
                return u32::from_str_radix(v.trim(), 16).ok();
            }
        }
    }
    // Device tree fallback: big-endian u32.
    let b = fs::read("/proc/device-tree/system/linux,revision").ok()?;
    (b.len() >= 4).then(|| u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
}

/// Decodes a new-style revision code → (family, soc, ram MB).
/// See raspberrypi.com/documentation/computers/raspberry-pi.html#new-style-revision-codes
pub fn decode_revision(rev: u32) -> Option<(PiFamily, Soc, Option<u32>)> {
    if rev & (1 << 23) == 0 {
        // Old-style code (Pi 1 era).
        return Some((PiFamily::OtherPi, Soc::Bcm2835, None));
    }
    let typ = (rev >> 4) & 0xff;
    let proc_ = (rev >> 12) & 0xf;
    let mem = (rev >> 20) & 0x7;
    let family = match typ {
        0x08 | 0x0d | 0x0e => PiFamily::Pi3,
        0x11 => PiFamily::Pi4,
        0x13 => PiFamily::Pi400,
        0x14 => PiFamily::Cm4,
        0x12 => PiFamily::Zero2,
        0x17 => PiFamily::Pi5,
        0x18 => PiFamily::Cm5,
        0x19 => PiFamily::Pi500,
        _ => PiFamily::OtherPi,
    };
    let soc = match proc_ {
        0 => Soc::Bcm2835,
        1 => Soc::Bcm2836,
        2 => Soc::Bcm2837,
        3 => Soc::Bcm2711,
        4 => Soc::Bcm2712,
        _ => Soc::Unknown,
    };
    let ram = match mem {
        0 => Some(256),
        1 => Some(512),
        2 => Some(1024),
        3 => Some(2048),
        4 => Some(4096),
        5 => Some(8192),
        6 => Some(16384),
        _ => None,
    };
    Some((family, soc, ram))
}

fn family_from_text(model: &str, compatible: &str) -> PiFamily {
    let m = model.to_ascii_lowercase();
    let c = compatible.to_ascii_lowercase();
    if c.contains("raspberrypi,5") || m.contains("raspberry pi 5") {
        PiFamily::Pi5
    } else if m.contains("raspberry pi 500") {
        PiFamily::Pi500
    } else if m.contains("compute module 5") {
        PiFamily::Cm5
    } else if m.contains("raspberry pi 400") {
        PiFamily::Pi400
    } else if m.contains("compute module 4") {
        PiFamily::Cm4
    } else if c.contains("raspberrypi,4") || m.contains("raspberry pi 4") {
        PiFamily::Pi4
    } else if m.contains("zero 2") {
        PiFamily::Zero2
    } else if m.contains("raspberry pi 3") {
        PiFamily::Pi3
    } else if m.contains("raspberry pi") || c.contains("raspberrypi") {
        PiFamily::OtherPi
    } else {
        PiFamily::NotRaspberryPi
    }
}

fn soc_from_compatible(compatible: &str, family: PiFamily) -> Soc {
    let c = compatible.to_ascii_lowercase();
    if c.contains("bcm2712") {
        Soc::Bcm2712
    } else if c.contains("bcm2711") {
        Soc::Bcm2711
    } else if c.contains("bcm2837") {
        Soc::Bcm2837
    } else if c.contains("bcm2836") {
        Soc::Bcm2836
    } else if c.contains("bcm2835") {
        Soc::Bcm2835
    } else {
        match family {
            PiFamily::Pi5 | PiFamily::Pi500 | PiFamily::Cm5 => Soc::Bcm2712,
            PiFamily::Pi4 | PiFamily::Pi400 | PiFamily::Cm4 => Soc::Bcm2711,
            PiFamily::Pi3 | PiFamily::Zero2 => Soc::Bcm2837,
            _ => Soc::Unknown,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ArgonScripts {
    pub one: bool,
    pub eon: bool,
}

fn detect_argon_scripts() -> ArgonScripts {
    let any = |paths: &[&str]| paths.iter().any(|p| Path::new(p).exists());
    ArgonScripts {
        one: any(&[
            "/etc/argon/argononed.py",
            "/usr/bin/argonone-config",
            "/etc/systemd/system/argononed.service",
            "/lib/systemd/system/argononed.service",
        ]),
        eon: any(&[
            "/etc/argon/argoneond.py",
            "/etc/argon/argoneon-rtcconfig.sh",
            "/usr/bin/argoneon-rtcconfig",
            "/etc/systemd/system/argoneond.service",
        ]),
    }
}

/// Probes bus 1 for the Argon devices. Uses kernel-bound sysfs nodes first (no permission needed),
/// then a 1-byte read through `/dev/i2c-1` when the user has access.
fn probe_i2c_bus1() -> (Probe, Probe, Probe, Probe) {
    let addrs = [0x1a_u16, 0x3c, 0x51, 0x4d];
    let mut out = [Probe::NoAccess; 4];

    let dev = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/i2c-1")
        .ok();

    for (i, addr) in addrs.iter().enumerate() {
        if Path::new(&format!("/sys/bus/i2c/devices/1-{addr:04x}")).exists() {
            out[i] = Probe::Present;
            continue;
        }
        if let Some(f) = &dev {
            out[i] = if i2c_read_probe(f, *addr) {
                Probe::Present
            } else {
                Probe::Absent
            };
        }
    }
    (out[0], out[1], out[2], out[3])
}

fn i2c_read_probe(f: &fs::File, addr: u16) -> bool {
    use std::os::unix::io::AsRawFd;
    const I2C_SLAVE: libc::c_ulong = 0x0703;
    let fd = f.as_raw_fd();
    // SAFETY: plain ioctl/read on an owned, open i2c-dev descriptor with a stack buffer.
    unsafe {
        if libc::ioctl(fd, I2C_SLAVE as _, addr as libc::c_ulong) < 0 {
            // EBUSY: address claimed by a kernel driver → device exists.
            return std::io::Error::last_os_error().raw_os_error() == Some(libc::EBUSY);
        }
        let mut b = [0u8; 1];
        libc::read(fd, b.as_mut_ptr() as *mut libc::c_void, 1) == 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_pi4_8gb() {
        // d03114 = Pi 4 Model B Rev 1.4, 8 GB, Sony UK
        let (fam, soc, ram) = decode_revision(0xd03114).unwrap();
        assert_eq!(fam, PiFamily::Pi4);
        assert_eq!(soc, Soc::Bcm2711);
        assert_eq!(ram, Some(8192));
    }

    #[test]
    fn decode_pi5_4gb() {
        // c04170 = Pi 5 Model B Rev 1.0, 4 GB
        let (fam, soc, ram) = decode_revision(0xc04170).unwrap();
        assert_eq!(fam, PiFamily::Pi5);
        assert_eq!(soc, Soc::Bcm2712);
        assert_eq!(ram, Some(4096));
    }

    #[test]
    fn case_inference() {
        let one = ArgonScripts {
            one: true,
            eon: false,
        };
        let b = BoardInfo::from_parts(
            "Raspberry Pi 5 Model B Rev 1.0",
            "raspberrypi,5-model-b,brcm,bcm2712",
            Some(0xc04170),
            one,
            Probe::NoAccess,
            Probe::NoAccess,
            Probe::NoAccess,
            Probe::NoAccess,
        );
        assert_eq!(b.case, ArgonCase::OneV3);
        assert_eq!(b.oc.stock_mhz, 2400);

        let b = BoardInfo::from_parts(
            "Raspberry Pi 4 Model B Rev 1.4",
            "raspberrypi,4-model-b,brcm,bcm2711",
            Some(0xd03114),
            ArgonScripts::default(),
            Probe::Present,
            Probe::Present,
            Probe::Present,
            Probe::Absent,
        );
        assert_eq!(b.case, ArgonCase::Eon);

        let b = BoardInfo::from_parts(
            "Generic",
            "",
            None,
            ArgonScripts::default(),
            Probe::Absent,
            Probe::Absent,
            Probe::Absent,
            Probe::Absent,
        );
        assert_eq!(b.case, ArgonCase::None);
        assert_eq!(b.family, PiFamily::NotRaspberryPi);
    }

    /// `cargo test print_board -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn print_board() {
        let b = BoardInfo::detect();
        println!("{}\n\n{}", b.summary(), b.details());
    }
}
