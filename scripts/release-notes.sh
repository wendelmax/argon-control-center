#!/usr/bin/env bash
# Prints the GitHub release notes for a tag.
#
#   scripts/release-notes.sh v0.1.0 [dist-dir]
#
# Body: the matching "## [x.y.z]" section of CHANGELOG.md. When the changelog has
# no such section, commits since the previous tag are grouped by Conventional
# Commit type. Installation instructions (with the real asset names found in
# dist-dir), checksum instructions and a comparison link are appended.
# Emoji and other pictographic symbols are removed from the final text.
set -euo pipefail

TAG="${1:?usage: release-notes.sh <tag> [dist-dir]}"
DIST="${2:-dist}"
VERSION="${TAG#v}"
REPO_URL="https://github.com/${GITHUB_REPOSITORY:-wendelmax/argon-control-center}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

PREV_TAG="$(git describe --tags --abbrev=0 --match 'v[0-9]*.[0-9]*.[0-9]*' "${TAG}^" 2>/dev/null || true)"

changelog_section() {
    awk -v ver="$VERSION" '
        $0 ~ "^## \\[" ver "\\]" { found = 1; next }
        found && /^## \[/ { exit }
        found && /^\[[^]]+\]: / { exit }
        found { print }
    ' CHANGELOG.md 2>/dev/null | sed -e :a -e '/^\n*$/{$d;N;ba' -e '}'
}

commit_summary() {
    local range="$TAG"
    [ -n "$PREV_TAG" ] && range="$PREV_TAG..$TAG"
    local feats fixes others
    feats="$(git log --no-merges --format='%s' "$range" | sed -n 's/^feat\(([^)]*)\)\{0,1\}!\{0,1\}: */- /p')"
    fixes="$(git log --no-merges --format='%s' "$range" | sed -n 's/^fix\(([^)]*)\)\{0,1\}!\{0,1\}: */- /p')"
    others="$(git log --no-merges --format='%s' "$range" | grep -vE '^(feat|fix)(\(|:|!)' | sed 's/^[a-z]*\(([^)]*)\)\{0,1\}!\{0,1\}: *//; s/^/- /' || true)"
    cap() { sed 's/^- \(.\)/- \u\1/'; }
    feats="$(printf '%s' "$feats" | cap)"
    fixes="$(printf '%s' "$fixes" | cap)"
    others="$(printf '%s' "$others" | cap)"
    [ -n "$feats" ] && printf '### Features\n%s\n\n' "$feats"
    [ -n "$fixes" ] && printf '### Fixes\n%s\n\n' "$fixes"
    [ -n "$others" ] && printf '### Other changes\n%s\n\n' "$others"
    return 0
}

asset() {
    # First file in DIST matching the glob, or a placeholder name.
    local f
    f="$(ls "$DIST"/$1 2>/dev/null | head -1 || true)"
    if [ -n "$f" ]; then basename "$f"; else echo "$2"; fi
}

DEB_ARM="$(asset "*_${VERSION}_arm64.deb" "argon-control-center_${VERSION}_arm64.deb")"
DEB_AMD="$(asset "*_${VERSION}_amd64.deb" "argon-control-center_${VERSION}_amd64.deb")"
RPM_ARM="$(asset "*-${VERSION}-*.aarch64.rpm" "argon-control-center-${VERSION}-1.aarch64.rpm")"
RPM_X86="$(asset "*-${VERSION}-*.x86_64.rpm" "argon-control-center-${VERSION}-1.x86_64.rpm")"
ARCH_PKG="$(asset "argon-control-center-${VERSION}-*.pkg.tar.zst" "argon-control-center-${VERSION}-1-x86_64.pkg.tar.zst")"

{
    BODY="$(changelog_section)"
    if [ -z "$BODY" ]; then
        BODY="$(commit_summary)"
    fi
    printf '%s\n\n' "$BODY"

    cat <<EOF
## Installation

| Distribution | Architecture | Package |
|---|---|---|
| Debian, Ubuntu, Raspberry Pi OS, Armbian | arm64 | \`$DEB_ARM\` |
| Debian, Ubuntu | amd64 | \`$DEB_AMD\` |
| Fedora | aarch64 | \`$RPM_ARM\` |
| Fedora | x86_64 | \`$RPM_X86\` |
| Arch Linux | x86_64 | \`$ARCH_PKG\` |

Debian / Ubuntu:

\`\`\`sh
sudo apt install ./$DEB_ARM
\`\`\`

Fedora:

\`\`\`sh
sudo dnf install ./$RPM_ARM
\`\`\`

Arch Linux (x86_64 package, or build from \`packaging/arch/PKGBUILD\` on Arch Linux ARM):

\`\`\`sh
sudo pacman -U ./$ARCH_PKG
\`\`\`

The packages configure hardware access (udev rules, \`i2c\`, \`gpio\` and \`video\` groups, \`i2c-dev\` at boot) and add the user who ran \`sudo\` to the hardware groups. Log out and back in afterwards. See the [README]($REPO_URL#permissions) for source installs.

## Verification

\`\`\`sh
sha256sum --check --ignore-missing SHA256SUMS
\`\`\`
EOF

    if [ -n "$PREV_TAG" ]; then
        printf '\n**Full changelog:** %s/compare/%s...%s\n' "$REPO_URL" "$PREV_TAG" "$TAG"
    else
        printf '\n**Full changelog:** %s/commits/%s\n' "$REPO_URL" "$TAG"
    fi
} | perl -CSD -pe 's/[\x{1F000}-\x{1FAFF}\x{2600}-\x{27BF}\x{2B00}-\x{2BFF}\x{1F1E6}-\x{1F1FF}\x{FE0E}\x{FE0F}\x{200D}\x{20E3}]+ ?//g; s/[ \t]+$//'
