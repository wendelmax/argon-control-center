# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Release notes on GitHub are generated from the section matching the tag.

## [Unreleased]

### Added
- AUR package `argon-control-center`, updated automatically after each release.
- Fedora COPR repository `wendelmax/argon-control-center`, built from each release tag.

### Changed
- Packages and CI target only 64-bit ARM, the Raspberry Pi architecture: `.deb` (arm64), `.rpm` (aarch64) and a new Arch Linux ARM package (aarch64). The x86_64 packages are no longer published.

## [0.2.0] - 2026-11-11

### Added
- KDE Plasma 6 desktop widgets: CPU, Thermal, GPU, Memory, Network, Storage, Power and System. Metric widgets offer bars, gauges (transparent background, sized to the widget) or a two-minute chart; in a panel they show a compact value. Installed by the packages and by `scripts/install.sh`.
- `--json`: one-line snapshot of CPU, temperature, fan, memory, disks, network, GPU, power flags and system information, computed in a few milliseconds from the previous reading.
- Help page in the application describing every feature, with links to the related pages.
- Terminal interface: opens automatically without a graphical session (or with `--tui`); read-only, btop-style boxes for cpu, mem, net, gpu and argon with gradient meters and history graphs. `--gui` forces the window.
- Manual page `argon-control-center(1)`, installed by the packages and by `scripts/install.sh`.
- Application icon (a board sliding out of an aluminium case), used by the menu entry and the window.

### Fixed
- The Arch package no longer builds a separate `-debug` package.

## [0.1.0] - 2026-11-11

First public release.

### Added
- Desktop control center for Raspberry Pi in Argon40 cases, written in Rust with egui/eframe.
- Automatic detection of the Raspberry Pi model, SoC and RAM from the board revision code, and of the Argon case (ONE for Pi 4, ONE V3 for Pi 5, EON) from the installed Argon scripts and an I2C scan.
- Board-aware overclocking profiles for BCM2837, BCM2711 and BCM2712, including `over_voltage_delta` support on Raspberry Pi 5.
- Live telemetry: CPU frequency and per-core load, temperature, fan duty, memory, network, disks and RAID arrays, with 60-second history charts.
- Fan curve editor for `argononed` (CPU and disk curves), fan profiles and immediate manual override over I2C.
- `config.txt` hardware toggles: I2C, SPI, I2S, audio, UART, VC4 KMS, camera and display detection, Argon BLSTR DAC and IR receiver.
- GPIO and serial lab: 40-pin logic analyzer and a serial/I2C console.
- `argononed` service management with an explanation of its role before it is stopped.
- GPU telemetry: V3D load per queue (render, geometry, compute) from `gpu_stats`, CMA memory and the GPU clock, shown in every dashboard style and next to `gpu_freq` on the Overclock page. With firmware access, live GPU / core / H.264 clocks, core voltage, GPU memory split and throttling flags.
- Firmware throttling flags strengthen the power-supply guard and the stability test (throttling or under-voltage reported by the firmware during a test changes the verdict).
- `--doctor` command and a Hardware access section in Settings > About listing what the current user can use and how to enable the rest.
- Packages and the permission setup grant the `video` group access to `/dev/vcio` (as on Raspberry Pi OS) and report missing optional tools.
- Each dashboard style has its own presentation:
  - Compact (btop-style): one titled box per row (cpu, mem, net, argon), each split into two columns, with gradient block meters filled left to right from 0 to 100 % (per core, memory, swap, disks, network, fan, temperature) and key/value lines; no graphs.
  - Gauges: radial gauges only — total CPU plus one gauge per core, clock, temperature, fan, memory, swap, one gauge per disk, and network download / upload.
  - Charts: rings and 60 s history charts.
- Dashboards are read-only: the quick fan, clock-profile, governor and argononed controls were removed from the compact dashboard and from the cores card. A status row (fan mode, clock profile, argononed) on every dashboard and the CPU / temperature / fan rings open the page where the setting can be changed with its safeguards.
- Charts dashboard: memory and network have separate charts; network is scaled to its own peak.
- Per-core CPU history chart (one line per core, 60 s) in the cores card, above the per-core bars.
- Charts dashboard: temperature, cores, memory and storage each take a full row. The system summary (board, OS, kernel, CPU range, boot profile) is a compact line in the footer on every page, with full details on hover.
- Full-width footer in every layout with the action status, argononed state, 5 V supply, IP, uptime, privilege mode and the system line; the sidebar status box and the top-bar badges were merged into it.
- Compact sidebar (176 px) that collapses to icons only (52 px) and the choice is remembered.
- Settings page with independent appearance options: sidebar or top-tab navigation, system or AORUS colors, and compact, charts or gauges dashboards, plus interface scaling from 80% to 150%.
- System colors follow the desktop scheme on KDE Plasma, GNOME, Cinnamon, MATE, XFCE and any window manager running xdg-desktop-portal, in light and dark variants with enforced contrast.
- Layouts adapt to narrow windows: dashboard rings and controls reflow, the GPIO header switches to a single column and navigation becomes a wrapping tab strip.
- Short labels on buttons, cards and options; file paths, `config.txt` keys and explanations are shown in tooltips.
- English interface by default, with Brazilian Portuguese available in Settings or with `--pt`.
- Preferences (appearance, language, temperature unit, scale and last page) are remembered between sessions. The temperature unit shown in the app is independent of the case OLED setting.
- Packages for 64-bit Raspberry Pi systems: Debian and Ubuntu (`.deb`, arm64), Fedora (`.rpm`, aarch64) and a `PKGBUILD` for Arch Linux ARM.
- Packages install udev rules, the `i2c` and `gpio` groups and load `i2c-dev` at boot, so hardware access works without running the application as root.
- `--status` command-line report with the detected hardware and the current overclock.
- `scripts/bump-version.sh` for MAJOR.MINOR.PATCH version bumps and release tags; version tags publish a GitHub Release automatically.

- GPIO lab pin-out diagram: the 40-pin header drawing (Dirk Koudstaal, element14, CC BY-SA) with live levels on every GPIO, click-to-select and per-pin tooltips; the compact list remains available. Credit is shown in Settings > About. The diagram is rendered on a background thread at start-up, so opening the lab is instant.
- Safe overclocking: settings above factory clocks are tested with a one-shot trial boot (`tryboot.txt`, Raspberry Pi 4 and 5) and kept only after a stable test; a power cycle always returns to the previous settings.
- Stability test (1, 3 or 10 minutes, or `--stress <seconds>`) recording temperature, clock and the 5 V rail, with results preserved if the system freezes.
- Power-supply guard: overclocking above factory clocks is blocked while under-voltage has been detected since boot; `--status` reports under-voltage events.

### Fixed
- Characters that egui's fonts cannot draw in normal text (shown as empty squares next to labels, selected options and the daemon status) were replaced; settings options use a drawn radio marker, and a test fails if any UI string uses a character missing from egui's proportional fonts.
- The window title, logs and the managed `config.txt` block are named "Argon Control Center"; AORUS only names the color scheme. Blocks written with the old marker are still recognised and replaced.
- The RPM package lists the asset credits file.
- With system colors, tooltip titles and section headings also use the desktop accent; amber is kept for warnings.
- With system colors, action buttons, card accents and selection outlines use the desktop accent instead of orange; orange is kept for the AORUS scheme, and warnings keep amber and red.
- Switching between sidebar and top-tab navigation no longer changes colors: both use the active color scheme for branding and selection.
- Horizontal overflow on the dashboard and Settings pages: option cards and charts no longer exceed their column, long values in About wrap, and the page area scrolls vertically only.
- Removed the duplicate Settings gear from the headers; Settings is always in the navigation (also in the narrow-window tab strip).
- Saving the hardware interfaces page no longer writes overclock values selected elsewhere (for example from the dashboard) to `config.txt`, and saving an overclock no longer changes interfaces.
- The under-voltage sensor is located by name (`rpi_volt`) instead of a fixed hwmon index.

### Security
- Privileged files are written through Polkit with the content passed on standard input, replaced atomically, and the previous version is kept as `<file>.argoncc.bak`.
- Hardware access is granted through group membership and udev rules; no setuid binaries or world-writable device nodes are used.

[Unreleased]: https://github.com/wendelmax/argon-control-center/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/wendelmax/argon-control-center/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/wendelmax/argon-control-center/releases/tag/v0.1.0
