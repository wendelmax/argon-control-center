#!/bin/sh
# Stages the files shared by every package into DESTDIR.
#   install-tree.sh <DESTDIR> <path-to-binary>
set -eu
DEST="$1"
BIN="$2"
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
C="$ROOT/packaging/common"

install -Dm755 "$BIN" "$DEST/usr/bin/argon-control-center"
install -Dm644 "$ROOT/argon-control-center.desktop" "$DEST/usr/share/applications/argon-control-center.desktop"
install -Dm644 "$C/60-argon-control-center.rules" "$DEST/usr/lib/udev/rules.d/60-argon-control-center.rules"
install -Dm644 "$C/argon-control-center.sysusers" "$DEST/usr/lib/sysusers.d/argon-control-center.conf"
install -Dm644 "$C/argon-control-center.modules-load" "$DEST/usr/lib/modules-load.d/argon-control-center.conf"
install -Dm755 "$C/setup-permissions.sh" "$DEST/usr/lib/argon-control-center/setup-permissions.sh"
install -Dm644 "$ROOT/README.md" "$DEST/usr/share/doc/argon-control-center/README.md"
