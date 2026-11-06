#!/usr/bin/env sh
# Generate packaging/flatpak/cargo-sources.json from Cargo.lock (doc 15).
# Downloads flatpak-cargo-generator.py (flatpak-builder-tools, MIT) at a pinned
# commit into target/flatpak/ and runs it in a venv there. Nothing is written
# to the repo root, and a clean clone needs only python3, curl and network.
set -eu

COMMIT=41c20aa10819cdb2a4f3ca171758a96d1955c018
SHA256=0a2db6be87d75910facef28ab46d4d6460802e8419ab850d0caa6a364d26b380
URL="https://raw.githubusercontent.com/flatpak/flatpak-builder-tools/$COMMIT/cargo/flatpak-cargo-generator.py"

root=$(cd "$(dirname "$0")/.." && pwd)
work="$root/target/flatpak"
mkdir -p "$work"

gen="$work/flatpak-cargo-generator.py"
if [ ! -f "$gen" ] || ! echo "$SHA256  $gen" | sha256sum -c - >/dev/null 2>&1; then
    curl -fsSL "$URL" -o "$gen"
    echo "$SHA256  $gen" | sha256sum -c -
fi

if [ ! -x "$work/venv/bin/python" ]; then
    python3 -m venv "$work/venv"
    "$work/venv/bin/pip" install --quiet "aiohttp>=3.9.5,<4" "PyYAML>=6.0.2,<7" "tomlkit>=0.13.3,<1.0"
fi

"$work/venv/bin/python" "$gen" "$root/Cargo.lock" -o "$root/packaging/flatpak/cargo-sources.json"
