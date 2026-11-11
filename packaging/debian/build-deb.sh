#!/bin/sh
# Builds a .deb for Debian / Ubuntu / Raspberry Pi OS / Armbian with plain dpkg-deb.
#   packaging/debian/build-deb.sh            # cargo build --release, then package
#   BIN=path/to/binary packaging/debian/build-deb.sh   # package an existing binary
# Output: dist/argon-control-center_<version>_<arch>.deb
set -eu
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT"

VERSION="$(sed -n 's/^version *= *"\(.*\)"/\1/p' Cargo.toml | head -1)"
ARCH="$(dpkg --print-architecture)"
if [ -z "${BIN:-}" ]; then
    if command -v mise >/dev/null 2>&1; then mise exec -- cargo build --release; else cargo build --release; fi
    BIN=target/release/argon-control-center
fi

STAGE="$(mktemp -d)"
chmod 755 "$STAGE"
trap 'rm -rf "$STAGE"' EXIT
sh packaging/common/install-tree.sh "$STAGE" "$BIN"
strip "$STAGE/usr/bin/argon-control-center" 2>/dev/null || true
# Debian machine-readable copyright file
{
    echo "Format: https://www.debian.org/doc/packaging-manuals/copyright-format/1.0/"
    echo "Upstream-Name: argon-control-center"
    echo "Source: https://github.com/wendelmax/argon-control-center"
    echo
    echo "Files: *"
    echo "Copyright: 2026 wendelmax"
    echo "License: MIT"
    sed 's/^$/./; s/^/ /' LICENSE | tail -n +4
} > "$STAGE/usr/share/doc/argon-control-center/copyright"

install -d "$STAGE/DEBIAN"
SIZE_KB="$(du -sk "$STAGE" | cut -f1)"
sed -e "s/@VERSION@/$VERSION/" -e "s/@ARCH@/$ARCH/" -e "s/@SIZE@/$SIZE_KB/" \
    packaging/debian/control.in > "$STAGE/DEBIAN/control"
install -m755 packaging/debian/postinst "$STAGE/DEBIAN/postinst"
install -m755 packaging/debian/prerm "$STAGE/DEBIAN/prerm"
install -m755 packaging/debian/postrm "$STAGE/DEBIAN/postrm"

mkdir -p dist
OUT="dist/argon-control-center_${VERSION}_${ARCH}.deb"
dpkg-deb --root-owner-group --build "$STAGE" "$OUT"
echo "$OUT"
