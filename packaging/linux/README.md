# Native Linux packages (Alpha)

Built by `scripts/package-linux.sh` and `.github/workflows/linux-native.yml` (T-004a, DEC-064). x86_64 only, main window only (the overlay is Beta). The binary is built in Ubuntu 22.04 so it runs on glibc 2.35 and newer.

| Package | Recipe | Tool (licence) |
|---|---|---|
| `.deb` | `just deb` | cargo-deb (MIT) |
| `.rpm` | `just rpm` | cargo-generate-rpm (MIT) |
| AppImage | `just appimage` | appimagetool (MIT) |
| Tarball | `just tarball` | tar |
| AUR | `packaging/aur/PKGBUILD`, `just aur-srcinfo` | makepkg |

`Cargo.toml.in` holds the `.deb` and `.rpm` metadata. It is rendered into `target/package/linux/pkg/` so the tools do not need metadata in `crates/app/Cargo.toml`. Keep its dependency lists in step with `packaging/aur/PKGBUILD` and `scripts/smoke-linux.sh`.

## Where the app looks for its files

The app reads `STALEMATE_SHARE_DIR` first, then `/app/share/stalemate` (Flatpak), then the folder next to the executable (`crates/core/src/settings.rs`). It does not search `/usr/share/stalemate`, so:

- `.deb`, `.rpm`, AUR: `/usr/bin/stalemate` is a small wrapper that sets `STALEMATE_SHARE_DIR=/usr/share/stalemate` and runs `/usr/lib/stalemate/stalemate`.
- AppImage: `AppRun` sets `STALEMATE_SHARE_DIR` to the mounted `usr/share/stalemate`.
- Tarball: `config/` and `i18n/` sit next to the binary.

The AppImage and tarball do not bundle libraries. They need fontconfig, libxkbcommon, Wayland or X11 and OpenGL from the host, as on any desktop.

## Publishing the AUR package

The release workflow fills in `pkgver` and the checksum (`scripts/aur-update.sh`) and uploads `PKGBUILD` and `.SRCINFO` as the `aur-package` artifact. Nothing is pushed to the AUR automatically. The checked-in `PKGBUILD` has `sha256sums=('SKIP')` and must not be published as it is.
