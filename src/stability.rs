//! CPU stability test: loads every core while sampling temperature, clock and the 5 V alarm.
//!
//! Progress is written to `stability-last.json` in the settings directory every second, so if
//! the system freezes or reboots mid-test the app can report the last readings afterwards.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

/// Temperature at which the Raspberry Pi 4/5 firmware starts throttling.
pub const THROTTLE_TEMP_C: f32 = 80.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Verdict {
    /// No throttling, no under-voltage and temperature below the throttle point.
    Stable,
    /// The clock dropped below the requested maximum under load.
    Throttled,
    /// Temperature reached the throttle point.
    TooHot,
    /// The 5 V supply dipped during the test.
    Undervoltage,
    /// Stopped by the user before the end.
    Stopped,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Report {
    pub duration_s: u32,
    pub elapsed_s: u32,
    pub threads: u32,
    pub target_mhz: u32,
    pub min_mhz_under_load: u32,
    pub max_temp_c: f32,
    pub last_temp_c: f32,
    pub throttled_samples: u32,
    pub undervoltage_samples: u32,
    /// Firmware throttling flags newly set during the test (bits 16-19 of get_throttled).
    #[serde(default)]
    pub firmware_new_flags: u32,
    pub finished: bool,
    pub verdict: Option<Verdict>,
    /// Trial boot summary when the test ran during a tryboot.
    pub context: String,
}

impl Report {
    fn new(duration_s: u32, threads: u32, target_mhz: u32, context: String) -> Self {
        Report {
            duration_s,
            elapsed_s: 0,
            threads,
            target_mhz,
            min_mhz_under_load: u32::MAX,
            max_temp_c: 0.0,
            last_temp_c: 0.0,
            throttled_samples: 0,
            undervoltage_samples: 0,
            firmware_new_flags: 0,
            finished: false,
            verdict: None,
            context,
        }
    }

    /// Verdict from the collected samples (first problem wins, most serious first).
    pub fn evaluate(&self) -> Verdict {
        let fw = crate::gpu::Throttled(self.firmware_new_flags);
        if self.undervoltage_samples > 0 || fw.undervoltage_since_boot() {
            Verdict::Undervoltage
        } else if self.max_temp_c >= THROTTLE_TEMP_C || fw.soft_temp_limit_since_boot() {
            Verdict::TooHot
        } else if self.throttled_samples >= 3
            || fw.throttled_since_boot()
            || fw.freq_capped_since_boot()
        {
            Verdict::Throttled
        } else {
            Verdict::Stable
        }
    }

    pub fn progress(&self) -> f32 {
        if self.duration_s == 0 {
            0.0
        } else {
            (self.elapsed_s as f32 / self.duration_s as f32).clamp(0.0, 1.0)
        }
    }
}

pub fn report_path() -> Option<PathBuf> {
    crate::settings::path().map(|p| p.with_file_name("stability-last.json"))
}

/// Last saved report (possibly unfinished if the system went down during the test).
pub fn last_report() -> Option<Report> {
    serde_json::from_str(&fs::read_to_string(report_path()?).ok()?).ok()
}

fn save_report(path: Option<&PathBuf>, r: &Report) {
    let Some(p) = path else { return };
    if let Some(dir) = p.parent() {
        let _ = fs::create_dir_all(dir);
    }
    if let Ok(json) = serde_json::to_string_pretty(r) {
        let tmp = p.with_extension("json.tmp");
        if fs::write(&tmp, json).is_ok() {
            let _ = fs::rename(tmp, p);
        }
    }
}

fn read_u32(path: &str) -> Option<u32> {
    fs::read_to_string(path).ok()?.trim().parse().ok()
}

fn cpu_temp_c() -> f32 {
    read_u32("/sys/class/thermal/thermal_zone0/temp")
        .map(|m| m as f32 / 1000.0)
        .unwrap_or(0.0)
}

fn cpu_mhz(file: &str) -> u32 {
    read_u32(&format!("/sys/devices/system/cpu/cpu0/cpufreq/{file}"))
        .map(|k| k / 1000)
        .unwrap_or(0)
}

/// A running test. Dropping it stops the workers.
pub struct StabilityTest {
    stop: Arc<AtomicBool>,
    report: Arc<Mutex<Report>>,
}

impl StabilityTest {
    /// Starts the test and records progress in [`report_path`].
    pub fn start(duration: Duration, context: String) -> Self {
        Self::start_with(duration, context, report_path())
    }

    /// Starts the test, recording progress in `persist` (nothing is written when `None`).
    pub fn start_with(duration: Duration, context: String, persist: Option<PathBuf>) -> Self {
        let threads = thread::available_parallelism()
            .map(|n| n.get() as u32)
            .unwrap_or(4);
        let target = cpu_mhz("scaling_max_freq");
        // Firmware "since boot" flags before the test; new ones set during it count as problems.
        let fw_before = crate::gpu::read_throttled().map(|t| t.0);
        let report = Arc::new(Mutex::new(Report::new(
            duration.as_secs() as u32,
            threads,
            target,
            context,
        )));
        save_report(persist.as_ref(), &report.lock().unwrap());
        let stop = Arc::new(AtomicBool::new(false));

        for _ in 0..threads {
            let stop = stop.clone();
            thread::spawn(move || burn(&stop));
        }

        {
            let stop = stop.clone();
            let report = report.clone();
            let persist = persist.clone();
            thread::spawn(move || {
                let started = Instant::now();
                while !stop.load(Ordering::Relaxed) && started.elapsed() < duration {
                    thread::sleep(Duration::from_secs(1));
                    let temp = cpu_temp_c();
                    let mhz = cpu_mhz("scaling_cur_freq");
                    let uv = crate::power::read_alarm().unwrap_or(false);
                    let mut r = report.lock().unwrap();
                    r.elapsed_s = started.elapsed().as_secs() as u32;
                    r.last_temp_c = temp;
                    r.max_temp_c = r.max_temp_c.max(temp);
                    // Ignore the first seconds while the governor ramps up.
                    if r.elapsed_s >= 3 && mhz > 0 {
                        r.min_mhz_under_load = r.min_mhz_under_load.min(mhz);
                        if r.target_mhz > 0 && (mhz as f32) < r.target_mhz as f32 * 0.95 {
                            r.throttled_samples += 1;
                        }
                    }
                    if uv {
                        r.undervoltage_samples += 1;
                    }
                    save_report(persist.as_ref(), &r);
                }
                let user_stopped = stop.load(Ordering::Relaxed);
                stop.store(true, Ordering::Relaxed);
                let mut r = report.lock().unwrap();
                r.finished = true;
                if let (Some(before), Some(after)) =
                    (fw_before, crate::gpu::read_throttled().map(|t| t.0))
                {
                    r.firmware_new_flags = (after & !before) & 0x000F_0000;
                }
                r.verdict = Some(if user_stopped {
                    Verdict::Stopped
                } else {
                    r.evaluate()
                });
                if r.min_mhz_under_load == u32::MAX {
                    r.min_mhz_under_load = 0;
                }
                save_report(persist.as_ref(), &r);
            });
        }

        StabilityTest { stop, report }
    }

    pub fn stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
    }

    pub fn snapshot(&self) -> Report {
        self.report.lock().unwrap().clone()
    }

    pub fn is_running(&self) -> bool {
        !self.report.lock().unwrap().finished
    }
}

impl Drop for StabilityTest {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

/// Integer and floating-point work that keeps a core fully busy until `stop` is set.
fn burn(stop: &AtomicBool) {
    let mut x: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut f = 1.000_001_f64;
    while !stop.load(Ordering::Relaxed) {
        for _ in 0..200_000 {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            f = (f * 1.000_000_3 + (x & 0xff) as f64 * 1e-9).sqrt() + 0.5;
        }
        std::hint::black_box((x, f));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report() -> Report {
        let mut r = Report::new(60, 4, 1800, String::new());
        r.max_temp_c = 65.0;
        r
    }

    #[test]
    fn verdict_order() {
        let mut r = report();
        assert_eq!(r.evaluate(), Verdict::Stable);
        r.throttled_samples = 3;
        assert_eq!(r.evaluate(), Verdict::Throttled);
        r.max_temp_c = 81.0;
        assert_eq!(r.evaluate(), Verdict::TooHot);
        r.undervoltage_samples = 1;
        assert_eq!(r.evaluate(), Verdict::Undervoltage);
    }

    #[test]
    fn firmware_flags_set_during_test_change_the_verdict() {
        let mut r = report();
        r.firmware_new_flags = 1 << 18; // throttled occurred
        assert_eq!(r.evaluate(), Verdict::Throttled);
        r.firmware_new_flags = 1 << 16; // under-voltage occurred
        assert_eq!(r.evaluate(), Verdict::Undervoltage);
    }

    #[test]
    fn short_run_finishes_and_stops_workers() {
        let t = StabilityTest::start_with(Duration::from_secs(2), String::new(), None);
        let deadline = Instant::now() + Duration::from_secs(10);
        while t.is_running() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(200));
        }
        let r = t.snapshot();
        assert!(r.finished);
        assert!(r.verdict.is_some());
    }
}
