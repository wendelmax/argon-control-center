#!/bin/sh
# Writes the AUR package for a released version into an AUR git checkout.
#   aur-publish.sh <version> <aur-checkout-dir>
# The PKGBUILD gets the checksum of the GitHub release tarball and .SRCINFO is
# regenerated with makepkg (run as a regular user on Arch Linux).
set -eu
VERSION="${1:?usage: aur-publish.sh <version> <aur-checkout-dir>}"
OUT="${2:?usage: aur-publish.sh <version> <aur-checkout-dir>}"
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"

case "$VERSION" in
    [0-9]*.[0-9]*.[0-9]*) ;;
    *) echo "error: invalid version '$VERSION'" >&2; exit 1 ;;
esac

URL="https://github.com/wendelmax/argon-control-center/archive/v$VERSION.tar.gz"
SUM="$(curl -fsSL "$URL" | sha256sum | cut -d' ' -f1)"
[ "${#SUM}" -eq 64 ] || { echo "error: could not download $URL" >&2; exit 1; }

sed -e "s/^pkgver=.*/pkgver=$VERSION/" \
    -e "s/^sha256sums=.*/sha256sums=('$SUM')/" \
    -e '/^# Replaced with the checksum/d' \
    "$ROOT/packaging/arch/PKGBUILD" > "$OUT/PKGBUILD"
cp "$ROOT/packaging/arch/argon-control-center.install" "$OUT/"
(cd "$OUT" && makepkg --printsrcinfo > .SRCINFO)
echo "AUR package for $VERSION written to $OUT (sha256 $SUM)"
