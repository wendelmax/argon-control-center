pub mod argon;
pub mod gpio_lab;
pub mod i18n;
pub mod overclock;
pub mod privileged;
pub mod telemetry;
pub mod theme;
pub mod ui;

use crate::argon::{load_fan_curve, FanPreset, CPU_FAN_CONF};
use crate::i18n::Language;
use crate::overclock::load_boot_config;
use crate::telemetry::TelemetryState;
use crate::ui::ArgonControlCenterApp;
use eframe::egui;

fn main() -> eframe::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let initial_lang = if args.iter().any(|a| a == "--en" || a == "--lang=en") {
        Language::EnUs
    } else {
        Language::PtBr
    };

    if args.iter().any(|a| a == "--status" || a == "-s") {
        let curve = load_fan_curve(CPU_FAN_CONF, FanPreset::BalancedDefault.points());
        let mut tel = TelemetryState::new();
        std::thread::sleep(std::time::Duration::from_millis(250));
        tel.refresh(&curve);
        let oc = load_boot_config();
        println!("=== ARGON AORUS CONTROL CENTER — TELEMETRY / TELEMETRIA ===");
        println!("Board / Placa : {}", tel.board_model);
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
            "RAM Usage     : {} / {} MB ({:.1}%)",
            tel.ram_used_mb, tel.ram_total_mb, tel.ram_usage_pct
        );
        println!(
            "Boot OC       : ARM={} MHz, GPU={} MHz, OverVoltage={}, Boost={}",
            oc.arm_freq_mhz, oc.gpu_freq_mhz, oc.over_voltage, oc.arm_boost
        );
        return Ok(());
    }

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("ARGON AORUS CONTROL CENTER — System & Hardware Configurator")
            .with_inner_size([1160.0, 750.0])
            .with_min_inner_size([480.0, 400.0]),
        ..Default::default()
    };

    eframe::run_native(
        "argon-control-center",
        native_options,
        Box::new(move |cc| Ok(Box::new(ArgonControlCenterApp::new(cc, initial_lang)))),
    )
}
