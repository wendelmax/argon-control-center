#!/usr/bin/env bash
# Semantic versioning helper (MAJOR.MINOR.PATCH).
#
#   scripts/bump-version.sh patch    # 0.1.0 -> 0.1.1  bug fixes only
#   scripts/bump-version.sh minor    # 0.1.1 -> 0.2.0  new backwards-compatible features
#   scripts/bump-version.sh major    # 0.2.0 -> 1.0.0  breaking changes
#   scripts/bump-version.sh --tag    # tag the current commit with the Cargo.toml version
#
# A bump updates Cargo.toml, Cargo.lock, the RPM spec and the PKGBUILD, and turns the
# [Unreleased] section of CHANGELOG.md into the new version. Commit it through a PR; once it
# is merged, run --tag on main and push the tag to publish the release.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
REPO_URL="https://github.com/wendelmax/argon-control-center"
SPEC=packaging/rpm/argon-control-center.spec
PKGBUILD=packaging/arch/PKGBUILD

current_version() {
    sed -n 's/^version *= *"\([0-9]*\.[0-9]*\.[0-9]*\)".*/\1/p' Cargo.toml | head -1
}

die() { echo "error: $*" >&2; exit 1; }

CUR="$(current_version)"
[ -n "$CUR" ] || die "could not read version from Cargo.toml"

if [ "${1:-}" = "--tag" ]; then
    TAG="v$CUR"
    [ -z "$(git status --porcelain --untracked-files=no)" ] || die "working tree has uncommitted changes"
    BRANCH="$(git rev-parse --abbrev-ref HEAD)"
    [ "$BRANCH" = "main" ] || die "tag releases from main (current branch: $BRANCH)"
    git rev-parse -q --verify "refs/tags/$TAG" >/dev/null && die "tag $TAG already exists"
    grep -q "^## \[$CUR\]" CHANGELOG.md || die "CHANGELOG.md has no section for $CUR"
    git tag -a "$TAG" -m "Argon Control Center $CUR"
    echo "Created tag $TAG. Publish it with:"
    echo "  git push origin $TAG"
    exit 0
fi

PART="${1:-}"
IFS=. read -r MA MI PA <<<"$CUR"
case "$PART" in
    major) NEW="$((MA + 1)).0.0" ;;
    minor) NEW="$MA.$((MI + 1)).0" ;;
    patch) NEW="$MA.$MI.$((PA + 1))" ;;
    *) die "usage: bump-version.sh major|minor|patch|--tag" ;;
esac

if ! awk '/^## \[Unreleased\]/{f=1;next} /^## \[/{exit} f && /[^[:space:]]/{found=1} END{exit !found}' CHANGELOG.md; then
    echo "warning: the [Unreleased] section of CHANGELOG.md is empty" >&2
fi

TODAY="$(date +%Y-%m-%d)"
SPEC_DATE="$(LC_ALL=C date '+%a %b %d %Y')"

# Cargo.toml: only the [package] version line.
sed -i "0,/^version *= *\"$CUR\"/s//version = \"$NEW\"/" Cargo.toml
# Cargo.lock: the entry for this package.
awk -v new="$NEW" '
    /^name = "argon-control-center"$/ { print; getline; sub(/"[^"]*"/, "\"" new "\""); print; next }
    { print }
' Cargo.lock > Cargo.lock.tmp && mv Cargo.lock.tmp Cargo.lock

# RPM spec: version, release 1, changelog entry.
sed -i -e "s/^Version: .*/Version:        $NEW/" -e "s/^Release: .*/Release:        1%{?dist}/" "$SPEC"
sed -i "/^%changelog$/a * $SPEC_DATE wendelmax - $NEW-1\n- Release $NEW\n" "$SPEC"

# PKGBUILD: version, release 1.
sed -i -e "s/^pkgver=.*/pkgver=$NEW/" -e "s/^pkgrel=.*/pkgrel=1/" "$PKGBUILD"

# CHANGELOG.md: new section under [Unreleased], comparison links.
sed -i "s/^## \[Unreleased\]$/## [Unreleased]\n\n## [$NEW] - $TODAY/" CHANGELOG.md
sed -i "s#^\[Unreleased\]: .*#[Unreleased]: $REPO_URL/compare/v$NEW...HEAD\n[$NEW]: $REPO_URL/compare/v$CUR...v$NEW#" CHANGELOG.md

echo "Bumped $CUR -> $NEW ($PART)."
echo "Next: review the diff, open a PR, and after merging run: scripts/bump-version.sh --tag"
