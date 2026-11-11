pub mod argon;
pub mod board;
pub mod gpio_lab;
pub mod i18n;
pub mod overclock;
pub mod pinout;
pub mod power;
pub mod privileged;
pub mod settings;
pub mod stability;
pub mod system_colors;
pub mod telemetry;
pub mod theme;
pub mod tryboot;
pub mod ui;

use crate::argon::{load_fan_curve, FanPreset, CPU_FAN_CONF};
use crate::i18n::Language;
use crate::overclock::load_boot_config;
use crate::telemetry::TelemetryState;
use crate::ui::ArgonControlCenterApp;
use eframe::egui;

fn main() -> eframe::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    // English is the default; PT-BR is available from the UI or via --pt.
    // A command-line flag overrides the language saved in settings.json.
    let lang_override = if args
        .iter()
        .any(|a| a == "--pt" || a == "--lang=pt" || a == "--lang=pt-br")
    {
        Some(Language::PtBr)
    } else if args.iter().any(|a| a == "--en" || a == "--lang=en") {
        Some(Language::EnUs)
    } else {
        None
    };

    if let Some(pos) = args.iter().position(|a| a == "--stress") {
        let secs: u64 = args.get(pos + 1).and_then(|v| v.parse().ok()).unwrap_or(60);
        return run_stress_cli(secs);
    }

    if args.iter().any(|a| a == "--status" || a == "-s") {
        let curve = load_fan_curve(CPU_FAN_CONF, FanPreset::BalancedDefault.points());
        let mut tel = TelemetryState::new();
        std::thread::sleep(std::time::Duration::from_millis(250));
        tel.refresh(&curve);
        let oc = load_boot_config();
        println!("=== ARGON CONTROL CENTER — TELEMETRY ===");
        let board = crate::board::current();
        println!("Board         : {}", tel.board_model);
        println!("Detected      : {}", board.summary());
        println!("OS / Kernel   : {} ({})", tel.os_name, tel.kernel_version);
        println!(
            "CPU Clock     : {} MHz (Scale: {}-{} MHz, Governor: {})",
            tel.cpu_cur_freq_mhz, tel.cpu_min_freq_mhz, tel.cpu_max_freq_mhz, tel.cpu_governor
        );
        println!(
            "CPU Temp      : {:.1} °C | Target Fan: {}% | Argononed: {}",
            tel.cpu_temp_c, tel.target_fan_speed_pct, tel.argon_service_status
        );
        println!(
            "Power         : {} under-voltage event(s) since boot{}",
            tel.undervoltage_events,
            if tel.undervoltage_alarm {
                " (active now)"
            } else {
                ""
            }
        );
        println!(
            "RAM Usage     : {} / {} MB ({:.1}%)",
            tel.ram_used_mb, tel.ram_total_mb, tel.ram_usage_pct
        );
        println!(
            "Boot OC       : ARM={} MHz, GPU={} MHz, OverVoltage={}, OV delta={} µV, Boost={}",
            oc.arm_freq_mhz,
            oc.gpu_freq_mhz,
            oc.over_voltage,
            oc.over_voltage_delta_uv,
            oc.arm_boost
        );
        return Ok(());
    }

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Argon Control Center")
            .with_inner_size([1160.0, 750.0])
            .with_min_inner_size([480.0, 400.0]),
        ..Default::default()
    };

    eframe::run_native(
        "argon-control-center",
        native_options,
        Box::new(move |cc| Ok(Box::new(ArgonControlCenterApp::new(cc, lang_override)))),
    )
}

/// `--stress <seconds>`: headless stability test with a final verdict (exit code 1 if unstable).
fn run_stress_cli(secs: u64) -> eframe::Result<()> {
    use crate::stability::{StabilityTest, Verdict};
    let test = StabilityTest::start(std::time::Duration::from_secs(secs), "cli".into());
    println!("Stability test: {secs} s on all cores (Ctrl+C to abort)");
    while test.is_running() {
        std::thread::sleep(std::time::Duration::from_secs(5));
        let r = test.snapshot();
        println!(
            "  {:>4}/{} s  temp {:.1} °C (max {:.1})  min clock {} MHz  under-voltage samples {}",
            r.elapsed_s,
            r.duration_s,
            r.last_temp_c,
            r.max_temp_c,
            if r.min_mhz_under_load == u32::MAX {
                0
            } else {
                r.min_mhz_under_load
            },
            r.undervoltage_samples
        );
    }
    let r = test.snapshot();
    let verdict = r.verdict.unwrap_or(Verdict::Stopped);
    println!(
        "Result: {:?} (max {:.1} °C, min {} / {} MHz, under-voltage samples {})",
        verdict, r.max_temp_c, r.min_mhz_under_load, r.target_mhz, r.undervoltage_samples
    );
    if verdict != Verdict::Stable {
        std::process::exit(1);
    }
    Ok(())
}
