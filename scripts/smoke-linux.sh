#!/usr/bin/env bash
# Install-and-launch smoke test for the native Linux packages (TST-094, T-004a).
# Runs as root inside a clean distro container (see .github/workflows/linux-native.yml).
#   scripts/smoke-linux.sh <deb|rpm|pkg|tarball|appimage> <file>
# deb, rpm and pkg are installed with the distro's own tool, which must pull in every
# dependency (no manual steps). The tarball and AppImage bring no libraries, so for those
# the generic library packages are installed first, as a user of any distro would have them.
set -euo pipefail
kind=${1:?kind}; file=$(readlink -f "${2:?file}")

. /etc/os-release
family=${ID_LIKE:-$ID}
case " $ID $family " in
    *" debian "*|*" ubuntu "*) pm=apt ;;
    *" fedora "*|*" rhel "*)   pm=dnf ;;
    *" arch "*)                pm=pacman ;;
    *) echo "smoke: unknown distro $ID" >&2; exit 2 ;;
esac

# Xvfb gives the app a display; the mesa drivers give it software OpenGL.
case $pm in
    apt)
        export DEBIAN_FRONTEND=noninteractive
        apt-get update -qq
        extra="xvfb xauth libgl1-mesa-dri"
        libs="libfontconfig1 libxkbcommon0 libxkbcommon-x11-0 libwayland-client0 libwayland-cursor0 libwayland-egl1 libx11-6 libx11-xcb1 libxcb1 libxcursor1 libxi6 libgl1 libegl1 libdbus-1-3 ca-certificates"
        [ "$kind" = appimage ] && extra="$extra file"
        install_file() { apt-get install -y -qq "$@"; }
        ;;
    dnf)
        extra="xorg-x11-server-Xvfb xauth mesa-dri-drivers"
        libs="fontconfig libxkbcommon libxkbcommon-x11 libwayland-client libwayland-cursor libwayland-egl libX11 libX11-xcb libxcb libXcursor libXi mesa-libGL mesa-libEGL dbus-libs ca-certificates"
        install_file() { dnf install -y -q "$@"; }
        ;;
    pacman)
        pacman -Syu --noconfirm
        extra="xorg-server-xvfb xorg-xauth mesa ttf-dejavu"
        libs="fontconfig libxkbcommon libxkbcommon-x11 wayland libx11 libxcb libxcursor libxi libglvnd dbus ca-certificates"
        install_file() { pacman -S --noconfirm --needed "$@"; }
        ;;
esac

case $kind in
    deb)  [ "$pm" = apt ]    || exit 2; apt-get install -y -qq "$file"; install_file $extra ;;
    rpm)  [ "$pm" = dnf ]    || exit 2; dnf install -y -q "$file"; install_file $extra ;;
    pkg)  [ "$pm" = pacman ] || exit 2; pacman -U --noconfirm "$file"; install_file $extra ;;
    tarball|appimage) install_file $libs $extra ;;
    *) echo "smoke: unknown kind $kind" >&2; exit 2 ;;
esac

# The package_format file (FR-SET-005) holds this package's word.
case $kind in pkg) want_format=aur ;; *) want_format=$kind ;; esac

case $kind in
    tarball)
        d=$(mktemp -d); tar -xzf "$file" -C "$d"
        app=$(echo "$d"/stalemate-*/stalemate); real=$app
        test "$(cat "$(dirname "$app")/package_format")" = tarball ;;
    appimage)
        cp "$file" /tmp/stalemate.AppImage; chmod +x /tmp/stalemate.AppImage
        app="/tmp/stalemate.AppImage --appimage-extract-and-run"; real=""
        (cd /tmp && /tmp/stalemate.AppImage --appimage-extract >/dev/null)
        test "$(cat /tmp/squashfs-root/usr/share/stalemate/package_format)" = appimage ;;
    *)
        app=stalemate; real=/usr/lib/stalemate/stalemate
        command -v stalemate >/dev/null
        test -f /usr/share/stalemate/config/rules.toml
        test -d /usr/share/stalemate/i18n/en-US
        test "$(cat /usr/share/stalemate/package_format)" = "$want_format"
        test -f /usr/share/applications/io.github.hatchbear.stalemate.desktop ;;
esac

# Every linked library resolves.
if [ -n "$real" ]; then
    if ldd "$real" | grep 'not found'; then echo "smoke: missing libraries" >&2; exit 1; fi
    echo "ldd: all libraries resolve"
fi

# Version check. Once the app has --version it must print the package_format word (FR-SET-005).
# Until then (non-zero exit) this is a warning.
set +e
v=$($app --version 2>&1); rc=$?
set -e
if [ $rc -eq 0 ]; then
    echo "version: $v"
    case $v in
        *"$want_format"*) ;;
        *) echo "smoke: '--version' output lacks package format '$want_format'" >&2; exit 1 ;;
    esac
else
    echo "smoke WARNING: '--version' not supported yet ($v)"
fi

# Start the window headless and let it close itself. Needs --demo (no core wiring yet).
export LIBGL_ALWAYS_SOFTWARE=1
xvfb-run -a $app --demo --quit-after-ms 1500
echo "smoke OK: $kind on $ID"
