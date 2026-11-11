//! One-shot system snapshot for desktop widgets, the TUI and scripts (`--json`).
//!
//! Rates (CPU %, network, GPU load) need two readings. Instead of sleeping, the previous raw
//! reading is kept in `$XDG_RUNTIME_DIR/argon-control-center/snapshot-cache.json`, so each call
//! returns immediately; the first call reports zero rates. Values that need extra processes
//! (vcgencmd, systemctl) are cached for `SLOW_TTL_MS` so several widgets polling every couple of
//! seconds do not spawn a pile of processes.

use crate::argon::{evaluate_fan_speed, load_fan_curve, FanPreset, CPU_FAN_CONF};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

const SLOW_TTL_MS: u64 = 10_000;

#[derive(Debug, Clone, Serialize)]
pub struct Snapshot {
    pub board: String,
    pub system: System,
    pub disks: Vec<crate::telemetry::DiskUsageInfo>,
    pub cpu: Cpu,
    pub temp_c: f32,
    /// Temperature formatted with the unit chosen in the app settings.
    pub temp_display: String,
    /// Target fan duty from the configured curve.
    pub fan_pct: u8,
    pub mem: Mem,
    pub swap: Mem,
    pub net: Net,
    pub gpu: Gpu,
    pub power: Power,
    pub daemon_active: bool,
    /// False when the Argon fan controller's I2C bus is missing (fan not controlled).
    pub fan_bus: bool,
    /// GPIO 0-27 from pinctrl; null without access to the pins.
    pub gpio: Option<crate::gpio_lab::GpioState>,
}

#[derive(Debug, Clone, Serialize)]
pub struct System {
    pub os: String,
    pub kernel: String,
    pub uptime_s: u64,
    pub load: [f32; 3],
}

#[derive(Debug, Clone, Serialize)]
pub struct Cpu {
    pub total: f32,
    pub cores: Vec<f32>,
    pub mhz: u32,
    pub governor: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Mem {
    pub used_mb: u64,
    pub total_mb: u64,
    pub pct: f32,
}

#[derive(Debug, Clone, Serialize)]
pub struct Net {
    pub rx_kbps: f32,
    pub tx_kbps: f32,
}

#[derive(Debug, Clone, Serialize)]
pub struct Gpu {
    pub render: Option<f32>,
    pub bin: Option<f32>,
    pub cma_used_mb: Option<u64>,
    pub cma_total_mb: Option<u64>,
    /// Live V3D clock from the firmware, if accessible.
    pub v3d_mhz: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Power {
    pub undervoltage_now: bool,
    /// Raw firmware flags (`vcgencmd get_throttled`), e.g. "0x50000".
    pub throttled: Option<String>,
    /// Localized summary of the firmware flags.
    pub summary: String,
    /// Any under-voltage or throttling now or since boot.
    pub problem: bool,
}

/// Raw counters kept between calls.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Cache {
    t_ms: u64,
    /// name -> (total jiffies, idle jiffies)
    cpu: HashMap<String, (u64, u64)>,
    net: Option<(u64, u64)>,
    gpu: HashMap<String, (u64, u64, u64)>,
    slow_t_ms: u64,
    v3d_mhz: Option<u32>,
    throttled: Option<u32>,
    daemon_active: bool,
    #[serde(default)]
    disks: Vec<crate::telemetry::DiskUsageInfo>,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn cache_path() -> PathBuf {
    let base = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    base.join("argon-control-center")
        .join("snapshot-cache.json")
}

fn read_u32(path: &str) -> Option<u32> {
    fs::read_to_string(path).ok()?.trim().parse().ok()
}

/// `name -> (total, idle)` from /proc/stat (idle includes iowait).
fn read_cpu_times() -> HashMap<String, (u64, u64)> {
    let mut out = HashMap::new();
    for line in fs::read_to_string("/proc/stat").unwrap_or_default().lines() {
        if !line.starts_with("cpu") {
            continue;
        }
        let mut parts = line.split_whitespace();
        let Some(name) = parts.next() else { continue };
        let v: Vec<u64> = parts.filter_map(|x| x.parse().ok()).collect();
        if v.len() < 5 {
            continue;
        }
        let total: u64 = v.iter().sum();
        out.insert(name.to_string(), (total, v[3] + v[4]));
    }
    out
}

fn usage(prev: Option<&(u64, u64)>, now: &(u64, u64)) -> f32 {
    match prev {
        Some(&(pt, pi)) => {
            let dt = now.0.saturating_sub(pt);
            let di = now.1.saturating_sub(pi);
            if dt == 0 {
                0.0
            } else {
                (dt.saturating_sub(di) as f32 * 100.0 / dt as f32).clamp(0.0, 100.0)
            }
        }
        None => 0.0,
    }
}

fn read_net_bytes() -> Option<(u64, u64)> {
    let text = fs::read_to_string("/proc/net/dev").ok()?;
    let mut rx = 0;
    let mut tx = 0;
    for line in text.lines().skip(2) {
        let (iface, rest) = line.split_once(':')?;
        if iface.trim() == "lo" {
            continue;
        }
        let cols: Vec<&str> = rest.split_whitespace().collect();
        if cols.len() >= 9 {
            rx += cols[0].parse::<u64>().unwrap_or(0);
            tx += cols[8].parse::<u64>().unwrap_or(0);
        }
    }
    Some((rx, tx))
}

/// (used MB, total MB) for memory and swap.
fn read_mem() -> (Mem, Mem) {
    let mut kb = HashMap::new();
    for line in fs::read_to_string("/proc/meminfo")
        .unwrap_or_default()
        .lines()
    {
        let mut p = line.split_whitespace();
        if let (Some(k), Some(v)) = (p.next(), p.next()) {
            if let Ok(v) = v.parse::<u64>() {
                kb.insert(k.trim_end_matches(':').to_string(), v);
            }
        }
    }
    let get = |k: &str| kb.get(k).copied().unwrap_or(0);
    let mk = |used_kb: u64, total_kb: u64| Mem {
        used_mb: used_kb / 1024,
        total_mb: total_kb / 1024,
        pct: if total_kb > 0 {
            used_kb as f32 * 100.0 / total_kb as f32
        } else {
            0.0
        },
    };
    let total = get("MemTotal");
    let swap_total = get("SwapTotal");
    (
        mk(total.saturating_sub(get("MemAvailable")), total),
        mk(swap_total.saturating_sub(get("SwapFree")), swap_total),
    )
}

/// Takes a snapshot, updating the rate cache.
pub fn take() -> Snapshot {
    let path = cache_path();
    let prev: Cache = fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    let t = now_ms();
    let dt_s = if prev.t_ms > 0 && t > prev.t_ms {
        (t - prev.t_ms) as f32 / 1000.0
    } else {
        0.0
    };

    // CPU
    let cpu_now = read_cpu_times();
    let total = cpu_now
        .get("cpu")
        .map(|n| usage(prev.cpu.get("cpu"), n))
        .unwrap_or(0.0);
    let mut core_names: Vec<&String> = cpu_now.keys().filter(|k| k.as_str() != "cpu").collect();
    core_names.sort_by_key(|n| {
        n.trim_start_matches("cpu")
            .parse::<u32>()
            .unwrap_or(u32::MAX)
    });
    let cores = core_names
        .iter()
        .map(|n| usage(prev.cpu.get(*n), &cpu_now[*n]))
        .collect();

    // Network
    let net_now = read_net_bytes();
    let (rx_kbps, tx_kbps) = match (prev.net, net_now) {
        (Some((prx, ptx)), Some((rx, tx))) if dt_s > 0.0 => (
            rx.saturating_sub(prx) as f32 / 1024.0 / dt_s,
            tx.saturating_sub(ptx) as f32 / 1024.0 / dt_s,
        ),
        _ => (0.0, 0.0),
    };

    // GPU load
    let gpu_now = crate::gpu::gpu_stats_path()
        .and_then(|p| fs::read_to_string(p).ok())
        .map(|s| crate::gpu::parse_gpu_stats(&s))
        .unwrap_or_default();
    let load = crate::gpu::load_between(&prev.gpu, &gpu_now);
    let cma = crate::gpu::cma_usage();

    // Slow values (processes), refreshed every SLOW_TTL_MS
    let slow_fresh = t.saturating_sub(prev.slow_t_ms) < SLOW_TTL_MS && prev.slow_t_ms > 0;
    let (slow_t_ms, v3d_mhz, throttled, daemon_active, disks) = if slow_fresh {
        (
            prev.slow_t_ms,
            prev.v3d_mhz,
            prev.throttled,
            prev.daemon_active,
            prev.disks.clone(),
        )
    } else {
        let throttled = crate::gpu::read_throttled().map(|th| th.0);
        let v3d = if throttled.is_some() {
            std::process::Command::new("vcgencmd")
                .args(["measure_clock", "v3d"])
                .output()
                .ok()
                .and_then(|o| crate::gpu::parse_clock(String::from_utf8_lossy(&o.stdout).trim()))
        } else {
            None
        };
        (
            t,
            v3d,
            throttled,
            crate::argon::get_argononed_service_status().0,
            crate::telemetry::read_disks(),
        )
    };

    // Temperature, fan target, clock
    let temp_c = read_u32("/sys/class/thermal/thermal_zone0/temp")
        .map(|m| m as f32 / 1000.0)
        .unwrap_or(0.0);
    let curve = load_fan_curve(CPU_FAN_CONF, FanPreset::BalancedDefault.points());
    let settings = crate::settings::load();
    let (mem, swap) = read_mem();

    let undervoltage_now = crate::power::read_alarm().unwrap_or(false);
    let (summary, fw_problem) = throttled
        .map(|th| crate::gpu::throttle_summary(crate::gpu::Throttled(th), settings.language))
        .unwrap_or_default();

    let first = |p: &str| fs::read_to_string(p).unwrap_or_default();
    let loadavg: Vec<f32> = first("/proc/loadavg")
        .split_whitespace()
        .take(3)
        .filter_map(|v| v.parse().ok())
        .collect();
    let snap = Snapshot {
        board: crate::board::current().summary(),
        system: System {
            os: crate::telemetry::read_os_pretty_name(),
            kernel: first("/proc/sys/kernel/osrelease").trim().to_string(),
            uptime_s: first("/proc/uptime")
                .split_whitespace()
                .next()
                .and_then(|v| v.parse::<f64>().ok())
                .unwrap_or(0.0) as u64,
            load: [
                loadavg.first().copied().unwrap_or(0.0),
                loadavg.get(1).copied().unwrap_or(0.0),
                loadavg.get(2).copied().unwrap_or(0.0),
            ],
        },
        disks: disks.clone(),
        cpu: Cpu {
            total,
            cores,
            mhz: read_u32("/sys/devices/system/cpu/cpu0/cpufreq/scaling_cur_freq")
                .map(|k| k / 1000)
                .unwrap_or(0),
            governor: fs::read_to_string("/sys/devices/system/cpu/cpu0/cpufreq/scaling_governor")
                .map(|s| s.trim().to_string())
                .unwrap_or_default(),
        },
        temp_c,
        temp_display: settings.temp_unit.format_temp(temp_c),
        fan_pct: evaluate_fan_speed(temp_c, &curve),
        mem,
        swap,
        net: Net { rx_kbps, tx_kbps },
        gpu: Gpu {
            render: load.map(|l| l.render),
            bin: load.map(|l| l.bin),
            cma_used_mb: cma.map(|c| c.0),
            cma_total_mb: cma.map(|c| c.1),
            v3d_mhz,
        },
        power: Power {
            undervoltage_now,
            throttled: throttled.map(|th| format!("{th:#x}")),
            summary,
            problem: undervoltage_now || fw_problem,
        },
        daemon_active,
        fan_bus: crate::argon::fan_bus_available() || !crate::argon::fan_bus_expected(),
        gpio: crate::gpio_lab::read_gpio_state(),
    };

    let cache = Cache {
        t_ms: t,
        cpu: cpu_now,
        net: net_now,
        gpu: gpu_now,
        slow_t_ms,
        v3d_mhz,
        throttled,
        daemon_active,
        disks,
    };
    if let Some(dir) = path.parent() {
        let _ = fs::create_dir_all(dir);
    }
    if let Ok(json) = serde_json::to_string(&cache) {
        // Write-then-rename: widgets polling at the same time never read a partial file.
        let tmp = path.with_extension(format!("json.{}", std::process::id()));
        if fs::write(&tmp, json).is_ok() {
            let _ = fs::rename(tmp, &path);
        }
    }
    snap
}

/// Snapshot as a single JSON line.
pub fn json() -> String {
    serde_json::to_string(&take()).unwrap_or_else(|_| "{}".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usage_from_two_samples() {
        assert_eq!(usage(None, &(100, 50)), 0.0);
        assert!((usage(Some(&(100, 50)), &(200, 75)) - 75.0).abs() < 0.01);
    }

    #[test]
    fn snapshot_serializes() {
        let j = json();
        let v: serde_json::Value = serde_json::from_str(&j).unwrap();
        assert!(v["cpu"]["cores"].is_array());
        assert!(v["mem"]["total_mb"].as_u64().unwrap() > 0);
    }
}
