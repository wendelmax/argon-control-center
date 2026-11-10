# ⚡ Argon Control Center (`argon-config-rs`)

[![CI](https://github.com/wendelmax/argon-config-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/wendelmax/argon-config-rs/actions/workflows/ci.yml)

Configurador desktop em **Rust + egui/eframe** para **Raspberry Pi** com gabinetes **Argon40** (Argon ONE, ONE V3, EON). Inspirado nos painéis Gigabyte AORUS e no KDE System Monitor: monitoramento em tempo real, curva da ventoinha, overclock, recursos de hardware, laboratório de GPIO e controle do serviço `argononed`.

> 🇺🇸 **English summary** — Native Linux desktop app (Rust/egui) to monitor and configure a Raspberry Pi in an Argon40 case: live telemetry, fan curve editor, board-aware overclocking (Pi 3/4/5), `config.txt` hardware toggles, GPIO logic analyzer + serial/I2C console and `argononed` service control. UI in Portuguese and English (`--en`), colors follow your desktop theme (KDE, GNOME, XFCE, any WM with xdg-desktop-portal). See [Requirements](#-requisitos) and [Build & run](#-compilar-e-executar).

---

## ✨ Recursos

### 🔎 Detecção automática da placa e do gabinete
Feita na inicialização (`src/board.rs`), sem configuração manual:

| O quê | Como |
|---|---|
| Modelo do Pi, SoC e RAM | código de revisão (`/proc/cpuinfo` ou `/proc/device-tree/system/linux,revision`), `/proc/device-tree/model` e `compatible` |
| Gabinete Argon | scripts Argon instalados (`/etc/argon/*`) + varredura I2C no barramento 1: MCU `0x1a`, OLED `0x3c`, RTC `0x51` (EON), DAC BLSTR `0x4d` |
| Perfil de overclock | presets e limites dos controles por SoC |

Placas suportadas e perfis:

| Placa | SoC | Eco | Padrão | OC Gaming | Extreme |
|---|---|---|---|---|---|
| Pi 5 / 500 / CM5 | BCM2712 | 1800 | 2400 | 2800 (`over_voltage_delta=50000`) | 3000 / GPU 1000 (`over_voltage_delta=100000`) |
| Pi 4 / 400 / CM4 | BCM2711 | 1500 | 1800 | 2000 / GPU 650 (`over_voltage=4`) | 2147 / GPU 750 (`over_voltage=6`) |
| Pi 3 / Zero 2 W | BCM2837 | 1000 | 1400 | 1450 | 1500 |

Gabinetes reconhecidos: **Argon ONE (Pi 4)**, **Argon ONE V3 (Pi 5)**, **Argon EON**. O resumo aparece na barra lateral/topo (passe o mouse para ver o diagnóstico completo) e em `--status`.

### 📊 Monitoramento
- CPU (frequência, governor, carga por núcleo), temperatura com pico, ventoinha (% PWM), RAM/swap, rede RX/TX, discos e arrays RAID (`/proc/mdstat`), alerta de subtensão 5V.
- Gráficos históricos de 60 s (temperatura, CPU, ventoinha, frequência, RAM, rede).

### 🌀 Smart Fan
- Editor gráfico da curva (arraste os pontos) para CPU (`/etc/argononed.conf`) e discos (`/etc/argononed-hdd.conf`).
- Perfis: Silent, Standard (padrão Argon), Performance, Full Speed (100%) e Custom.
- Override manual imediato via I2C (`0x1a`, registrador `0x80`).

### 🔥 Overclock
- **Em tempo real (sem reboot):** governor, frequências mín./máx. e boost via `sysfs`.
- **Firmware (`config.txt`):** presets da tabela acima e controles de `arm_freq`, `gpu_freq`, `over_voltage` (Pi 3/4) ou `over_voltage_delta` (Pi 5), `temp_limit`, `arm_boost`, `force_turbo`. O app grava num bloco gerenciado (`# --- BEGIN ARGON AORUS CONTROL CENTER ---`) e preserva o resto do arquivo.

### 🛠 Recursos de hardware
- I2C, SPI, I2S, áudio onboard, UART, VC4 KMS, auto-detecção de câmera/display.
- DAC Argon BLSTR (`dtoverlay=hifiberry-dacplus,slave`), receptor IR (`dtoverlay=gpio-ir,gpio_pin=23`).
- Unidade de temperatura (`/etc/argonunits.conf`), OLED (`/etc/argoneonoled.conf`), EEPROM (`PSU_MAX_CURRENT=5000`).

### 🔬 GPIO & Serial Lab
- Analisador lógico dos 40 pinos em tempo real (nível, direção, pull, bordas).
- Console estilo Arduino IDE: serial (`/dev/ttyS0`, `/dev/ttyAMA0`, `/dev/ttyAMA10` no Pi 5, `/dev/ttyUSB*`, `/dev/ttyACM*`) e comandos I2C (`scan`, `mcu`, `fan <0-100>`, `high/low <bcm>`).
- Botão para liberar acesso a GPIO/I2C ao usuário (via Polkit).

### ⚙ Serviços
- `argononed.service`: iniciar, parar, reiniciar, habilitar/desabilitar no boot, com explicação do que o serviço faz antes de desligá-lo.

### 🎨 Interface
- **Temas:** ⚡ KDE Compacto (tudo numa tela), 🖥 KDE Completo (sidebar + donuts + gráficos), 🎨 Breeze + v1 e 🎮 AORUS v1.
- **Cores do sistema** (`src/system_colors.rs`): KDE (`kdeglobals`), GNOME/Cinnamon/MATE/XFCE/Budgie (tema GTK e cor de destaque), qualquer WM via xdg-desktop-portal. Suporta tema claro e escuro em todas as abas (textos e destaques são ajustados para manter contraste mínimo WCAG) e acompanha a troca de tema ao vivo. O tema 🎮 AORUS mantém o visual neon fixo.
- **Idiomas:** Português (padrão) e Inglês, com troca na interface.
- Tooltips explicativos em todos os controles, layout responsivo (janela mínima 480×400).

---

## 📋 Requisitos

### Hardware
- Raspberry Pi 3, 4, 400, CM4, 5, 500 ou CM5 (outros sistemas ARM/x86 rodam o monitoramento, sem os recursos específicos do Pi).
- Gabinete Argon40 é opcional: sem ele, a aba de ventoinha fica sem efeito, mas o resto funciona.

### Sistema
- Linux com sessão gráfica **Wayland ou X11** e OpenGL (driver `vc4`/`v3d` no Pi).
- Testado em Armbian (Ubuntu 26.04) e Raspberry Pi OS; deve funcionar em qualquer Debian/Ubuntu/Fedora/Arch para ARM64.

### Pacotes (Debian/Ubuntu/Raspberry Pi OS)

```bash
# Build (bibliotecas usadas pelo eframe/glow)
sudo apt install build-essential pkg-config libgl1-mesa-dev libxkbcommon-dev \
  libwayland-dev libx11-dev libxcursor-dev libxrandr-dev libxi-dev

# Execução
sudo apt install policykit-1 i2c-tools gpiod python3 python3-smbus
# pinctrl (GPIO Lab): pacote raspi-utils no Raspberry Pi OS
sudo apt install raspi-utils   # se disponível na sua distro
```

| Ferramenta | Usada para |
|---|---|
| `pkexec` (Polkit) | gravar `config.txt`, `/etc/argon*.conf` e controlar o serviço com senha gráfica |
| `systemctl` | controle do `argononed.service` |
| `i2cdetect` / `i2cget` | varredura I2C e console do GPIO Lab |
| `pinctrl`, `gpioget`/`gpioset` | analisador lógico de GPIO |
| `python3` + scripts Argon | override da ventoinha, IR e EEPROM |

### Scripts oficiais da Argon40 (recomendado)
Instalam o `argononed` (curva automática da ventoinha, botão power, OLED):

```bash
curl https://download.argon40.com/argon1.sh | bash        # Argon ONE (Pi 4) e ONE V3 (Pi 5)
curl https://download.argon40.com/argoneon.sh | bash      # Argon EON
```

### Permissões
- Leitura de telemetria: nenhuma permissão especial.
- Escrita em arquivos do sistema: o app pede a senha via Polkit quando necessário.
- Acesso direto a I2C/GPIO (detecção do MCU/OLED/RTC, GPIO Lab): coloque o usuário nos grupos `i2c`, `gpio` e `dialout` (ou use o botão **🔓 Liberar Acesso Hardware** no GPIO Lab) e faça login novamente:
  ```bash
  sudo usermod -aG i2c,dialout "$USER"
  getent group gpio && sudo usermod -aG gpio "$USER"   # o grupo gpio não existe em todas as distros
  ```
  Sem esse acesso, a detecção do gabinete usa apenas os scripts instalados e os dispositivos já reconhecidos pelo kernel.

### Toolchain
- **Rust stable** (edição 2021). O projeto usa [`mise`](https://mise.jdx.dev) para fixar a versão (`mise.toml`), mas `rustup` também funciona.

---

## 🚀 Compilar e executar

```bash
git clone https://github.com/wendelmax/argon-config-rs.git
cd argon-config-rs

mise install            # instala o Rust definido em mise.toml
mise run build          # cargo build --release
mise run run            # abre a interface gráfica
```

Sem `mise`:

```bash
cargo build --release
./target/release/argon-control-center
```

### Opções de linha de comando

| Opção | Efeito |
|---|---|
| `--status`, `-s` | imprime telemetria, placa/gabinete detectados e overclock atual no terminal |
| `--en`, `--lang=en` | abre a interface em inglês |

### Instalar (binário + atalho no menu)

```bash
mise run install        # compila e instala em ~/.local/bin + ~/.local/share/applications
mise run uninstall      # remove
```

Sem `mise`: `scripts/install.sh` (ou `scripts/install.sh --uninstall`). Para instalar para todos os usuários: `PREFIX=/usr/local sudo -E scripts/install.sh`.

---

## 🧪 Desenvolvimento

```bash
mise run check                                         # cargo test
mise run lint                                          # fmt --check + clippy -D warnings (igual à CI)
cargo test print_board -- --ignored --nocapture        # diagnóstico da placa detectada
cargo test print_detected -- --ignored --nocapture     # diagnóstico das cores do sistema
```

### Estrutura

| Arquivo | Responsabilidade |
|---|---|
| `src/main.rs` | entrada, CLI (`--status`, `--en`) |
| `src/ui.rs` | interface (abas, layouts dos temas) |
| `src/board.rs` | detecção da placa, gabinete e perfil de overclock |
| `src/telemetry.rs` | coleta de sensores (`/proc`, `/sys`) |
| `src/argon.rs` | curvas da ventoinha e comunicação com o MCU Argon |
| `src/overclock.rs` | leitura/escrita do `config.txt` e cpufreq em tempo real |
| `src/gpio_lab.rs` | analisador lógico e console serial/I2C |
| `src/theme.rs` | estilos, donuts e gráficos |
| `src/system_colors.rs` | paleta do desktop (KDE/GTK/portal) |
| `src/i18n.rs` | textos PT-BR / EN-US |
| `scripts/install.sh` | instalação/remoção do binário e do atalho |
| `.github/workflows/ci.yml` | CI: fmt, clippy e testes em x86_64 e ARM64 |
| `src/privileged.rs` | elevação via Polkit |

---

## ⚠️ Avisos
- Overclock e `over_voltage` podem causar instabilidade; mantenha refrigeração ativa. Se o Pi não iniciar após um ajuste, edite `config.txt` no cartão SD em outro computador e remova o bloco `ARGON AORUS CONTROL CENTER`.
- Parar o `argononed` desativa a curva automática da ventoinha e o botão power do gabinete.
