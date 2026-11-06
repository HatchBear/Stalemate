#!/usr/bin/env bash
# Release gate (DEC-049). Fails when the release build includes the `broadcast`
# feature (vendored haste, which depends on mars) and mars still has no licence.
# To pass today, release builds must drop the feature: --no-default-features. That needs
# stalemate-app to depend on stalemate-core with default-features = false and pass a
# `broadcast` feature through (crates/app is not owned by platform-qa; see the T-008 report).
# Run it with the same feature flags the release will use:
#   RELEASE_CARGO_FLAGS="--no-default-features" bash scripts/release-check.sh
#
# Closed-Alpha mode (DEC-071): `--closed-alpha` or STALEMATE_CLOSED_ALPHA=1. A missing mars
# licence then prints a warning and exits 0, so invited testers can get Alpha builds before the
# licence is published (blukai agreed to license it, DEC-069). Public releases stay blocked.
set -euo pipefail
cd "$(dirname "$0")/.."

closed_alpha=${STALEMATE_CLOSED_ALPHA:-0}
for a in "$@"; do
    case $a in
        --closed-alpha) closed_alpha=1 ;;
        *) echo "release-check: unknown argument $a" >&2; exit 2 ;;
    esac
done

flags=${RELEASE_CARGO_FLAGS:-}

# Fail closed: no cargo, or a failing cargo tree, is an error, never a pass.
if ! command -v cargo >/dev/null 2>&1; then
    echo "release-check: FAIL. cargo is not on PATH, so the dependency tree can't be checked." >&2
    exit 2
fi

# Is mars in the normal and build dependency tree of the shipped app?
tree_err=$(mktemp)
trap 'rm -f "$tree_err"' EXIT
# shellcheck disable=SC2086
if ! tree=$(cargo tree -p stalemate-app $flags -e normal,build --prefix none --format '{p}' --locked 2>"$tree_err"); then
    echo "release-check: FAIL. 'cargo tree' exited with an error, so the build can't be checked:" >&2
    cat "$tree_err" >&2
    exit 2
fi
if ! printf '%s\n' "$tree" | grep -q '^mars '; then
    echo "release-check: mars is not in the build ($flags). OK."
    exit 0
fi

# mars is in the build. Does it declare a licence?
licence=$(cargo metadata --format-version 1 --locked 2>/dev/null | python3 -I -c '
import json, sys
m = json.load(sys.stdin)
for p in m["packages"]:
    if p["name"] == "mars":
        print(p.get("license") or p.get("license_file") or "")
        break
')

if [ -n "$licence" ]; then
    echo "release-check: mars declares licence '$licence'. OK."
    exit 0
fi

if [ "$closed_alpha" = 1 ]; then
    echo "release-check: WARNING (DEC-071, closed Alpha only). mars has no licence yet, but this is a" >&2
    echo "closed Alpha build for invited testers. Do not publish it: no open release, Flathub or AUR." >&2
    exit 0
fi

echo "release-check: FAIL. The release build includes the 'broadcast' feature (haste and mars)," >&2
echo "and mars has no licence. Build the release with --no-default-features until mars is" >&2
echo "licensed (DEC-049). Closed Alpha builds can use --closed-alpha (DEC-071)." >&2
exit 1
