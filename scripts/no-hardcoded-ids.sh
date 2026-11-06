#!/usr/bin/env sh
# TST-017 (FR-DATA-008, NFR-MAINT-001): flag hard-coded hero or item ID lists.
#
# A heuristic, not a proof. It scans crates/*/src/**/*.rs, skipping test files
# (tests/ dirs, tests.rs, *_test.rs) and #[cfg(test)] items, and flags:
#   - an array literal with 5 or more integer literals
#   - an array literal with 3 or more (id, "name") tuples
#   - 3 or more `123 => "name"` match arms close together
#   - a const, static or let named like hero/item/upgrade/ability that holds
#     an array of 2 or more literals
# A deliberate exception can carry a  no-hardcoded-ids-allow: <reason>  comment on
# the same line or the line above.
#
# Limits:
#   - Single literals (`hero_id == 15`) and short lists are not caught.
#   - IDs built at runtime, or kept in other file types (.slint, .json embedded with
#     include_str!, SQL) are not scanned. Config under config/ is where such data belongs.
#   - Integer arrays that are not IDs (colours, byte tables) can trigger it; mark them
#     with the allow comment.
#   - Names are not checked against real hero or item names, because the check must not
#     contain such a list itself.
#   - Test code is skipped by file name and by #[cfg(test)], so a test helper in a
#     normal file outside a cfg(test) item is scanned.
# The runtime half of TST-017 (start against mock assets, assets cached per patch) is
# a unit test, not part of this script.
set -eu
root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"
exec python3 -I "$root/scripts/no_hardcoded_ids.py" crates
