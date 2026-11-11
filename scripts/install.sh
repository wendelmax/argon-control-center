#!/usr/bin/env bash
# Installs Argon Control Center for the current user (no root needed).
#   scripts/install.sh            # build + install to ~/.local
#   PREFIX=/usr/local sudo -E scripts/install.sh   # system-wide
#   scripts/install.sh --uninstall
set -euo pipefail

PREFIX="${PREFIX:-$HOME/.local}"
BIN_DIR="$PREFIX/bin"
APP_DIR="$PREFIX/share/applications"
BIN_NAME="argon-control-center"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

WIDGET_DIR="$PREFIX/share/plasma/plasmoids"
ICON_DIR="$PREFIX/share/icons/hicolor/scalable/apps"

if [[ "${1:-}" == "--uninstall" ]]; then
    rm -f "$BIN_DIR/$BIN_NAME" "$APP_DIR/$BIN_NAME.desktop" "$ICON_DIR/$BIN_NAME.svg" "$PREFIX/share/metainfo/io.github.wendelmax.argon_control_center.metainfo.xml"
    rm -rf "$WIDGET_DIR"/org.argon.controlcenter.*
    rm -f "$PREFIX/share/man/man1/argon-control-center.1.gz"
    command -v update-desktop-database >/dev/null && update-desktop-database "$APP_DIR" 2>/dev/null || true
    echo "Removed: $BIN_DIR/$BIN_NAME, $APP_DIR/$BIN_NAME.desktop"
    exit 0
fi

cd "$ROOT"
if command -v mise >/dev/null; then
    mise exec -- cargo build --release
else
    cargo build --release
fi

install -Dm755 "target/release/$BIN_NAME" "$BIN_DIR/$BIN_NAME"
install -d "$APP_DIR"
# The .desktop in the repo uses the bare binary name; pin the absolute path in case
# $BIN_DIR is not in the desktop session's PATH.
sed "s|^Exec=.*|Exec=$BIN_DIR/$BIN_NAME|" "$BIN_NAME.desktop" > "$APP_DIR/$BIN_NAME.desktop"
chmod 644 "$APP_DIR/$BIN_NAME.desktop"
command -v update-desktop-database >/dev/null && update-desktop-database "$APP_DIR" 2>/dev/null || true

# Application icon
install -Dm644 "assets/$BIN_NAME.svg" "$ICON_DIR/$BIN_NAME.svg"
command -v gtk-update-icon-cache >/dev/null && gtk-update-icon-cache -qtf "$PREFIX/share/icons/hicolor" 2>/dev/null || true

# AppStream metadata (software centers)
install -Dm644 "packaging/common/io.github.wendelmax.argon_control_center.metainfo.xml" "$PREFIX/share/metainfo/io.github.wendelmax.argon_control_center.metainfo.xml"

# Manual page (man argon-control-center)
install -d "$PREFIX/share/man/man1"
gzip -9n -c docs/argon-control-center.1 > "$PREFIX/share/man/man1/argon-control-center.1.gz"

# Plasma 6 desktop widgets (Add Widgets > "Argon")
sh packaging/plasma/stage.sh "$WIDGET_DIR"

echo "Installed: $BIN_DIR/$BIN_NAME"
echo "Plasma widgets: $WIDGET_DIR (Add Widgets > Argon)"
echo "Menu entry: $APP_DIR/$BIN_NAME.desktop"
case ":$PATH:" in
    *":$BIN_DIR:"*) ;;
    *) echo "Warning: $BIN_DIR is not in PATH" ;;
esac
