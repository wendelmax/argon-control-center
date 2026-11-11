#!/bin/sh
# Argon Control Center: hardware permission setup. Must run as root.
#
#   setup-permissions.sh [USER]
#
# - creates the i2c and gpio groups (if missing)
# - installs the udev rules when not already shipped by a distro package
# - loads i2c-dev now and at every boot
# - reloads udev so /dev/i2c-* and /dev/gpiochip* get the new group/mode
# - removes setuid bits that very old versions of this app set on GPIO/I2C tools
# - adds USER to i2c, gpio, video (firmware mailbox for vcgencmd) and the serial group
#   (dialout on Debian/Fedora, uucp on Arch)
# - prints which optional tools and hardware interfaces are available
set -eu
umask 022

if [ "$(id -u)" -ne 0 ]; then
    echo "setup-permissions.sh must run as root (try: sudo $0 ${1:-\$USER})" >&2
    exit 1
fi

TARGET_USER="${1:-}"
# Only trust a sibling rules file when run as a real file (not via `sh -s` from stdin).
case "$0" in
    */setup-permissions.sh) SELF_DIR="$(cd "$(dirname "$0")" && pwd)" ;;
    *) SELF_DIR="" ;;
esac

for g in i2c gpio; do
    getent group "$g" >/dev/null 2>&1 || groupadd --system "$g"
done

SERIAL_GROUP=dialout
getent group dialout >/dev/null 2>&1 || SERIAL_GROUP=uucp
getent group "$SERIAL_GROUP" >/dev/null 2>&1 || SERIAL_GROUP=""

PKG_RULES=/usr/lib/udev/rules.d/60-argon-control-center.rules
if [ ! -f "$PKG_RULES" ] && [ ! -f /lib/udev/rules.d/60-argon-control-center.rules ]; then
    if [ -n "$SELF_DIR" ] && [ -f "$SELF_DIR/60-argon-control-center.rules" ]; then
        install -Dm644 "$SELF_DIR/60-argon-control-center.rules" /etc/udev/rules.d/60-argon-control-center.rules
    elif [ -n "${ARGONCC_RULES:-}" ]; then
        mkdir -p /etc/udev/rules.d
        printf '%s\n' "$ARGONCC_RULES" > /etc/udev/rules.d/60-argon-control-center.rules
        chmod 644 /etc/udev/rules.d/60-argon-control-center.rules
    fi
fi

if [ ! -f /usr/lib/modules-load.d/argon-control-center.conf ]; then
    mkdir -p /etc/modules-load.d
    echo i2c-dev > /etc/modules-load.d/argon-control-center.conf
fi
modprobe i2c-dev 2>/dev/null || true

for b in /usr/bin/pinctrl /usr/bin/gpioget /usr/bin/gpioset /usr/bin/gpioinfo \
         /usr/sbin/i2cdetect /usr/sbin/i2cdump /usr/sbin/i2cget /usr/sbin/i2cset; do
    if [ -u "$b" ]; then chmod u-s "$b"; fi
done

if command -v udevadm >/dev/null 2>&1; then
    udevadm control --reload-rules || true
    udevadm trigger --subsystem-match=i2c-dev --subsystem-match=gpio --action=change || true
    udevadm trigger --sysname-match='gpiomem*' --action=change 2>/dev/null || true
    udevadm trigger --sysname-match=vcio --action=change 2>/dev/null || true
fi

if [ -n "$TARGET_USER" ] && [ "$TARGET_USER" != "root" ] && id "$TARGET_USER" >/dev/null 2>&1; then
    GROUPS_TO_ADD="i2c,gpio"
    getent group video >/dev/null 2>&1 && GROUPS_TO_ADD="$GROUPS_TO_ADD,video"
    GROUPS_TO_ADD="$GROUPS_TO_ADD${SERIAL_GROUP:+,$SERIAL_GROUP}"
    usermod -aG "$GROUPS_TO_ADD" "$TARGET_USER"
    echo "Added $TARGET_USER to: $GROUPS_TO_ADD (log out and back in to apply)."
fi
echo "Hardware permissions configured."

# Report of optional tools and interfaces (missing ones only disable the related features).
report() {
    if [ "$2" = ok ]; then printf '  %-10s %s\n' "[ok]" "$1"; else printf '  %-10s %s  -> %s\n' "[missing]" "$1" "$3"; fi
}
have() { command -v "$1" >/dev/null 2>&1 && echo ok || echo no; }
exists() { [ -e "$1" ] && echo ok || echo no; }
V3D_STATS=$(ls /sys/devices/platform/*/*.v3d/gpu_stats /sys/devices/platform/*.v3d/gpu_stats /sys/devices/platform/*/*/*.v3d/gpu_stats 2>/dev/null | head -n1)
echo "Optional tools and interfaces:"
report "vcgencmd (GPU clocks, throttling flags)" "$(have vcgencmd)" "Debian/Ubuntu: raspi-utils-core or libraspberrypi-bin; Arch: raspberrypi-utils"
report "/dev/vcio (firmware mailbox)" "$(exists /dev/vcio)" "Raspberry Pi kernel with the vcio driver"
report "V3D gpu_stats (GPU load)" "$([ -n "$V3D_STATS" ] && echo ok || echo no)" "Linux 6.8 or newer with the v3d driver"
report "i2c-tools (I2C scan)" "$(have i2cdetect)" "package i2c-tools"
report "gpiod tools" "$(have gpioget)" "Debian/Ubuntu: gpiod; Fedora: libgpiod-utils; Arch: libgpiod"
report "pinctrl (GPIO analyzer)" "$(have pinctrl)" "Raspberry Pi OS: raspi-utils"
report "pkexec (Polkit)" "$(have pkexec)" "Debian/Ubuntu: pkexec; Fedora/Arch: polkit"
