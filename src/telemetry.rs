use crate::argon::{evaluate_fan_speed, FanCurvePoint};
use std::collections::{HashMap, VecDeque};
use std::fs;
use std::net::UdpSocket;
use std::process::Command;
use std::time::Instant;

pub const HISTORY_LEN: usize = 60;

#[derive(Debug, Clone, Default)]
pub struct CpuStatSample {
    pub total: u64,
    pub idle: u64,
}

#[derive(Debug, Clone)]
pub struct DiskUsageInfo {
    pub device: String,
    pub mount_point: String,
    pub total_gb: f32,
    pub used_gb: f32,
    pub usage_pct: f32,
}

#[derive(Debug, Clone)]
pub struct RaidArrayInfo {
    pub name: String,
    pub level: String,
    pub state: String,
    pub disks: String,
}

#[derive(Debug, Clone)]
pub struct TelemetryState {
    pub board_model: String,
    pub os_name: String,
    pub kernel_version: String,
    pub ip_address: String,
    pub uptime_secs: u64,
    pub load_avg: (f32, f32, f32),

    // CPU
    pub cpu_total_usage_pct: f32,
    pub cpu_core_usage_pct: Vec<(String, f32)>,
    pub cpu_cur_freq_mhz: u32,
    pub cpu_min_freq_mhz: u32,
    pub cpu_max_freq_mhz: u32,
    pub cpu_hw_min_mhz: u32,
    pub cpu_hw_max_mhz: u32,
    pub cpu_governor: String,
    pub available_governors: Vec<String>,
    pub available_frequencies_mhz: Vec<u32>,
    pub cpu_boost_enabled: bool,

    // Thermal & Power
    pub cpu_temp_c: f32,
    pub max_temp_c: f32,
    pub undervoltage_alarm: bool,
    /// Under-voltage events since boot (kernel log at start-up + events seen while running).
    pub undervoltage_events: u32,

    // Fan
    pub target_fan_speed_pct: u8,
    pub manual_fan_override: Option<u8>,
    pub argon_service_active: bool,
    pub argon_service_status: String,
    pub argon_service_enabled_boot: bool,

    // Memory
    pub ram_total_mb: u64,
    pub ram_used_mb: u64,
    pub ram_available_mb: u64,
    pub ram_usage_pct: f32,
    pub swap_total_mb: u64,
    pub swap_used_mb: u64,

    // Storage & RAID
    pub disks: Vec<DiskUsageInfo>,
    pub raid_arrays: Vec<RaidArrayInfo>,

    // Network
    pub net_rx_kbps: f32,
    pub net_tx_kbps: f32,

    // Rolling histories (0..HISTORY_LEN)
    pub temp_history: VecDeque<f32>,
    pub cpu_history: VecDeque<f32>,
    pub fan_history: VecDeque<f32>,
    pub freq_history: VecDeque<f32>,
    pub ram_history: VecDeque<f32>,
    pub net_rx_history: VecDeque<f32>,
    pub net_tx_history: VecDeque<f32>,

    prev_cpu_samples: HashMap<String, CpuStatSample>,
    prev_net_bytes: Option<(u64, u64, Instant)>,
    pub last_slow_refresh: Option<Instant>,
    slow_rx: PendingSlow,
}

impl Default for TelemetryState {
    fn default() -> Self {
        Self::new()
    }
}

impl TelemetryState {
    pub fn new() -> Self {
        let board_model = fs::read_to_string("/proc/device-tree/model")
            .unwrap_or_else(|_| "Raspberry Pi / ARM64 System".to_string())
            .trim_matches('\0')
            .trim()
            .to_string();

        let os_name = read_os_pretty_name();
        let kernel_version = fs::read_to_string("/proc/sys/kernel/osrelease")
            .unwrap_or_else(|_| "Linux".to_string())
            .trim()
            .to_string();

        let mut s = Self {
            board_model,
            os_name,
            kernel_version,
            ip_address: detect_local_ip(),
            uptime_secs: 0,
            load_avg: (0.0, 0.0, 0.0),
            cpu_total_usage_pct: 0.0,
            cpu_core_usage_pct: Vec::new(),
            cpu_cur_freq_mhz: 1800,
            cpu_min_freq_mhz: 600,
            cpu_max_freq_mhz: 1800,
            cpu_hw_min_mhz: 600,
            cpu_hw_max_mhz: 1800,
            cpu_governor: "ondemand".to_string(),
            available_governors: vec![
                "performance".to_string(),
                "ondemand".to_string(),
                "schedutil".to_string(),
                "conservative".to_string(),
                "powersave".to_string(),
            ],
            available_frequencies_mhz: Vec::new(),
            cpu_boost_enabled: true,
            cpu_temp_c: 45.0,
            max_temp_c: 45.0,
            undervoltage_alarm: false,
            undervoltage_events: crate::power::undervoltage_events_since_boot().unwrap_or(0),
            target_fan_speed_pct: 0,
            manual_fan_override: None,
            argon_service_active: true,
            argon_service_status: "active".to_string(),
            argon_service_enabled_boot: true,
            ram_total_mb: 8192,
            ram_used_mb: 1024,
            ram_available_mb: 7168,
            ram_usage_pct: 12.5,
            swap_total_mb: 0,
            swap_used_mb: 0,
            disks: Vec::new(),
            raid_arrays: Vec::new(),
            net_rx_kbps: 0.0,
            net_tx_kbps: 0.0,
            temp_history: VecDeque::from(vec![45.0; HISTORY_LEN]),
            cpu_history: VecDeque::from(vec![0.0; HISTORY_LEN]),
            fan_history: VecDeque::from(vec![0.0; HISTORY_LEN]),
            freq_history: VecDeque::from(vec![1800.0; HISTORY_LEN]),
            ram_history: VecDeque::from(vec![12.0; HISTORY_LEN]),
            net_rx_history: VecDeque::from(vec![0.0; HISTORY_LEN]),
            net_tx_history: VecDeque::from(vec![0.0; HISTORY_LEN]),
            prev_cpu_samples: read_proc_stat(),
            prev_net_bytes: None,
            last_slow_refresh: None,
            slow_rx: PendingSlow(None),
        };
        s.refresh(&[]);
        s
    }

    pub fn refresh(&mut self, active_fan_curve: &[FanCurvePoint]) {
        // 1. CPU Usage from /proc/stat
        let cur_samples = read_proc_stat();
        let mut core_list = Vec::new();
        for (name, cur) in &cur_samples {
            if let Some(prev) = self.prev_cpu_samples.get(name) {
                let total_d = cur.total.saturating_sub(prev.total);
                let idle_d = cur.idle.saturating_sub(prev.idle);
                let pct = if total_d > 0 {
                    ((total_d.saturating_sub(idle_d)) as f32 * 100.0 / (total_d as f32))
                        .clamp(0.0, 100.0)
                } else {
                    0.0
                };
                if name == "cpu" {
                    self.cpu_total_usage_pct = pct;
                } else {
                    core_list.push((name.to_uppercase(), pct));
                }
            }
        }
        core_list.sort_by(|a, b| a.0.cmp(&b.0));
        if !core_list.is_empty() {
            self.cpu_core_usage_pct = core_list;
        }
        self.prev_cpu_samples = cur_samples;

        // 2. CPU Frequency & Governor from sysfs
        let cpufreq_base = "/sys/devices/system/cpu/cpu0/cpufreq";
        if let Some(cur_khz) = read_u32_file(&format!("{cpufreq_base}/scaling_cur_freq"))
            .or_else(|| read_u32_file(&format!("{cpufreq_base}/cpuinfo_cur_freq")))
        {
            self.cpu_cur_freq_mhz = cur_khz / 1000;
        }
        if let Some(min_khz) = read_u32_file(&format!("{cpufreq_base}/scaling_min_freq")) {
            self.cpu_min_freq_mhz = min_khz / 1000;
        }
        if let Some(max_khz) = read_u32_file(&format!("{cpufreq_base}/scaling_max_freq")) {
            self.cpu_max_freq_mhz = max_khz / 1000;
        }
        if let Some(hw_min) = read_u32_file(&format!("{cpufreq_base}/cpuinfo_min_freq")) {
            self.cpu_hw_min_mhz = hw_min / 1000;
        }
        if let Some(hw_max) = read_u32_file(&format!("{cpufreq_base}/cpuinfo_max_freq")) {
            self.cpu_hw_max_mhz = hw_max / 1000;
        }
        if let Ok(gov) = fs::read_to_string(format!("{cpufreq_base}/scaling_governor")) {
            self.cpu_governor = gov.trim().to_string();
        }
        if let Ok(govs) = fs::read_to_string(format!("{cpufreq_base}/scaling_available_governors"))
        {
            let parsed: Vec<String> = govs.split_whitespace().map(|s| s.to_string()).collect();
            if !parsed.is_empty() {
                self.available_governors = parsed;
            }
        }
        if let Ok(freqs) =
            fs::read_to_string(format!("{cpufreq_base}/scaling_available_frequencies"))
        {
            let parsed: Vec<u32> = freqs
                .split_whitespace()
                .filter_map(|s| s.parse::<u32>().ok())
                .map(|khz| khz / 1000)
                .collect();
            if !parsed.is_empty() {
                self.available_frequencies_mhz = parsed;
            }
        }
        if let Some(boost) = read_u32_file(&format!("{cpufreq_base}/boost"))
            .or_else(|| read_u32_file("/sys/devices/system/cpu/cpufreq/boost"))
        {
            self.cpu_boost_enabled = boost > 0;
        }

        // 3. Temperature & Undervoltage
        if let Some(milli_c) = read_u32_file("/sys/class/thermal/thermal_zone0/temp")
            .or_else(|| read_u32_file("/sys/class/hwmon/hwmon0/temp1_input"))
        {
            self.cpu_temp_c = (milli_c as f32) / 1000.0;
            if self.cpu_temp_c > self.max_temp_c {
                self.max_temp_c = self.cpu_temp_c;
            }
        }
        if let Some(alarm) = crate::power::read_alarm() {
            // Count new events (rising edges) on top of what the kernel logged before start-up.
            if alarm && !self.undervoltage_alarm {
                self.undervoltage_events += 1;
            }
            self.undervoltage_alarm = alarm;
        }

        // 4. Target Fan Speed
        let curve_speed = evaluate_fan_speed(self.cpu_temp_c, active_fan_curve);
        self.target_fan_speed_pct = self.manual_fan_override.unwrap_or(curve_speed);

        // 5. Memory from /proc/meminfo
        read_meminfo(self);

        // 6. Uptime & Loadavg
        if let Ok(up) = fs::read_to_string("/proc/uptime") {
            if let Some(first) = up.split_whitespace().next() {
                if let Ok(secs) = first.parse::<f64>() {
                    self.uptime_secs = secs as u64;
                }
            }
        }
        if let Ok(la) = fs::read_to_string("/proc/loadavg") {
            let parts: Vec<&str> = la.split_whitespace().collect();
            if parts.len() >= 3 {
                self.load_avg = (
                    parts[0].parse().unwrap_or(0.0),
                    parts[1].parse().unwrap_or(0.0),
                    parts[2].parse().unwrap_or(0.0),
                );
            }
        }

        // 7. Network throughput
        if let Some((rx, tx)) = read_net_bytes() {
            let now = Instant::now();
            if let Some((prev_rx, prev_tx, prev_time)) = self.prev_net_bytes {
                let dt = now.duration_since(prev_time).as_secs_f32().max(0.1);
                self.net_rx_kbps = (rx.saturating_sub(prev_rx) as f32 / 1024.0) / dt;
                self.net_tx_kbps = (tx.saturating_sub(prev_tx) as f32 / 1024.0) / dt;
            }
            self.prev_net_bytes = Some((rx, tx, now));
        }

        // 8. Push rolling history
        push_history(&mut self.temp_history, self.cpu_temp_c);
        push_history(&mut self.cpu_history, self.cpu_total_usage_pct);
        push_history(&mut self.fan_history, self.target_fan_speed_pct as f32);
        push_history(&mut self.freq_history, self.cpu_cur_freq_mhz as f32);
        push_history(&mut self.ram_history, self.ram_usage_pct);
        push_history(&mut self.net_rx_history, self.net_rx_kbps);
        push_history(&mut self.net_tx_history, self.net_tx_kbps);

        // 9. Slow refresh (every 5 seconds: disks, RAID, service status, IP).
        // `df` and `systemctl` are spawned on a worker thread so the UI never stalls on them;
        // the very first sample is taken synchronously so the dashboard starts populated.
        if let Some(rx) = &self.slow_rx.0 {
            match rx.try_recv() {
                Ok(data) => {
                    self.apply_slow(data);
                    self.slow_rx.0 = None;
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => self.slow_rx.0 = None,
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
            }
        }
        match self.last_slow_refresh {
            None => {
                self.apply_slow(SlowData::collect());
                self.last_slow_refresh = Some(Instant::now());
            }
            Some(t) if t.elapsed().as_secs() >= 5 && self.slow_rx.0.is_none() => {
                let (tx, rx) = std::sync::mpsc::channel();
                std::thread::spawn(move || {
                    let _ = tx.send(SlowData::collect());
                });
                self.slow_rx.0 = Some(rx);
                self.last_slow_refresh = Some(Instant::now());
            }
            _ => {}
        }
    }

    fn apply_slow(&mut self, d: SlowData) {
        self.disks = d.disks;
        self.raid_arrays = d.raid_arrays;
        self.argon_service_active = d.service_active;
        self.argon_service_status = d.service_status;
        self.argon_service_enabled_boot = d.service_enabled_boot;
        self.ip_address = d.ip_address;
    }

    pub fn format_uptime(&self) -> String {
        let d = self.uptime_secs / 86400;
        let h = (self.uptime_secs % 86400) / 3600;
        let m = (self.uptime_secs % 3600) / 60;
        let s = self.uptime_secs % 60;
        if d > 0 {
            format!("{d}d {h:02}h {m:02}m")
        } else {
            format!("{h:02}:{m:02}:{s:02}")
        }
    }
}

fn push_history(buf: &mut VecDeque<f32>, val: f32) {
    if buf.len() >= HISTORY_LEN {
        buf.pop_front();
    }
    buf.push_back(val);
}

fn read_u32_file(path: &str) -> Option<u32> {
    fs::read_to_string(path).ok()?.trim().parse::<u32>().ok()
}

fn read_os_pretty_name() -> String {
    if let Ok(content) = fs::read_to_string("/etc/os-release") {
        for line in content.lines() {
            if let Some(rest) = line.strip_prefix("PRETTY_NAME=") {
                return rest.trim_matches('"').to_string();
            }
        }
    }
    "Linux ARM64".to_string()
}

fn detect_local_ip() -> String {
    if let Ok(sock) = UdpSocket::bind("0.0.0.0:0") {
        if sock.connect("8.8.8.8:80").is_ok() {
            if let Ok(addr) = sock.local_addr() {
                return addr.ip().to_string();
            }
        }
    }
    "127.0.0.1".to_string()
}

fn read_proc_stat() -> HashMap<String, CpuStatSample> {
    let mut map = HashMap::new();
    if let Ok(content) = fs::read_to_string("/proc/stat") {
        for line in content.lines() {
            if !line.starts_with("cpu") {
                continue;
            }
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 5 {
                continue;
            }
            let name = parts[0].to_string();
            let mut total = 0u64;
            let mut idle = 0u64;
            for (idx, val_str) in parts[1..].iter().enumerate() {
                if let Ok(v) = val_str.parse::<u64>() {
                    total += v;
                    if idx == 3 || idx == 4 {
                        idle += v;
                    }
                }
            }
            map.insert(name, CpuStatSample { total, idle });
        }
    }
    map
}

fn read_meminfo(state: &mut TelemetryState) {
    let mut mem_total_kb = 0u64;
    let mut mem_avail_kb = 0u64;
    let mut swap_total_kb = 0u64;
    let mut swap_free_kb = 0u64;

    if let Ok(content) = fs::read_to_string("/proc/meminfo") {
        for line in content.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                if let Ok(val) = parts[1].parse::<u64>() {
                    match parts[0] {
                        "MemTotal:" => mem_total_kb = val,
                        "MemAvailable:" => mem_avail_kb = val,
                        "SwapTotal:" => swap_total_kb = val,
                        "SwapFree:" => swap_free_kb = val,
                        _ => {}
                    }
                }
            }
        }
    }
    if mem_total_kb > 0 {
        let used_kb = mem_total_kb.saturating_sub(mem_avail_kb);
        state.ram_total_mb = mem_total_kb / 1024;
        state.ram_used_mb = used_kb / 1024;
        state.ram_available_mb = mem_avail_kb / 1024;
        state.ram_usage_pct = (used_kb as f32 * 100.0 / mem_total_kb as f32).clamp(0.0, 100.0);
    }
    state.swap_total_mb = swap_total_kb / 1024;
    state.swap_used_mb = swap_total_kb.saturating_sub(swap_free_kb) / 1024;
}

fn read_net_bytes() -> Option<(u64, u64)> {
    let content = fs::read_to_string("/proc/net/dev").ok()?;
    let mut rx_total = 0u64;
    let mut tx_total = 0u64;
    for line in content.lines().skip(2) {
        if let Some((iface, rest)) = line.split_once(':') {
            let iface = iface.trim();
            if iface == "lo" {
                continue;
            }
            let cols: Vec<&str> = rest.split_whitespace().collect();
            if cols.len() >= 9 {
                rx_total += cols[0].parse::<u64>().unwrap_or(0);
                tx_total += cols[8].parse::<u64>().unwrap_or(0);
            }
        }
    }
    Some((rx_total, tx_total))
}

fn read_disks() -> Vec<DiskUsageInfo> {
    let mut disks = Vec::new();
    if let Ok(out) = Command::new("df").args(["-kP"]).output() {
        let text = String::from_utf8_lossy(&out.stdout);
        for line in text.lines().skip(1) {
            let cols: Vec<&str> = line.split_whitespace().collect();
            if cols.len() >= 6 {
                let dev = cols[0];
                let mount = cols[5];
                if !dev.starts_with("/dev/") || dev.starts_with("/dev/loop") {
                    continue;
                }
                let total_kb = cols[1].parse::<f32>().unwrap_or(0.0);
                let used_kb = cols[2].parse::<f32>().unwrap_or(0.0);
                if total_kb > 0.0 {
                    disks.push(DiskUsageInfo {
                        device: dev.to_string(),
                        mount_point: mount.to_string(),
                        total_gb: total_kb / (1024.0 * 1024.0),
                        used_gb: used_kb / (1024.0 * 1024.0),
                        usage_pct: (used_kb * 100.0 / total_kb).clamp(0.0, 100.0),
                    });
                }
            }
        }
    }
    disks
}

fn read_raid_arrays() -> Vec<RaidArrayInfo> {
    let mut arrays = Vec::new();
    if let Ok(content) = fs::read_to_string("/proc/mdstat") {
        for line in content.lines() {
            let cols: Vec<&str> = line.split_whitespace().collect();
            if cols.len() >= 4 && cols[1] == ":" && cols[0] != "Personalities" {
                arrays.push(RaidArrayInfo {
                    name: cols[0].to_string(),
                    state: cols[2].to_string(),
                    level: cols[3].to_string(),
                    disks: cols[4..].join(" "),
                });
            }
        }
    }
    arrays
}

/// Values that need external commands; collected off the UI thread.
struct SlowData {
    disks: Vec<DiskUsageInfo>,
    raid_arrays: Vec<RaidArrayInfo>,
    service_active: bool,
    service_status: String,
    service_enabled_boot: bool,
    ip_address: String,
}

impl SlowData {
    fn collect() -> Self {
        let (service_active, service_status) = crate::argon::get_argononed_service_status();
        SlowData {
            disks: read_disks(),
            raid_arrays: read_raid_arrays(),
            service_active,
            service_status,
            service_enabled_boot: crate::argon::get_argononed_boot_enabled(),
            ip_address: detect_local_ip(),
        }
    }
}

/// In-flight slow sample. A cloned `TelemetryState` simply starts without one.
#[derive(Default)]
struct PendingSlow(Option<std::sync::mpsc::Receiver<SlowData>>);

impl Clone for PendingSlow {
    fn clone(&self) -> Self {
        PendingSlow(None)
    }
}

impl std::fmt::Debug for PendingSlow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(if self.0.is_some() {
            "PendingSlow(pending)"
        } else {
            "PendingSlow(idle)"
        })
    }
}
