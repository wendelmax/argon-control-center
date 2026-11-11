<div align="center">

# Argon Control Center

**A native Linux control center for Raspberry Pi computers in Argon40 cases.**

Monitor the system, tune the fan curve, overclock safely and manage hardware features from one desktop application.

[![CI](https://github.com/wendelmax/argon-config-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/wendelmax/argon-config-rs/actions/workflows/ci.yml)
[![Packages](https://github.com/wendelmax/argon-config-rs/actions/workflows/package.yml/badge.svg)](https://github.com/wendelmax/argon-config-rs/actions/workflows/package.yml)
[![Release](https://img.shields.io/github/v/release/wendelmax/argon-config-rs?sort=semver)](https://github.com/wendelmax/argon-config-rs/releases)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
![Platform](https://img.shields.io/badge/platform-Linux%20arm64%20%7C%20x86__64-informational)
![Rust](https://img.shields.io/badge/rust-stable-orange)

[Installation](#installation) · [Usage](#usage) · [Documentation](#configuration) · [Changelog](CHANGELOG.md) · [Releases](https://github.com/wendelmax/argon-config-rs/releases)

</div>

---

## Table of contents

- [Overview](#overview)
- [Features](#features)
- [Compatibility](#compatibility)
- [Installation](#installation)
- [Usage](#usage)
- [Safe overclocking](#safe-overclocking)
- [Configuration](#configuration)
- [Permissions](#permissions)
- [Building from source](#building-from-source)
- [Development](#development)
- [Release process](#release-process)
- [Project structure](#project-structure)
- [Contributing](#contributing)
- [Security](#security)
- [License](#license)

## Overview

Argon Control Center is a desktop application written in Rust with [egui](https://github.com/emilk/egui). It brings together the tasks that are otherwise spread across `config.txt`, the Argon40 scripts, `systemctl` and sysfs:

- live telemetry with history charts;
- a graphical editor for the `argononed` fan curve;
- overclocking presets matched to the detected board;
- hardware toggles for `config.txt`;
- a GPIO logic analyzer and a serial/I2C console.

The board and the Argon case are detected automatically at startup, so presets and limits always match the hardware. The interface follows the desktop color scheme and is available in English and Brazilian Portuguese.

## Features

**Hardware detection**
- Raspberry Pi model, SoC and memory decoded from the board revision code.
- Argon case identified from the installed Argon40 scripts and an I2C scan (fan MCU `0x1a`, OLED `0x3c`, RTC `0x51`, BLSTR DAC `0x4d`).

**Monitoring**
- CPU frequency, governor and per-core load, SoC temperature with peak, fan duty, memory and swap, network throughput, disks and RAID arrays, and the 5 V under-voltage alarm.
- 60-second history charts. Slow probes run on a worker thread so the interface stays responsive.

**Fan control**
- Drag-and-drop curve editor for the CPU (`/etc/argononed.conf`) and disk (`/etc/argononed-hdd.conf`) curves.
- Silent, Standard, Performance and Full Speed profiles, plus an immediate manual override over I2C.

**Overclocking**
- Live changes without rebooting: governor, minimum and maximum frequency, and boost.
- Firmware presets written to a managed block in `config.txt`. The rest of the file is preserved.
- `over_voltage` on Raspberry Pi 3 and 4; `over_voltage_delta` on Raspberry Pi 5.
- Safe overclocking: trial boot, stability test and a power-supply guard (see [Safe overclocking](#safe-overclocking)).

**Hardware features**
- I2C, SPI, I2S, onboard audio, UART, VC4 KMS, camera and display auto-detection.
- Argon BLSTR DAC and IR receiver overlays, OLED settings and EEPROM power-supply limit.

**GPIO and serial lab**
- Live state of the 40-pin header: level, direction, pull and edge counters.
- Serial monitor for UART and USB adapters, and I2C diagnostic commands.

**Service management**
- Start, stop, restart, enable and disable `argononed.service`, with an explanation of its role before it is stopped.

**Interface**
- Sidebar or top-tab navigation, system or AORUS color scheme, and three dashboard styles, all configurable in Settings.
- Light and dark desktop themes on KDE Plasma, GNOME, Cinnamon, MATE, XFCE and any window manager running xdg-desktop-portal, with enforced text contrast.
- Interface scaling from 80% to 150%; layouts adapt to narrow windows.

## Compatibility

### Boards and overclocking profiles

| Board | SoC | Eco | Stock | OC | Extreme |
|---|---|---|---|---|---|
| Raspberry Pi 5, 500, CM5 | BCM2712 | 1800 MHz | 2400 MHz | 2800 MHz | 3000 MHz |
| Raspberry Pi 4, 400, CM4 | BCM2711 | 1500 MHz | 1800 MHz | 2000 MHz | 2147 MHz |
| Raspberry Pi 3, Zero 2 W | BCM2837 | 1000 MHz | 1400 MHz | 1450 MHz | 1500 MHz |

Other Linux systems (ARM or x86_64) can run the monitoring features; Raspberry Pi specific features are disabled.

### Argon40 cases

| Case | Board |
|---|---|
| Argon ONE, Argon ONE M.2 | Raspberry Pi 4 |
| Argon ONE V3, Argon ONE V3 M.2 | Raspberry Pi 5 |
| Argon EON | Raspberry Pi 4 |

A case is optional. Without one, fan control has no effect and everything else works.

### Distributions

Packaged for Debian, Ubuntu, Raspberry Pi OS, Armbian, Fedora and Arch Linux. Requires a Wayland or X11 session with OpenGL.

## Installation

### Packages

Download the package for your distribution from the [latest release](https://github.com/wendelmax/argon-config-rs/releases/latest).

| Distribution | Command |
|---|---|
| Debian, Ubuntu, Raspberry Pi OS, Armbian | `sudo apt install ./argon-control-center_<version>_arm64.deb` |
| Fedora | `sudo dnf install ./argon-control-center-<version>-1.<arch>.rpm` |
| Arch Linux (x86_64) | `sudo pacman -U ./argon-control-center-<version>-1-x86_64.pkg.tar.zst` |
| Arch Linux ARM | build `packaging/arch/PKGBUILD` with `makepkg -si` |

The packages install the application, its menu entry and the hardware access rules (see [Permissions](#permissions)). Log out and back in once after installing.

Verify a download with:

```sh
sha256sum --check --ignore-missing SHA256SUMS
```

### Argon40 daemon (recommended)

Fan curves, the power button and the OLED display are handled by the official `argononed` service:

```sh
curl https://download.argon40.com/argon1.sh | bash      # Argon ONE (Pi 4) and ONE V3 (Pi 5)
curl https://download.argon40.com/argoneon.sh | bash    # Argon EON
```

## Usage

Launch **Argon Control Center** from the application menu, or run:

```sh
argon-control-center
```

| Option | Description |
|---|---|
| `--status`, `-s` | Print telemetry, detected hardware, power events and the current overclock, then exit |
| `--stress <seconds>` | Run the stability test without the interface; exits with status 1 if the result is not stable |
| `--pt`, `--lang=pt` | Start in Brazilian Portuguese |
| `--en`, `--lang=en` | Start in English |

Example:

```text
$ argon-control-center --status
=== ARGON CONTROL CENTER — TELEMETRY ===
Board         : Raspberry Pi 4 Model B Rev 1.4
Detected      : Pi 4 · BCM2711 · 8 GB · Argon ONE (Pi 4)
CPU Temp      : 47.7 °C | Target Fan: 0% | Argononed: active
```

## Safe overclocking

A firmware overclock that the board or power supply cannot sustain usually shows up as a system that no longer boots. The application guards against this in three ways.

**Power-supply guard.** The 5 V rail is monitored through the `rpi_volt` sensor and the kernel log. If under-voltage has occurred since boot, settings above factory clocks are blocked, because a supply that already dips at stock clocks cannot sustain an overclock. Use the official 5.1 V / 3 A supply (or the one recommended for your Argon case) and reboot.

**Trial boot.** On boards with the tryboot bootloader feature (Raspberry Pi 4 with a recent EEPROM, Raspberry Pi 5), settings above factory clocks are not written to `config.txt`. They go to `tryboot.txt`, and the board boots once with them:

1. **Test boot** writes `tryboot.txt`; nothing changes yet.
2. **Reboot into trial** starts the board once with the new settings.
3. If the board hangs, switch it off and on: it boots the unchanged `config.txt`, and the application reports that the trial failed.
4. If it boots, run the stability test. **Keep** copies the settings to `config.txt` once the test is stable; **Discard** removes them.

On boards without tryboot, saving above factory clocks requires explicitly accepting the risk.

**Stability test.** Loads every core for 1, 3 or 10 minutes while recording temperature, clock and the 5 V rail, then reports one of: stable, throttled, too hot (80 °C) or under-voltage. Progress is saved every second, so if the system freezes during the test the result is reported after the next start. The same test is available from the command line:

```sh
argon-control-center --stress 180
```

Each page saves only its own settings: saving hardware interfaces never writes overclock values, and saving an overclock never changes interfaces.

## Configuration

Preferences are changed on the **Settings** page and stored in `~/.config/argon-control-center/settings.json`.

| Setting | Options |
|---|---|
| Navigation | Sidebar, Top tabs |
| Colors | Follow system, AORUS |
| Dashboard | Compact, Charts, Gauges |
| Interface scale | 80% to 150% |
| Language | English, Português (Brasil) |
| Temperature unit | °C, °F (the case OLED unit is applied separately) |

System files written by the application:

| File | Purpose |
|---|---|
| `/boot/firmware/config.txt` (or `/boot/config.txt`) | Overclocking and hardware overlays |
| `/boot/firmware/tryboot.txt` | Overclock under trial (used for a single boot) |
| `/etc/argononed.conf`, `/etc/argononed-hdd.conf` | Fan curves |
| `/etc/argonunits.conf` | Temperature unit used by `argononed` and the OLED |
| `/etc/argoneonoled.conf` | OLED display settings |

Every write goes through Polkit, replaces the file atomically and keeps the previous version as `<file>.argoncc.bak`.

## Permissions

The application never needs to run as root. Privileged writes prompt for a password through Polkit. Direct hardware access uses group membership:

| Feature | Requirement |
|---|---|
| Telemetry | None |
| System files, service control | Polkit authentication when needed |
| I2C (case detection, fan override, console) | `i2c` group |
| GPIO lab | `gpio` group |
| Serial console | `dialout` (Debian, Fedora) or `uucp` (Arch) |

The packages create the groups, install udev rules for `/dev/i2c-*`, `/dev/gpiochip*` and `/dev/gpiomem*`, and load `i2c-dev` at boot. The user who installed the package with `sudo` is added to the groups automatically.

For installations from source, use **Unlock Direct Hardware Access** in the GPIO lab, or run:

```sh
sudo packaging/common/setup-permissions.sh "$USER"
```

No setuid binaries or world-writable device nodes are used.

## Building from source

### Dependencies

```sh
# Debian / Ubuntu / Raspberry Pi OS
sudo apt install build-essential pkg-config libgl1-mesa-dev libxkbcommon-dev \
  libwayland-dev libx11-dev libxcursor-dev libxrandr-dev libxi-dev

# Fedora
sudo dnf install gcc pkgconf mesa-libGL-devel mesa-libEGL-devel \
  libxkbcommon-devel wayland-devel libX11-devel

# Arch Linux
sudo pacman -S --needed base-devel libglvnd libxkbcommon wayland libx11
```

Rust stable is required. The toolchain is pinned with [mise](https://mise.jdx.dev) (`mise.toml`); [rustup](https://rustup.rs) also works.

Optional runtime tools: `pkexec` (Polkit), `i2c-tools`, `gpiod` / `libgpiod-utils` and `pinctrl` (`raspi-utils` on Raspberry Pi OS).

### Build and install

```sh
git clone https://github.com/wendelmax/argon-config-rs.git
cd argon-config-rs
mise install          # install the pinned Rust toolchain
mise run install      # build and install to ~/.local
```

Without mise, use `cargo build --release` and `scripts/install.sh`. Remove the installation with `mise run uninstall` or `scripts/install.sh --uninstall`.

### Packages

```sh
packaging/debian/build-deb.sh                          # Debian / Ubuntu  -> dist/*.deb
rpmbuild -bb packaging/rpm/argon-control-center.spec   # Fedora (source tarball in ~/rpmbuild/SOURCES)
(cd packaging/arch && makepkg -si)                     # Arch Linux
```

## Development

```sh
mise run check     # tests
mise run lint      # rustfmt check and clippy with warnings as errors
cargo run          # run the application

cargo test print_board -- --ignored --nocapture     # print detected hardware
cargo test print_detected -- --ignored --nocapture  # print detected desktop colors
```

Continuous integration runs formatting, clippy, tests and a release build on x86_64 and ARM64 for every pull request. Changes to packaging also build, install and test the Debian, Fedora and Arch packages.

Commit messages follow [Conventional Commits](https://www.conventionalcommits.org/) (`feat:`, `fix:`, `chore:`, `ci:`).

## Release process

Versions follow [Semantic Versioning](https://semver.org/): increment **MAJOR** for breaking changes, **MINOR** for new features and **PATCH** for fixes.

1. Record changes under `[Unreleased]` in [CHANGELOG.md](CHANGELOG.md).
2. Bump the version on a branch and merge it through a pull request:
   ```sh
   scripts/bump-version.sh patch    # or: minor, major
   ```
   This updates `Cargo.toml`, `Cargo.lock`, the RPM spec, the PKGBUILD and the changelog.
3. Tag the merged commit on `main` and push the tag:
   ```sh
   scripts/bump-version.sh --tag
   git push origin vX.Y.Z
   ```

Pushing the tag builds and tests every package and publishes a GitHub Release named "Argon Control Center X.Y.Z". The release notes come from the changelog and include installation instructions and checksums. Tags with a pre-release suffix such as `v1.2.0-rc.1` are published as pre-releases. Preview the notes with `scripts/release-notes.sh vX.Y.Z`.

## Project structure

```text
src/
  main.rs            entry point and command-line options
  ui.rs              application window, pages and layouts
  board.rs           board, case and overclocking profile detection
  telemetry.rs       sensors from /proc and /sys
  argon.rs           fan curves and Argon MCU access
  overclock.rs       config.txt parsing and writing, live cpufreq
  gpio_lab.rs        GPIO analyzer, serial and I2C console, permission setup
  theme.rs           styles, charts and theme-aware colors
  system_colors.rs   desktop color scheme detection
  settings.rs        user preferences
  i18n.rs            English and Portuguese strings
  privileged.rs      Polkit elevation and atomic writes
packaging/           Debian, RPM and Arch packaging, udev rules, permission setup
scripts/             install, version bump and release notes helpers
.github/workflows/   CI and package/release pipelines
```

## Contributing

Contributions are welcome. Please open an issue to discuss larger changes first.

1. Fork the repository and create a branch from `main`.
2. Make your change and add tests where practical.
3. Run `mise run lint` and `mise run check`.
4. Add an entry under `[Unreleased]` in `CHANGELOG.md`.
5. Open a pull request with a Conventional Commit title.

## Security

Overclocking and over-voltage settings can make a system unstable. Use the trial boot, keep active cooling in place and fix any under-voltage first. If a board no longer boots after a permanent change, edit `config.txt` on the boot media from another computer: restore `config.txt.argoncc.bak` or remove the `ARGON AORUS CONTROL CENTER` block.

Stopping `argononed` disables the automatic fan curve and the case power button.

To report a security issue, please use [GitHub private vulnerability reporting](https://github.com/wendelmax/argon-config-rs/security/advisories/new) instead of a public issue.

## License

Released under the [MIT License](LICENSE).

Argon40, Raspberry Pi and AORUS are trademarks of their respective owners. This project is not affiliated with or endorsed by them.
