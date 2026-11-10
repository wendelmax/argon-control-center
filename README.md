# ⚡ Argon Control Center (`argon-config-rs`)

[![CI](https://github.com/wendelmax/argon-config-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/wendelmax/argon-config-rs/actions/workflows/ci.yml)
[![Packages](https://github.com/wendelmax/argon-config-rs/actions/workflows/package.yml/badge.svg)](https://github.com/wendelmax/argon-config-rs/actions/workflows/package.yml)

Native Linux desktop app written in **Rust + egui/eframe** to monitor and configure a **Raspberry Pi** in an **Argon40** case (Argon ONE, ONE V3, EON). Inspired by the Gigabyte AORUS control panels and the KDE System Monitor: live telemetry, fan curve editor, board-aware overclocking, `config.txt` hardware toggles, a GPIO lab and `argononed` service control.

The interface is in **English** by default, with **Brazilian Portuguese** available as an alternative (language switch in the header, or `--pt`).

---

## ✨ Features

### 🔎 Automatic board and case detection
Done at startup (`src/board.rs`), with no manual configuration:

| What | How |
|---|---|
| Pi model, SoC and RAM | revision code (`/proc/cpuinfo` or `/proc/device-tree/system/linux,revision`), `/proc/device-tree/model` and `compatible` |
| Argon case | installed Argon scripts (`/etc/argon/*`) + I2C scan on bus 1: MCU `0x1a`, OLED `0x3c`, RTC `0x51` (EON), BLSTR DAC `0x4d` |
| Overclock profile | presets and control limits per SoC |

Supported boards and overclock profiles (MHz):

| Board | SoC | Eco | Stock | OC Gaming | Extreme |
|---|---|---|---|---|---|
| Pi 5 / 500 / CM5 | BCM2712 | 1800 | 2400 | 2800 (`over_voltage_delta=50000`) | 3000 / GPU 1000 (`over_voltage_delta=100000`) |
| Pi 4 / 400 / CM4 | BCM2711 | 1500 | 1800 | 2000 / GPU 650 (`over_voltage=4`) | 2147 / GPU 750 (`over_voltage=6`) |
| Pi 3 / Zero 2 W | BCM2837 | 1000 | 1400 | 1450 | 1500 |

Recognised cases: **Argon ONE (Pi 4)**, **Argon ONE V3 (Pi 5)**, **Argon EON**. The summary is shown in the sidebar/header (hover for full diagnostics) and by `--status`.

### 📊 Monitoring
- CPU (frequency, governor, per-core load), temperature with peak, fan (% PWM), RAM/swap, network RX/TX, disks and RAID arrays (`/proc/mdstat`), 5 V under-voltage alarm.
- 60-second history charts (temperature, CPU, fan, frequency, RAM, network).
- Slow probes (`df`, `systemctl`) run on a worker thread, so the UI never stutters.

### 🌀 Smart Fan
- Drag-and-drop curve editor for CPU (`/etc/argononed.conf`) and disks (`/etc/argononed-hdd.conf`).
- Profiles: Silent, Standard (Argon default), Performance, Full Speed (100%) and Custom.
- Immediate manual override over I2C (`0x1a`, register `0x80`).

### 🔥 Overclocking
- **Live (no reboot):** governor, min/max frequency and boost via `sysfs`.
- **Firmware (`config.txt`):** presets from the table above plus `arm_freq`, `gpu_freq`, `over_voltage` (Pi 3/4) or `over_voltage_delta` (Pi 5), `temp_limit`, `arm_boost`, `force_turbo`. Changes go into a managed block (`# --- BEGIN ARGON AORUS CONTROL CENTER ---`); the rest of the file is preserved.

### 🛠 Hardware features
- I2C, SPI, I2S, onboard audio, UART, VC4 KMS, camera/display auto-detect.
- Argon BLSTR DAC (`dtoverlay=hifiberry-dacplus,slave`), IR receiver (`dtoverlay=gpio-ir,gpio_pin=23`).
- Temperature unit (`/etc/argonunits.conf`), OLED (`/etc/argoneonoled.conf`), EEPROM (`PSU_MAX_CURRENT=5000`).

### 🔬 GPIO & Serial Lab
- Live logic analyzer for the 40-pin header (level, direction, pull, edges).
- Arduino-IDE-style console: serial (`/dev/ttyS0`, `/dev/ttyAMA0`, `/dev/ttyAMA10` on Pi 5, `/dev/ttyUSB*`, `/dev/ttyACM*`) and I2C commands (`scan`, `mcu`, `fan <0-100>`, `high/low <bcm>`).
- **🔓 Unlock Direct Hardware Access** button: one Polkit prompt sets up groups and udev rules (see [Permissions](#-permissions)).

### ⚙ Services
- `argononed.service`: start, stop, restart, enable/disable at boot, with an explanation of what the service does before you stop it.

### 🎨 Interface
- **Themes:** ⚡ KDE Compact (everything on one screen), 🖥 KDE Full (sidebar + donuts + charts), 🎨 Breeze + v1 and 🎮 AORUS v1.
- **System colors** (`src/system_colors.rs`): KDE (`kdeglobals`), GNOME/Cinnamon/MATE/XFCE/Budgie (GTK theme and accent color), any WM through xdg-desktop-portal. Light and dark schemes on every tab, with WCAG contrast guards; theme changes are picked up live. 🎮 AORUS keeps its fixed neon look.
- **Languages:** English (default) and Português (Brasil).
- Language, theme and last tab are remembered in `~/.config/argon-control-center/settings.json`.

### 🔐 Safe system writes
Privileged files are written through Polkit with the content passed on stdin (no predictable `/tmp` staging file), replaced atomically, and the previous version is kept as `<file>.argoncc.bak` (e.g. `/boot/firmware/config.txt.argoncc.bak`).

---

## 📦 Installation

### Packages
Every `v*` tag publishes packages on the [Releases](https://github.com/wendelmax/argon-config-rs/releases) page.

| Distro | Package | Install |
|---|---|---|
| Debian, Ubuntu, Raspberry Pi OS, Armbian | `.deb` (arm64, amd64) | `sudo apt install ./argon-control-center_<ver>_arm64.deb` |
| Fedora | `.rpm` (aarch64, x86_64) | `sudo dnf install ./argon-control-center-<ver>.rpm` |
| Arch Linux / Arch Linux ARM | `PKGBUILD` (`packaging/arch`) | `makepkg -si` |

Packages install the binary, the menu entry, the udev rules and the `i2c`/`gpio` groups, and load `i2c-dev` at boot. When installed with `sudo`, the user who ran `sudo` is added to the hardware groups automatically.

Build the packages yourself:

```bash
packaging/debian/build-deb.sh                              # → dist/*.deb
rpmbuild -bb packaging/rpm/argon-control-center.spec       # Fedora (source tarball in ~/rpmbuild/SOURCES)
cd packaging/arch && makepkg -si                           # Arch
```

### From source (current user, no root)

```bash
git clone https://github.com/wendelmax/argon-config-rs.git
cd argon-config-rs
mise install            # Rust toolchain pinned in mise.toml
mise run install        # builds and installs to ~/.local/bin + ~/.local/share/applications
mise run uninstall      # removes it
```

Without `mise`: `scripts/install.sh` (or `scripts/install.sh --uninstall`). System-wide: `PREFIX=/usr/local sudo -E scripts/install.sh`. Then set up [permissions](#-permissions).

---

## 📋 Requirements

### Hardware
- Raspberry Pi 3, 4, 400, CM4, 5, 500 or CM5. Other ARM/x86 Linux systems run the monitoring features without the Pi-specific ones.
- An Argon40 case is optional: without it the fan tab has no effect; everything else works.

### System
- Linux with a **Wayland or X11** session and OpenGL (`vc4`/`v3d` driver on the Pi).
- Tested on Armbian (Ubuntu 26.04) and Raspberry Pi OS; packaged for Debian/Ubuntu, Fedora and Arch.

### Runtime tools

| Tool | Used for | Debian/Ubuntu | Fedora | Arch |
|---|---|---|---|---|
| `pkexec` (Polkit) | writing `config.txt`, `/etc/argon*.conf`, service control | `pkexec` / `policykit-1` | `polkit` | `polkit` |
| `i2cdetect`, `i2cget` | I2C scan and console | `i2c-tools` | `i2c-tools` | `i2c-tools` |
| `gpioget`, `gpioset` | GPIO tools | `gpiod` | `libgpiod-utils` | `libgpiod` |
| `pinctrl` | GPIO logic analyzer | `raspi-utils` | — | — |
| `python3` + Argon scripts | fan override, IR, EEPROM | `python3-smbus` | `python3-smbus` | `i2c-tools` |

### Build dependencies

```bash
# Debian / Ubuntu
sudo apt install build-essential pkg-config libgl1-mesa-dev libxkbcommon-dev \
  libwayland-dev libx11-dev libxcursor-dev libxrandr-dev libxi-dev
# Fedora
sudo dnf install gcc pkgconf mesa-libGL-devel mesa-libEGL-devel libxkbcommon-devel wayland-devel libX11-devel
# Arch
sudo pacman -S --needed base-devel libglvnd libxkbcommon wayland libx11
```

Rust **stable** (edition 2021). The project pins it with [`mise`](https://mise.jdx.dev) (`mise.toml`); `rustup` works too.

### Official Argon40 scripts (recommended)
They install `argononed` (automatic fan curve, power button, OLED):

```bash
curl https://download.argon40.com/argon1.sh | bash      # Argon ONE (Pi 4) and ONE V3 (Pi 5)
curl https://download.argon40.com/argoneon.sh | bash    # Argon EON
```

---

## 🔐 Permissions

| Feature | Needs |
|---|---|
| Telemetry | nothing |
| Writing system files, service control | Polkit password prompt (on demand) |
| I2C (case detection, fan override, console) | `i2c` group |
| GPIO lab | `gpio` group (`/dev/gpiochip*`, `/dev/gpiomem*`) |
| Serial console | `dialout` (Debian/Fedora) or `uucp` (Arch) |

The packages set this up automatically. For source installs, use **🔓 Unlock Direct Hardware Access** in the GPIO Lab, or run:

```bash
sudo packaging/common/setup-permissions.sh "$USER"    # then log out and back in
```

The script creates the groups, installs `60-argon-control-center.rules`, loads `i2c-dev` at boot, reloads udev and adds the user to the groups. It never relies on setuid binaries or world-writable device nodes, and it removes setuid bits left by earlier development versions of this app.

---

## 🚀 Usage

| Option | Effect |
|---|---|
| *(none)* | opens the GUI (English, or the language saved in settings) |
| `--pt`, `--lang=pt` | opens the GUI in Brazilian Portuguese |
| `--en`, `--lang=en` | opens the GUI in English |
| `--status`, `-s` | prints telemetry, detected board/case and current overclock |

---

## 🧪 Development

```bash
mise run check                                         # cargo test
mise run lint                                          # fmt --check + clippy -D warnings (same as CI)
cargo test print_board -- --ignored --nocapture        # detected board diagnostics
cargo test print_detected -- --ignored --nocapture     # detected desktop colors
```

CI (`.github/workflows/ci.yml`) runs fmt, clippy, tests and a release build on x86_64 and ARM64. `.github/workflows/package.yml` builds and smoke-tests the `.deb`, `.rpm` and Arch packages, and attaches them to the GitHub Release on `v*` tags.

### Releasing

1. Move the `[Unreleased]` entries in `CHANGELOG.md` to a new `## [x.y.z] - YYYY-MM-DD` section and bump `version` in `Cargo.toml` (plus `Version` in the RPM spec and `pkgver` in the `PKGBUILD`).
2. Merge to `main`, then tag and push: `git tag vX.Y.Z && git push origin vX.Y.Z`.

The `Packages` workflow then builds and tests every package, checks that the tag matches `Cargo.toml`, writes `SHA256SUMS` and publishes the GitHub Release "Argon Control Center X.Y.Z". The release notes come from the matching `CHANGELOG.md` section (or from Conventional Commits since the previous tag), followed by installation and verification instructions. Tags with a suffix (`v1.2.0-rc.1`) are published as pre-releases. Preview the notes locally with `scripts/release-notes.sh vX.Y.Z`.

### Layout

| Path | Purpose |
|---|---|
| `src/main.rs` | entry point, CLI |
| `src/ui.rs` | interface (tabs, theme layouts) |
| `src/board.rs` | board, case and overclock profile detection |
| `src/telemetry.rs` | sensors (`/proc`, `/sys`) |
| `src/argon.rs` | fan curves and Argon MCU |
| `src/overclock.rs` | `config.txt` read/write, live cpufreq |
| `src/gpio_lab.rs` | logic analyzer, serial/I2C console, permission setup |
| `src/theme.rs` | styles, theme-aware colors, donuts and charts |
| `src/system_colors.rs` | desktop palette (KDE/GTK/portal) |
| `src/settings.rs` | saved preferences |
| `src/i18n.rs` | UI strings (EN / PT-BR) |
| `src/privileged.rs` | Polkit elevation, atomic writes with backup |
| `packaging/` | `.deb`, `.rpm`, Arch `PKGBUILD`, udev rules, permission setup |
| `scripts/install.sh` | per-user install from source |
| `scripts/release-notes.sh` | GitHub release notes for a tag |
| `CHANGELOG.md` | release history (Keep a Changelog) |

---

## ⚠️ Warnings
- Overclocking and over-voltage can cause instability; keep active cooling. If the Pi stops booting after a change, edit `config.txt` on the SD card from another computer: restore `config.txt.argoncc.bak` or delete the `ARGON AORUS CONTROL CENTER` block.
- Stopping `argononed` disables the automatic fan curve and the case power button.

---

## 📄 License

[MIT](LICENSE) © 2026 wendelmax
