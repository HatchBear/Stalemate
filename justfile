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

win-installer:
    @echo "win-installer: not available yet (T-028)"; exit 1

fixtures-verify:
    @echo "fixtures-verify: not available yet (needs fixtures from T-013)"; exit 1
