#!/usr/bin/env bash
# Release gate (DEC-049). Fails when the release build includes the `broadcast`
# feature (vendored haste, which depends on mars) and mars still has no licence.
# To pass today, release builds must drop the feature: --no-default-features. That needs
# stalemate-app to depend on stalemate-core with default-features = false and pass a
# `broadcast` feature through (crates/app is not owned by platform-qa; see the T-008 report).
# Run it with the same feature flags the release will use:
#   RELEASE_CARGO_FLAGS="--no-default-features" bash scripts/release-check.sh
set -euo pipefail
cd "$(dirname "$0")/.."

flags=${RELEASE_CARGO_FLAGS:-}

# Is mars in the normal and build dependency tree of the shipped app?
# shellcheck disable=SC2086
if ! cargo tree -p stalemate-app $flags -e normal,build --prefix none --format '{p}' --locked 2>/dev/null | grep -q '^mars '; then
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

echo "release-check: FAIL. The release build includes the 'broadcast' feature (haste and mars)," >&2
echo "and mars has no licence. Build the release with --no-default-features until mars is" >&2
echo "licensed (DEC-049)." >&2
exit 1
