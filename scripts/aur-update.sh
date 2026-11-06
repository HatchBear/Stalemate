#!/usr/bin/env bash
# Sets pkgver and the source checksum in packaging/aur/PKGBUILD and .SRCINFO (T-004a).
#   scripts/aur-update.sh <version or tag> [sha256]
# Without a checksum it downloads the tag's tarball from GitHub and hashes it.
# It edits files only. Pushing to the AUR is a manual step by the author (DEC-064).
set -euo pipefail
cd "$(dirname "$0")/.."
ver=${1:?usage: aur-update.sh <version or tag> [sha256]}
ver=${ver#v}
case $ver in *-*) echo "aur-update: pre-release tags ($ver) are not published to the AUR" >&2; exit 3 ;; esac
sum=${2:-}
if [ -z "$sum" ]; then
    url="https://github.com/HatchBear/Stalemate/archive/refs/tags/v$ver.tar.gz"
    sum=$(curl -fsSL "$url" | sha256sum | cut -d' ' -f1)
fi
printf '%s' "$sum" | grep -Eq '^[0-9a-f]{64}$' || { echo "aur-update: bad checksum: $sum" >&2; exit 1; }
cd packaging/aur
sed -i -e "s/^pkgver=.*/pkgver=$ver/" -e "s/^pkgrel=.*/pkgrel=1/" \
    -e "s/^sha256sums=('[^']*')/sha256sums=('$sum')/" PKGBUILD
sed -i -e "s/^\(\s*pkgver = \).*/\1$ver/" -e "s/^\(\s*pkgrel = \).*/\11/" \
    -e "s#^\(\s*source = stalemate-\)[^:]*\(.tar.gz::.*/tags/v\)[^/]*\(.tar.gz\)#\1$ver\2$ver\3#" \
    -e "s/^\(\s*sha256sums = \).*/\1$sum/" .SRCINFO
echo "PKGBUILD and .SRCINFO now at $ver ($sum)"
