//! GPU and firmware telemetry.
//!
//! Sources, each optional (the app shows what is available):
//! - **V3D load** — `…/v3d/gpu_stats` (kernel 6.8+, Raspberry Pi 4 and 5): per-queue job count
//!   and busy time; no privileges needed.
//! - **CMA** — `/proc/meminfo` `CmaTotal` / `CmaFree`: memory pool used by the GPU and display.
//! - **Firmware** — `vcgencmd` through `/dev/vcio` (group `video` once the udev rule from the
//!   packages is installed): GPU / core / H.264 clocks, core voltage, GPU memory split and the
//!   throttling flags.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

/// Busy percentage per V3D queue over the last sampling interval.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct GpuLoad {
    /// Fragment shading / 3D rendering.
    pub render: f32,
    /// Binning (geometry).
    pub bin: f32,
    /// Texture formatting unit.
    pub tfu: f32,
    /// Compute shaders.
    pub csd: f32,
    /// Render jobs per second (roughly frames per second for 3D work).
    pub render_jobs_per_s: f32,
}

impl GpuLoad {
    /// Overall load: the busiest queue.
    pub fn overall(&self) -> f32 {
        self.render.max(self.bin).max(self.tfu).max(self.csd)
    }
}

/// Firmware throttling flags from `vcgencmd get_throttled`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Throttled(pub u32);

impl Throttled {
    pub fn undervoltage_now(&self) -> bool {
        self.0 & (1 << 0) != 0
    }
    pub fn freq_capped_now(&self) -> bool {
        self.0 & (1 << 1) != 0
    }
    pub fn throttled_now(&self) -> bool {
        self.0 & (1 << 2) != 0
    }
    pub fn soft_temp_limit_now(&self) -> bool {
        self.0 & (1 << 3) != 0
    }
    pub fn undervoltage_since_boot(&self) -> bool {
        self.0 & (1 << 16) != 0
    }
    pub fn freq_capped_since_boot(&self) -> bool {
        self.0 & (1 << 17) != 0
    }
    pub fn throttled_since_boot(&self) -> bool {
        self.0 & (1 << 18) != 0
    }
    pub fn soft_temp_limit_since_boot(&self) -> bool {
        self.0 & (1 << 19) != 0
    }
    /// Any problem now or since boot.
    pub fn any(&self) -> bool {
        self.0 & 0x000F_000F != 0
    }
}

/// Values read through `vcgencmd` (all optional).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Firmware {
    pub v3d_mhz: Option<u32>,
    pub core_mhz: Option<u32>,
    pub h264_mhz: Option<u32>,
    pub core_volts: Option<f32>,
    pub gpu_mem_mb: Option<u32>,
    pub throttled: Option<Throttled>,
}

// ---------------------------------------------------------------------------------------------
// V3D gpu_stats
// ---------------------------------------------------------------------------------------------

/// Location of `gpu_stats` (searched once under /sys/devices/platform).
pub fn gpu_stats_path() -> Option<&'static Path> {
    static PATH: OnceLock<Option<PathBuf>> = OnceLock::new();
    PATH.get_or_init(|| find_gpu_stats(Path::new("/sys/devices/platform"), 3))
        .as_deref()
}

fn find_gpu_stats(dir: &Path, depth: u32) -> Option<PathBuf> {
    let entries = fs::read_dir(dir).ok()?;
    let mut subdirs = Vec::new();
    for e in entries.flatten() {
        let p = e.path();
        let name = e.file_name().to_string_lossy().to_string();
        if name.ends_with(".v3d") && p.join("gpu_stats").exists() {
            return Some(p.join("gpu_stats"));
        }
        if depth > 0 && e.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            subdirs.push(p);
        }
    }
    subdirs
        .into_iter()
        .find_map(|d| find_gpu_stats(&d, depth - 1))
}

/// `queue -> (timestamp_ns, jobs, runtime_ns)`.
pub fn parse_gpu_stats(text: &str) -> HashMap<String, (u64, u64, u64)> {
    text.lines()
        .skip(1)
        .filter_map(|l| {
            let f: Vec<&str> = l.split_whitespace().collect();
            if f.len() < 4 {
                return None;
            }
            Some((
                f[0].to_string(),
                (f[1].parse().ok()?, f[2].parse().ok()?, f[3].parse().ok()?),
            ))
        })
        .collect()
}

/// Computes queue load from two `gpu_stats` readings.
pub fn load_between(
    prev: &HashMap<String, (u64, u64, u64)>,
    now: &HashMap<String, (u64, u64, u64)>,
) -> Option<GpuLoad> {
    let busy = |q: &str| -> Option<(f32, f32)> {
        let (t0, j0, r0) = *prev.get(q)?;
        let (t1, j1, r1) = *now.get(q)?;
        let dt = t1.checked_sub(t0)? as f32;
        if dt <= 0.0 {
            return None;
        }
        let pct = (r1.saturating_sub(r0) as f32 / dt * 100.0).clamp(0.0, 100.0);
        let jobs_per_s = j1.saturating_sub(j0) as f32 / (dt / 1e9);
        Some((pct, jobs_per_s))
    };
    let (render, render_jobs_per_s) = busy("render")?;
    Some(GpuLoad {
        render,
        bin: busy("bin").map(|b| b.0).unwrap_or(0.0),
        tfu: busy("tfu").map(|b| b.0).unwrap_or(0.0),
        csd: busy("csd").map(|b| b.0).unwrap_or(0.0),
        render_jobs_per_s,
    })
}

/// Keeps the previous `gpu_stats` reading to compute load as a rate.
#[derive(Debug, Clone, Default)]
pub struct GpuSampler {
    prev: Option<HashMap<String, (u64, u64, u64)>>,
}

impl GpuSampler {
    /// `None` when the kernel has no `gpu_stats` or on the first call.
    pub fn sample(&mut self) -> Option<GpuLoad> {
        let now = parse_gpu_stats(&fs::read_to_string(gpu_stats_path()?).ok()?);
        let load = self.prev.as_ref().and_then(|p| load_between(p, &now));
        self.prev = Some(now);
        load
    }
}

// ---------------------------------------------------------------------------------------------
// CMA
// ---------------------------------------------------------------------------------------------

/// `(used_mb, total_mb)` of the CMA pool, if the kernel reports it.
pub fn cma_usage() -> Option<(u64, u64)> {
    parse_cma(&fs::read_to_string("/proc/meminfo").ok()?)
}

pub fn parse_cma(meminfo: &str) -> Option<(u64, u64)> {
    let kb = |key: &str| -> Option<u64> {
        meminfo
            .lines()
            .find(|l| l.starts_with(key))?
            .split_whitespace()
            .nth(1)?
            .parse()
            .ok()
    };
    let total = kb("CmaTotal:")?;
    let free = kb("CmaFree:")?;
    (total > 0).then(|| (total.saturating_sub(free) / 1024, total / 1024))
}

// ---------------------------------------------------------------------------------------------
// vcgencmd
// ---------------------------------------------------------------------------------------------

fn vcgencmd(args: &[&str]) -> Option<String> {
    let out = Command::new("vcgencmd").args(args).output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
    // Without access to /dev/vcio vcgencmd prints an error but may still exit with 0.
    if !out.status.success() || text.is_empty() || text.contains("Can't open") {
        return None;
    }
    Some(text)
}

/// `frequency(46)=500000000` -> 500 (MHz).
pub fn parse_clock(s: &str) -> Option<u32> {
    let hz: u64 = s.split('=').nth(1)?.trim().parse().ok()?;
    Some((hz / 1_000_000) as u32)
}

/// `throttled=0x50005`.
pub fn parse_throttled(s: &str) -> Option<Throttled> {
    let hex = s.split('=').nth(1)?.trim().trim_start_matches("0x");
    u32::from_str_radix(hex, 16).ok().map(Throttled)
}

/// `volt=0.8500V`.
pub fn parse_volts(s: &str) -> Option<f32> {
    s.split('=')
        .nth(1)?
        .trim()
        .trim_end_matches('V')
        .parse()
        .ok()
}

/// `gpu=76M`.
pub fn parse_mem(s: &str) -> Option<u32> {
    s.split('=')
        .nth(1)?
        .trim()
        .trim_end_matches('M')
        .parse()
        .ok()
}

/// True when `vcgencmd` exists and `/dev/vcio` is accessible to this user.
pub fn firmware_available() -> bool {
    vcgencmd(&["get_throttled"]).is_some()
}

/// Reads every firmware value; runs several processes, so call it off the UI thread.
pub fn read_firmware() -> Firmware {
    // One call tells whether /dev/vcio is usable; skip the rest otherwise.
    let Some(throttled) = read_throttled() else {
        return Firmware::default();
    };
    Firmware {
        v3d_mhz: vcgencmd(&["measure_clock", "v3d"])
            .as_deref()
            .and_then(parse_clock),
        core_mhz: vcgencmd(&["measure_clock", "core"])
            .as_deref()
            .and_then(parse_clock),
        h264_mhz: vcgencmd(&["measure_clock", "h264"])
            .as_deref()
            .and_then(parse_clock),
        core_volts: vcgencmd(&["measure_volts", "core"])
            .as_deref()
            .and_then(parse_volts),
        gpu_mem_mb: vcgencmd(&["get_mem", "gpu"]).as_deref().and_then(parse_mem),
        throttled: Some(throttled),
    }
}

/// Only the throttling flags (one process).
pub fn read_throttled() -> Option<Throttled> {
    vcgencmd(&["get_throttled"])
        .as_deref()
        .and_then(parse_throttled)
}

#[cfg(test)]
mod tests {
    use super::*;

    const STATS_A: &str = "queue\ttimestamp\tjobs\truntime\nbin\t1000000000\t10\t5000000\nrender\t1000000000\t10\t100000000\ntfu\t1000000000\t0\t0\ncsd\t1000000000\t0\t0\n";
    const STATS_B: &str = "queue\ttimestamp\tjobs\truntime\nbin\t2000000000\t40\t15000000\nrender\t2000000000\t40\t350000000\ntfu\t2000000000\t0\t0\ncsd\t2000000000\t0\t0\n";

    #[test]
    fn load_from_two_samples() {
        let l = load_between(&parse_gpu_stats(STATS_A), &parse_gpu_stats(STATS_B)).unwrap();
        assert!((l.render - 25.0).abs() < 0.01, "{l:?}");
        assert!((l.bin - 1.0).abs() < 0.01, "{l:?}");
        assert!((l.render_jobs_per_s - 30.0).abs() < 0.01, "{l:?}");
        assert_eq!(l.overall(), l.render);
    }

    #[test]
    fn firmware_formats() {
        assert_eq!(parse_clock("frequency(46)=500000000"), Some(500));
        assert_eq!(parse_volts("volt=0.8500V"), Some(0.85));
        assert_eq!(parse_mem("gpu=76M"), Some(76));
        let t = parse_throttled("throttled=0x50005").unwrap();
        assert!(t.undervoltage_now() && t.throttled_now());
        assert!(t.undervoltage_since_boot() && t.throttled_since_boot());
        assert!(!t.freq_capped_now() && !t.soft_temp_limit_since_boot());
        assert!(t.any());
        assert!(!Throttled(0).any());
    }

    #[test]
    fn cma_from_meminfo() {
        let m = "MemTotal: 8000000 kB\nCmaTotal:  524288 kB\nCmaFree:   407220 kB\n";
        assert_eq!(parse_cma(m), Some((114, 512)));
        assert_eq!(parse_cma("MemTotal: 1 kB\n"), None);
    }
}
