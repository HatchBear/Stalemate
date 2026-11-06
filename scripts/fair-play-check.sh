#!/usr/bin/env bash
# TST-030 source, lockfile and symbol check; also TST-033 (no Steam Web API) at source level.
#
# Fails if the source tree, Cargo.lock or the built binaries contain anything on the
# forbidden list in scripts/fair-play-patterns.txt (memory access, injection, graphics
# hooks, packet capture, input, Steam Web API, game-coordinator clients, forced
# refresh against deadlock-api).
#
# Usage: scripts/fair-play-check.sh [--require-binaries]
#   --require-binaries  fail if no built binaries are found (CI uses this)
#
# Binaries are looked up in $CARGO_TARGET_DIR (default: target) under debug/ and release/.
# This is a static check of what we write and link. The runtime half of TST-033 (a
# network capture during a match) is a manual test and is not covered here.
set -u

root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root" || exit 2
patterns=scripts/fair-play-patterns.txt
require_bins=0
[ "${1:-}" = "--require-binaries" ] && require_bins=1

fail=0
report() { printf 'FAIL [%s] %s\n' "$1" "$2"; fail=$((fail + 1)); }

# Drop hits that match a reviewed exception in scripts/fair-play-allow.txt.
# Reads grep -n output (file:line:text) on stdin.
allowed() {
    cat_=$1
    ex=$(grep -Ev '^[[:space:]]*(#|$)' scripts/fair-play-allow.txt | awk -v c="$cat_" '$1 == c')
    if [ -z "$ex" ]; then cat; return; fi
    while IFS= read -r hit; do
        file=${hit%%:*}
        keep=1
        while read -r _ afile are; do
            if [ "$afile" = "$file" ] && printf '%s\n' "$hit" | grep -Eq "$are"; then keep=0; break; fi
        done <<EOF_EX
$ex
EOF_EX
        [ "$keep" -eq 1 ] && printf '%s\n' "$hit"
    done
    return 0
}

rules() { grep -Ev '^[[:space:]]*(#|$)' "$patterns" | awk -v k="$1" '$1 == k'; }

# ---- 1. source scan -------------------------------------------------------
echo "== source scan =="
scan_paths=""
for p in crates ui dotnet packaging config tests Cargo.toml justfile; do
    [ -e "$p" ] && scan_paths="$scan_paths $p"
done
src_hits=0
while read -r _ cat re; do
    # shellcheck disable=SC2086
    out=$(grep -rEnI --exclude-dir=target --exclude-dir=bin --exclude-dir=obj \
        --exclude-dir=fixtures --exclude='*.md' --exclude='*.lock' --exclude='cargo-sources.json' \
        -e "$re" $scan_paths 2>/dev/null | grep -v 'fair-play-allow' |
        grep -Ev '^[^:]*:[0-9]+:[[:space:]]*(//|#)' | allowed "$cat")
    if [ -n "$out" ]; then
        while IFS= read -r line; do report "$cat" "$line  (pattern: $re)"; done <<EOF_HITS
$out
EOF_HITS
        src_hits=$((src_hits + 1))
    fi
done < <(rules src)
[ "$src_hits" -eq 0 ] && echo "ok: no forbidden source patterns"

# ---- 2. Cargo.lock ------------------------------------------------------
echo "== Cargo.lock =="
lock_hits=0
if [ -f Cargo.lock ]; then
    names=$(sed -n 's/^name = "\(.*\)"$/\1/p' Cargo.lock | sort -u)
    while read -r _ cat re; do
        m=$(printf '%s\n' "$names" | grep -E "$re")
        if [ -n "$m" ]; then
            for c in $m; do report "$cat" "Cargo.lock contains crate '$c' (pattern: $re)"; done
            lock_hits=$((lock_hits + 1))
        fi
    done < <(rules lock)
    [ "$lock_hits" -eq 0 ] && echo "ok: no forbidden crates ($(printf '%s\n' "$names" | wc -l | tr -d ' ') crates checked)"
else
    report lock "Cargo.lock not found"
fi

# ---- 3. built binaries ----------------------------------------------------
echo "== binaries =="
tdir=${CARGO_TARGET_DIR:-target}
case $tdir in /*) ;; *) tdir="$root/$tdir" ;; esac
bins=""
for prof in debug release; do
    for b in stalemate stalemate-overlay stalemate-fixture-recorder stalemate-model-trainer; do
        for f in "$tdir/$prof/$b" "$tdir/$prof/$b.exe"; do
            [ -f "$f" ] && bins="$bins$f
"
        done
    done
done
if [ -z "$bins" ]; then
    if [ "$require_bins" -eq 1 ]; then
        report binaries "no built binaries found under $tdir (build first: cargo build --workspace)"
    else
        echo "WARN: no built binaries under $tdir; symbol check skipped (run cargo build --workspace first)"
    fi
fi
sym_re=$(rules sym | awk '{ $1 = ""; $2 = ""; sub(/^  /, ""); print }' | paste -sd'|' -)
while IFS= read -r f; do
    [ -n "$f" ] || continue
    echo "checking ${f#"$root"/}"
    case $f in
    *.exe)
        # PE: imported function names are plain ASCII in the import table.
        syms=$(grep -aoE '[A-Za-z_][A-Za-z0-9_]{3,}' "$f" | sort -u)
        ;;
    *)
        if command -v nm >/dev/null 2>&1; then
            syms=$(nm -D --undefined-only "$f" 2>/dev/null | awk '{ print $NF }' | sed 's/@.*//' | sort -u)
        else
            echo "WARN: nm not found; skipping symbol check for $f"
            syms=""
        fi
        if command -v objdump >/dev/null 2>&1; then
            libs=$(objdump -p "$f" 2>/dev/null | awk '$1 == "NEEDED" { print $2 }')
            while read -r _ cat re; do
                for l in $(printf '%s\n' "$libs" | grep -E "$re"); do
                    report "$cat" "${f#"$root"/} links library $l"
                done
            done < <(rules lib)
        fi
        ;;
    esac
    if [ -n "$syms" ] && [ -n "$sym_re" ]; then
        for s in $(printf '%s\n' "$syms" | grep -E "$sym_re"); do
            report symbol "${f#"$root"/} imports $s"
        done
    fi
done < <(printf '%s' "$bins")

echo
if [ "$fail" -gt 0 ]; then
    echo "fair-play-check: FAILED ($fail finding(s))"
    exit 1
fi
echo "fair-play-check: passed"
