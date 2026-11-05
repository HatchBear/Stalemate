# 15 — Tools, Build and Deploy

## Purpose

Specifies the developer environment, build commands, Flatpak and Windows packaging, CI, versioning and release channels.

## Scope

- **Covers:** Everything from cloning the repository to publishing a release.
- **Elsewhere:** The test strategy is in [16](16-testing-and-qa.md).

## Details

### Developer environment

| Tool | Version | Purpose |
|---|---|---|
| Rust toolchain via `rustup` | Current stable at project start, pinned in `rust-toolchain.toml` [ASSUMPTION] | Build |
| `clippy`, `rustfmt` | With the toolchain | Lint, format |
| `just` | Latest | Task runner |
| .NET SDK | The version DemoFile targets [VERIFY] | PRG-21 |
| `flatpak`, `flatpak-builder` | Distribution packages | Flatpak builds |
| KDE runtime and SDK | `org.kde.Platform` / `org.kde.Sdk` 6.x [VERIFY current branch] | Flatpak base |
| Rust SDK extension | `org.freedesktop.Sdk.Extension.rust-stable`, branch matching the KDE SDK's freedesktop base [VERIFY] | Rust inside flatpak-builder |
| `flatpak-cargo-generator.py` | From flatpak-builder-tools | Offline cargo sources for Flathub |
| `resvg`, `oxipng` | Latest | Asset export ([13](13-art-and-asset-pipeline.md)) |
| `sqlite3` CLI | Any | Debugging |
| Steam and Deadlock | Current | Manual and fixture testing |
| A local LLM server, for example Ollama | Latest | Testing the Local provider |
| Windows 11 machine or VM with MSVC Build Tools | Current | Windows builds and tests |
| WiX Toolset | Version supported by the chosen cargo integration [VERIFY] | Windows installer |

**Test machines:**

- KDE Plasma 6 on Wayland (primary), with both native Steam and Flatpak Steam.
- Windows 11.
- Windows 10 for v1.

### `justfile` tasks

| Task | Command (summary) |
|---|---|
| `just build` | `cargo build --workspace` |
| `just test` | `cargo test --workspace` and `dotnet test dotnet/` |
| `just lint` | `cargo fmt --check`, then `cargo clippy --workspace -- -D warnings` |
| `just export-assets` | Asset pipeline ([13](13-art-and-asset-pipeline.md)) |
| `just flatpak` | Generate cargo sources, then `flatpak-builder --force-clean --user --install build-dir packaging/flatpak/io.github.hatchbear.stalemate.yml` |
| `just win-installer` | `cargo build --release`, publish PRG-21, build the WiX MSI |
| `just fixtures-verify` | Run TC-04-1 and TC-04-5 against every FX-BC and FX-DEM fixture |

### Flatpak manifest

Path: `packaging/flatpak/io.github.hatchbear.stalemate.yml`. Versions marked VERIFY must be checked at setup.

```yaml
app-id: io.github.hatchbear.stalemate                      # CD-01, reverse-DNS
runtime: org.kde.Platform
runtime-version: '6.x'                # VERIFY current branch
sdk: org.kde.Sdk
sdk-extensions:
  - org.freedesktop.Sdk.Extension.rust-stable   # VERIFY branch compatibility
command: stalemate
finish-args:
  - --share=network                   # deadlock-api, Broadcast, LLM providers
  - --share=ipc
  - --socket=wayland
  - --socket=fallback-x11
  - --device=dri
  - --filesystem=~/.local/share/Steam:ro                    # native Steam
  - --filesystem=~/.steam:ro
  - --filesystem=~/.var/app/com.valvesoftware.Steam:ro      # Flatpak Steam
  # Steam libraries elsewhere: the user grants access through the FileChooser portal
build-options:
  append-path: /usr/lib/sdk/rust-stable/bin
  env:
    CARGO_HOME: /run/build/stalemate/cargo
modules:
  - name: stalemate
    buildsystem: simple
    build-commands:
      - cargo --offline fetch --manifest-path Cargo.toml
      - cargo --offline build --release --workspace
      - install -Dm755 target/release/stalemate /app/bin/stalemate
      - install -Dm755 target/release/stalemate-overlay /app/bin/stalemate-overlay
      - install -Dm644 assets/icon/ast-icon-001-app.svg /app/share/icons/hicolor/scalable/apps/io.github.hatchbear.stalemate.svg
      - install -Dm644 packaging/flatpak/io.github.hatchbear.stalemate.desktop /app/share/applications/io.github.hatchbear.stalemate.desktop
      - install -Dm644 packaging/flatpak/io.github.hatchbear.stalemate.metainfo.xml /app/share/metainfo/io.github.hatchbear.stalemate.metainfo.xml
      - cp -r config models prompts i18n /app/share/stalemate/
    sources:
      - type: dir
        path: ../..
      - cargo-sources.json            # from flatpak-cargo-generator.py
```

**PRG-21 in the Flatpak:** Bundle a self-contained, trimmed .NET publish of the fallback parser in Beta, on both platforms (DEC-024). Measure the size impact at T-004 and revisit the decision then if the Flatpak is too large.

**Flathub review:** The read-only Steam paths are justified in the submission as needed to read `console.log` and Steam config (RSK-15).

### Windows packaging

- **Targets:** `x86_64-pc-windows-msvc`. Binaries `stalemate.exe`, `stalemate-overlay.exe` and `stalemate-parser-fallback.exe` (self-contained .NET).
- **Installer:** WiX MSI, per-user install to `%LOCALAPPDATA%\Programs\stalemate\` [ASSUMPTION]. Uses AST-ICON-003, AST-STORE-002 and AST-STORE-003.
- **Code signing:** Apply to a free open-source signing programme, which needs the open GPL-3.0 licence (DEC-020, DEC-033). Until approved, ship unsigned with a README note about SmartScreen warnings.

### CI (GitHub Actions) [ASSUMPTION host]

| Workflow | Trigger | Jobs |
|---|---|---|
| `ci.yml` | Every push and PR | `lint` (Linux); `test-linux`; `test-windows`; `assets-check` (TST-110) |
| `flatpak.yml` | Push to `main`, tags | Build the Flatpak bundle in the KDE SDK container; upload as an artifact |
| `windows.yml` | Push to `main`, tags | Build the MSI; upload as an artifact |
| `release.yml` | Tag `v*` | Create a GitHub Release with the `.flatpak` bundle and the MSI; for beta and stable, open a PR to the Flathub repository |

### Versioning and channels

| Channel | Versions [ASSUMPTION] | Linux | Windows |
|---|---|---|---|
| Alpha | `0.1.x`–`0.4.x` | `.flatpak` bundle on GitHub Releases | — |
| Beta | `0.5.x`–`0.9.x` | Flathub beta branch | MSI on GitHub Releases (prerelease) |
| Stable | `1.0.0`+ | Flathub stable | MSI on GitHub Releases |

Versions follow SemVer. Every release lists the game patch it was tested against.

### Release checklist

1. All Must requirements for the phase pass ([16](16-testing-and-qa.md)).
2. `just fixtures-verify` passes on the newest patch fixture.
3. A DATA-004 model exists and its Brier score on recent matches is still ≤ 0.20, or the "Model from earlier patches" label works.
4. No placeholder assets remain (v1 only).
5. The privacy policy URL is live (v1 only).
6. Release notes include the tested game patch.
7. The repository carries the GPL-3.0 licence text (DEC-020).

## Dependencies

- [10 Architecture](10-technical-architecture.md)
- [13 Assets](13-art-and-asset-pipeline.md)
- [14 Data assets](14-data-and-content-assets.md)
- [16 Testing](16-testing-and-qa.md)
- [17 Roadmap](17-roadmap.md)

## Open Questions

- OQ-14 (decided): Free open-source signing programme, and unsigned with a README note until approved (DEC-033).
- What are the current `org.kde.Platform` branch and the matching rust-stable extension branch? [VERIFY]
- [ASSUMPTION] GitHub as host, version ranges per channel, per-user MSI install, bundling PRG-21 in the Flatpak.

## Acceptance Criteria

- [ ] A fresh clone builds and passes `just lint test` on Linux and Windows.
- [ ] `just flatpak` produces an installable Flatpak that launches on KDE Plasma 6 Wayland.
- [ ] `just win-installer` produces an MSI that installs and uninstalls cleanly on Windows 11.
- [ ] A `v*` tag produces a GitHub Release with both packages.
