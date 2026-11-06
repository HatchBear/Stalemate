#!/usr/bin/env bash
# Builds packaging/aur/PKGBUILD in an Arch container from a local source tarball, then runs
# the install smoke test on the result (TST-094, T-004a). Runs as root in archlinux:latest.
#   scripts/smoke-aur.sh <source tarball> <repo dir>
# The tarball must unpack to Stalemate-<pkgver>/ (git archive --prefix=Stalemate-<pkgver>/).
set -euo pipefail
tarball=$(readlink -f "${1:?source tarball}"); repo=$(readlink -f "${2:?repo dir}")
pkgver=$(sed -n 's/^pkgver=//p' "$repo/packaging/aur/PKGBUILD")

pacman -Syu --noconfirm --needed base-devel sudo
useradd -m builder
echo 'builder ALL=(ALL) NOPASSWD: ALL' > /etc/sudoers.d/builder
b=/home/builder/aur
mkdir -p "$b"
cp "$repo/packaging/aur/PKGBUILD" "$b/"
# makepkg uses a file with the expected name instead of downloading it.
cp "$tarball" "$b/stalemate-$pkgver.tar.gz"
chown -R builder:builder /home/builder
su builder -c "cd $b && makepkg -s --noconfirm --skipchecksums"
pkg=$(ls "$b"/stalemate-"$pkgver"-1-x86_64.pkg.tar.zst)
bash "$repo/scripts/smoke-linux.sh" pkg "$pkg"
