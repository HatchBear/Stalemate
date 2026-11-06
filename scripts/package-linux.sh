#!/usr/bin/env bash
# Native Linux packages (T-004a, DEC-064, NFR-PLAT-005). x86_64.
#
#   scripts/package-linux.sh stage [DEST] [BINARY]   lay out the install tree (default DEST target/package/linux/root)
#   scripts/package-linux.sh tarball | deb | rpm | appimage
#   scripts/package-linux.sh aur-srcinfo
#
# Run `cargo build --release -p stalemate-app` first. Output goes to target/package/linux/.
# VERSION overrides the version from Cargo.toml (a tag such as v0.1.0 is accepted).
#
# Install layout (the app reads STALEMATE_SHARE_DIR first, crates/core/src/settings.rs):
#   /usr/bin/stalemate                 wrapper that sets STALEMATE_SHARE_DIR=/usr/share/stalemate
#   /usr/lib/stalemate/stalemate       the binary
#   /usr/share/stalemate/{config,i18n} files the app reads at run time
#   /usr/share/stalemate/package_format one word (aur, deb, rpm, appimage, tarball) for the
#                                      diagnostics zip (FR-SET-005); written per package here
# Only the main window is packaged. The overlay is Beta. No Valve art is packaged.
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"

out=target/package/linux
bin=${BINARY:-${CARGO_TARGET_DIR:-target}/release/stalemate}
app_id=io.github.hatchbear.stalemate

version=${VERSION:-}
if [ -z "$version" ]; then
    version=$(sed -n 's/^version *= *"\([^"]*\)".*/\1/p' Cargo.toml | head -1)
fi
version=${version#v}

# Final icon files win; the Alpha placeholders are the fallback (TST-110 warns about them).
icon_file() { # <name>
    if [ -f "assets/icon/$1" ]; then echo "assets/icon/$1"; return; fi
    local p="assets/placeholders/${1%.*}.placeholder.${1##*.}"
    [ -f "$p" ] && echo "$p"
}

stage() {
    local dest=${1:-$out/root} binary=${2:-$bin}
    [ -f "$binary" ] || { echo "package-linux: $binary not found. Run: cargo build --release -p stalemate-app" >&2; exit 1; }
    rm -rf "$dest"
    install -d "$dest/usr/bin" "$dest/usr/lib/stalemate" "$dest/usr/share/stalemate"
    install -m755 "$binary" "$dest/usr/lib/stalemate/stalemate"
    if command -v strip >/dev/null 2>&1; then strip "$dest/usr/lib/stalemate/stalemate" || true; fi
    cat > "$dest/usr/bin/stalemate" <<'WRAP'
#!/bin/sh
# Points the app at its shipped config and i18n folders, then starts the real binary.
STALEMATE_SHARE_DIR=${STALEMATE_SHARE_DIR:-/usr/share/stalemate}
export STALEMATE_SHARE_DIR
exec /usr/lib/stalemate/stalemate "$@"
WRAP
    chmod 755 "$dest/usr/bin/stalemate"
    cp -r config i18n "$dest/usr/share/stalemate/"
    # models/ and prompts/ are empty in the repository today; copy them when they have files.
    for d in models prompts; do
        if [ -n "$(ls -A "$d" 2>/dev/null | grep -v '^.gitkeep$')" ]; then cp -r "$d" "$dest/usr/share/stalemate/"; fi
    done
    find "$dest/usr/share/stalemate" -name .gitkeep -delete
    # Sibling of config/. The binary is shared by several formats, so this is written at staging.
    echo "${PACKAGE_FORMAT:-tarball}" > "$dest/usr/share/stalemate/package_format"
    install -Dm644 "packaging/flatpak/$app_id.desktop" "$dest/usr/share/applications/$app_id.desktop"
    install -Dm644 "packaging/flatpak/$app_id.metainfo.xml" "$dest/usr/share/metainfo/$app_id.metainfo.xml"
    install -Dm644 LICENSE "$dest/usr/share/doc/stalemate/LICENSE"
    local svg png
    svg=$(icon_file ast-icon-001-app.svg)
    [ -n "$svg" ] && install -Dm644 "$svg" "$dest/usr/share/icons/hicolor/scalable/apps/$app_id.svg"
    for s in 16 24 32 48 64 128 256 512; do
        png=$(icon_file "ast-icon-002-app-$s.png")
        [ -n "$png" ] && install -Dm644 "$png" "$dest/usr/share/icons/hicolor/${s}x${s}/apps/$app_id.png"
    done
    return 0
}

tarball() {
    local name=stalemate-$version-linux-x86_64 t=$out/tar
    rm -rf "$t"; mkdir -p "$t/$name"
    install -m755 "$bin" "$t/$name/stalemate"
    if command -v strip >/dev/null 2>&1; then strip "$t/$name/stalemate" || true; fi
    # The app finds config/ and i18n/ next to the executable.
    cp -r config i18n "$t/$name/"
    find "$t/$name" -name .gitkeep -delete
    echo tarball > "$t/$name/package_format"
    install -Dm644 "packaging/flatpak/$app_id.desktop" "$t/$name/share/applications/$app_id.desktop"
    [ -d "$out/root/usr/share/icons" ] || stage >/dev/null
    cp -r "$out/root/usr/share/icons" "$t/$name/share/icons"
    install -m644 LICENSE "$t/$name/LICENSE"
    cat > "$t/$name/README.txt" <<README
Stalemate $version, portable Linux build (x86_64).
Run ./stalemate. Keep config/ and i18n/ next to it.
For a menu entry, copy share/applications/$app_id.desktop to ~/.local/share/applications/
and share/icons/ into ~/.local/share/icons/, and set Exec= to the full path of stalemate.
Needs fontconfig, libxkbcommon, Wayland or X11, and OpenGL libraries from your distribution.
README
    tar -C "$t" --owner=0 --group=0 -czf "$out/$name.tar.gz" "$name"
    echo "Wrote $out/$name.tar.gz"
}

render_stub() { # <package format word> <version for the package metadata>
    PACKAGE_FORMAT=$1 stage >/dev/null
    rm -rf "$out/pkg"; mkdir -p "$out/pkg"
    sed "s/@VERSION@/$2/" packaging/linux/Cargo.toml.in > "$out/pkg/Cargo.toml"
    cp packaging/linux/stub.rs "$out/pkg/stub.rs"
    ln -s "$root/$out/root" "$out/pkg/root"
}

deb() {
    # Debian wants ~ for pre-releases: 0.1.0-alpha.1 -> 0.1.0~alpha.1
    local dv=${version/-/\~}
    render_stub deb "$version"
    (cd "$out/pkg" && cargo deb --no-build --no-strip --deb-version "$dv" -o "$root/$out/stalemate_${dv}_amd64.deb")
}

rpm() {
    # rpm versions cannot hold a hyphen; ~ sorts a pre-release before the release (alpha tags).
    local rv=${version/-/\~}
    render_stub rpm "$rv"
    # --auto-req disabled: the tool would copy the build host's library file names (Ubuntu's
    # libbz2.so.1.0, which Fedora does not provide). The requires list is in Cargo.toml.in.
    (cd "$out/pkg" && cargo generate-rpm --auto-req disabled -o "$root/$out/stalemate-$rv-1.x86_64.rpm" --target-dir "$root/$out/pkg-target" 2>&1)
}

appimage() {
    local ad=$out/Stalemate.AppDir
    rm -rf "$ad"; mkdir -p "$ad/usr/bin" "$ad/usr/share/stalemate"
    install -m755 "$bin" "$ad/usr/bin/stalemate"
    if command -v strip >/dev/null 2>&1; then strip "$ad/usr/bin/stalemate" || true; fi
    cp -r config i18n "$ad/usr/share/stalemate/"
    find "$ad" -name .gitkeep -delete
    echo appimage > "$ad/usr/share/stalemate/package_format"
    cat > "$ad/AppRun" <<'RUN'
#!/bin/sh
HERE=$(dirname "$(readlink -f "$0")")
STALEMATE_SHARE_DIR=${STALEMATE_SHARE_DIR:-$HERE/usr/share/stalemate}
export STALEMATE_SHARE_DIR
exec "$HERE/usr/bin/stalemate" "$@"
RUN
    chmod 755 "$ad/AppRun"
    install -m644 "packaging/flatpak/$app_id.desktop" "$ad/$app_id.desktop"
    install -Dm644 "packaging/flatpak/$app_id.desktop" "$ad/usr/share/applications/$app_id.desktop"
    local svg png
    svg=$(icon_file ast-icon-001-app.svg)
    png=$(icon_file ast-icon-002-app-256.png)
    if [ -n "$png" ]; then install -m644 "$png" "$ad/$app_id.png"; ln -s "$app_id.png" "$ad/.DirIcon"; fi
    [ -n "$svg" ] && install -m644 "$svg" "$ad/$app_id.svg"
    install -Dm644 "packaging/flatpak/$app_id.metainfo.xml" "$ad/usr/share/metainfo/$app_id.appdata.xml"
    install -Dm644 LICENSE "$ad/usr/share/doc/stalemate/LICENSE"
    local tool=${APPIMAGETOOL:-}
    if [ -z "$tool" ]; then tool=$(command -v appimagetool || true); fi
    [ -n "$tool" ] || { echo "package-linux: appimagetool not found. Set APPIMAGETOOL=/path/to/appimagetool-x86_64.AppImage (MIT, github.com/AppImage/appimagetool)." >&2; exit 1; }
    # --appimage-extract-and-run lets an AppImage tool run where FUSE is missing (containers, CI).
    local run=("$tool")
    case $tool in *.AppImage) run=("$tool" --appimage-extract-and-run) ;; esac
    # APPIMAGE_EXTRACT_AND_RUN also covers a tool installed without the .AppImage suffix (CI).
    APPIMAGE_EXTRACT_AND_RUN=1 ARCH=x86_64 "${run[@]}" "$ad" "$out/Stalemate-$version-x86_64.AppImage"
    echo "Wrote $out/Stalemate-$version-x86_64.AppImage"
}

aur_srcinfo() {
    (cd packaging/aur && makepkg --printsrcinfo > .SRCINFO)
    echo "Wrote packaging/aur/.SRCINFO"
}

cmd=${1:-}
[ $# -gt 0 ] && shift
case $cmd in
    stage) stage "$@" ;;
    tarball) tarball ;;
    deb) deb ;;
    rpm) rpm ;;
    appimage) appimage ;;
    aur-srcinfo) aur_srcinfo ;;
    *) sed -n '2,10p' "$0"; exit 2 ;;
esac
