# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Release notes on GitHub are generated from the section matching the tag.

## [Unreleased]

## [0.1.0] - 2026-11-10

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
- Four interface themes. The KDE and Breeze themes follow the desktop color scheme on KDE Plasma, GNOME, Cinnamon, MATE, XFCE and any window manager running xdg-desktop-portal, in light and dark variants with enforced contrast.
- English interface by default, with Brazilian Portuguese available from the language switch or `--pt`.
- Language, theme and last tab are remembered between sessions.
- Packages for Debian and Ubuntu (`.deb`, amd64 and arm64), Fedora (`.rpm`, x86_64 and aarch64) and Arch Linux (`.pkg.tar.zst` and `PKGBUILD`).
- Packages install udev rules, the `i2c` and `gpio` groups and load `i2c-dev` at boot, so hardware access works without running the application as root.
- `--status` command-line report with the detected hardware and the current overclock.

### Security
- Privileged files are written through Polkit with the content passed on standard input, replaced atomically, and the previous version is kept as `<file>.argoncc.bak`.
- Hardware access is granted through group membership and udev rules; no setuid binaries or world-writable device nodes are used.

[Unreleased]: https://github.com/wendelmax/argon-config-rs/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/wendelmax/argon-config-rs/releases/tag/v0.1.0
