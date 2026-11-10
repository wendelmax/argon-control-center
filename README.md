# ⚡ Argon AORUS Control Center (`argon-config-rs`)

Configurador desktop completo em **Rust (`egui` / `eframe`)** com visual inspirado no **Gigabyte Control Center / AORUS Engine** para gerenciamento dos gabinetes **Argon One / EON / V3** e **Raspberry Pi 4 / 5** no Linux.

## Principais Recursos

1. **📊 Dashboard / Monitoramento em Tempo Real**:
   - **4 Medidores Radiais (Tachometers)** estilo AORUS:
     - **CPU Clock Frequency** (MHz em tempo real e Governor ativo)
     - **CPU Core Temperature** (°C ou °F com registro de pico máximo)
     - **Argon Cooling Fan** (% PWM calculado pela curva ou em override manual)
     - **Memory Utilization** (% RAM e uso em MB)
   - **Gráfico Osciloscópio de Telemetria (60s)**: Temperatura, Carga de CPU e Rotação da Fan em tempo real.
   - **Carga Individual por Núcleo (CPU0..CPU3)**, Status de Tensão 5V (`rpi_volt`), Throughput de Rede (RX/TX KB/s), Uso de Discos e Arrays RAID (`/proc/mdstat`).

2. **🌀 Smart Fan 5 (Controle de Velocidade da Fan Argon)**:
   - Editor gráfico interativo 2D (arraste os pontos de temperatura x PWM % diretamente com o mouse).
   - Suporte para curva de **CPU (`/etc/argononed.conf`)** e **HDD/SSD (`/etc/argononed-hdd.conf`)**.
   - Perfis 1-Click: **SILENT**, **STANDARD (Padrão Argon)**, **PERFORMANCE** e **FULL SPEED (100%)**.
   - **Override Manual Imediato via Barramento I2C (`0x1a`)**: Defina 0%, 30%, 55%, 75%, 100% ou qualquer valor via slider instantaneamente.

3. **🔥 Overclocking (AORUS Engine)**:
   - **Ajuste Dinâmico em Tempo Real (Sem Reboot)**: Altera `scaling_governor` (`performance`, `ondemand`, `schedutil`, `powersave`, `conservative`), frequências mínima/máxima (`scaling_min_freq` / `scaling_max_freq`) e `boost` em todos os núcleos via `sysfs`.
   - **Overclocking de Firmware (`/boot/firmware/config.txt`)**:
     - Perfis 1-Click: **ECO Mode (1500 MHz)**, **Stock Boost (1800 MHz)**, **AORUS OC Gaming (2000 MHz CPU / 650 MHz GPU)** e **AORUS Extreme (2147 MHz CPU / 750 MHz GPU)**.
     - Sliders individuais para `arm_freq`, `gpu_freq`, `over_voltage` (com estimativa de tensão em Volts), `temp_limit`, `arm_boost` e `force_turbo`.

4. **🛠 Recursos & Hardware**:
   - Ativar/desativar barramentos e overlays no `config.txt`: **I2C (`dtparam=i2c_arm=on`)**, **SPI**, **I2S**, **Áudio Onboard**, **UART Serial**, **Driver 3D VC4 KMS V3D**, **Auto-Detect Câmera CSI / Display DSI**.
   - **Argon BLSTR DAC (`dtoverlay=hifiberry-dacplus,slave`)** e **Receptor Argon IR (`dtoverlay=gpio-ir,gpio_pin=23`)**.
   - Configuração da **Unidade de Temperatura (`/etc/argonunits.conf`)**: Celsius (°C) ou Fahrenheit (°F).
   - Configuração do **Painel OLED Argon (`/etc/argoneonoled.conf`)** e **EEPROM (`PSU_MAX_CURRENT=5000`)**.

5. **⚙ Serviços & Logs**:
   - Controle direto do daemon `argononed.service` (`start`, `stop`, `restart`, `enable`).
   - Elevação de privilégios integrada via **Polkit (`pkexec`)** para gravar arquivos do sistema com segurança.

## Como Executar

Com o `mise` instalado:

```bash
# Verificar telemetria via terminal
mise exec -- cargo run -- --status

# Executar a interface gráfica
mise run run
```
