# Stalemate task runner. See 15-tools-build-and-deploy.md.

default:
    @just --list

build:
    cargo build --workspace

test:
    cargo test --workspace
    cargo test -p stalemate-core --features fixtures
    cargo test -p stalemate-compliance --features fixtures
    @if command -v dotnet >/dev/null 2>&1 && { ls dotnet/*.sln dotnet/*.csproj dotnet/*/*.csproj >/dev/null 2>&1; }; then dotnet test dotnet/; else echo "dotnet tests skipped (no dotnet or no project in dotnet/)"; fi

lint:
    # Not --all: that would also format the vendored haste (third_party/haste), a path dependency.
    cargo fmt --check -p stalemate-core -p stalemate-app -p stalemate-overlay -p stalemate-model-trainer -p stalemate-fixture-recorder -p stalemate-compliance
    cargo clippy --workspace --all-targets -- -D warnings

export-assets:
    @echo "export-assets: not available yet (T-030)"; exit 1

flatpak:
    sh scripts/flatpak-sources.sh
    @if command -v flatpak-builder >/dev/null 2>&1; then FB=flatpak-builder; else FB="flatpak run org.flatpak.Builder"; fi; \
        $FB --force-clean --user --install --state-dir=target/flatpak/state target/flatpak/build packaging/flatpak/io.github.hatchbear.stalemate.yml

# Compliance checks: fair play (TST-030), hard-coded IDs (TST-017), assets (TST-110).
compliance:
    bash scripts/fair-play-check.sh
    sh scripts/no-hardcoded-ids.sh
    sh scripts/assets-check.sh

# Windows MSI and portable zip (T-028a); needs MSVC tools and WiX 5.0.2, see packaging/windows/README.md
win-installer:
    cargo build --release -p stalemate-app
    pwsh -NoProfile -File scripts/package-windows.ps1

# Native Linux .deb (T-004a); needs cargo-deb. Build in Ubuntu 22.04 for an old glibc, as linux-native.yml does
deb:
    cargo build --release -p stalemate-app
    bash scripts/package-linux.sh deb

# Native Linux .rpm (T-004a); needs cargo-generate-rpm
rpm:
    cargo build --release -p stalemate-app
    bash scripts/package-linux.sh rpm

# AppImage (T-004a); needs appimagetool or APPIMAGETOOL=/path/to/appimagetool-x86_64.AppImage
appimage:
    cargo build --release -p stalemate-app
    bash scripts/package-linux.sh appimage

# Portable tarball with config/ and i18n/ (T-004a)
tarball:
    cargo build --release -p stalemate-app
    bash scripts/package-linux.sh tarball

# Regenerate packaging/aur/.SRCINFO from the PKGBUILD; needs makepkg (Arch or a container)
aur-srcinfo:
    bash scripts/package-linux.sh aur-srcinfo

fixtures-verify:
    @echo "fixtures-verify: not available yet (needs fixtures from T-013)"; exit 1

# Fixture recorder (PRG-27, T-013). See crates/fixture-recorder/README.md.
# Run while the match is live. Raw files go to fixtures-raw/<id>/, which is git-ignored.
fixture-record MATCH_ID:
    cargo run --release -p stalemate-fixture-recorder -- record --match-id {{MATCH_ID}} --out fixtures-raw/{{MATCH_ID}}

# After the match: write the committable FX-BC and FX-API files. ME is your Account ID.
fixture-normalize MATCH_ID ME:
    cargo run --release -p stalemate-fixture-recorder -- normalize --raw fixtures-raw/{{MATCH_ID}} --match-id {{MATCH_ID}} --me {{ME}}
    cargo run --release -p stalemate-fixture-recorder -- api --raw fixtures-raw/{{MATCH_ID}} --match-id {{MATCH_ID}} --me {{ME}}
