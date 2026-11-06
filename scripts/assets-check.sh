#!/usr/bin/env sh
# TST-110: asset check (doc 13).
#
# Checks:
#   1. Every Alpha-priority asset exists as a final file or as a .placeholder file in
#      assets/placeholders/: AST-ICON-001, AST-ICON-002 (8 sizes), AST-UI-005 (5), AST-UI-008 (3).
#   2. SVG rules for every SVG under assets/ (except assets/source/): no <image>, no external
#      href, no <text>. Placeholder SVGs are exempt from the <text> rule because doc 13 requires
#      them to show the asset ID as text (conflict inside doc 13); each exemption is counted as
#      a warning.
#   3. PNG files are 8-bit RGBA, not interlaced, and match the size in their file name.
#   4. No Valve files: no *.vpk, *.dem or *.bcast outside private-reference/, and nothing
#      tracked by git under private-reference/ except .gitkeep.
#
# Placeholders are a warning, not a failure. Pass --release (or set ASSETS_RELEASE=1) to make
# any remaining placeholder a failure (v1 release builds).
set -u
root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root" || exit 2

release=${ASSETS_RELEASE:-0}
[ "${1:-}" = "--release" ] && release=1

fail=0
warn_placeholder=0
warn_text=0
bad() { printf 'FAIL %s\n' "$1"; fail=$((fail + 1)); }

# --- 1. Alpha assets ---------------------------------------------------------
# <dir of final file> <name stem> <ext>
check_asset() {
    dir=$1 stem=$2 ext=$3
    if [ -f "assets/$dir/$stem.$ext" ]; then
        return 0
    elif [ -f "assets/placeholders/$stem.placeholder.$ext" ]; then
        warn_placeholder=$((warn_placeholder + 1))
        return 0
    fi
    bad "missing asset: assets/$dir/$stem.$ext (or assets/placeholders/$stem.placeholder.$ext)"
}

check_asset icon ast-icon-001-app svg
for s in 16 24 32 48 64 128 256 512; do check_asset icon "ast-icon-002-app-$s" png; done
for n in lane-troopers neutral-camps breakables urn sinners-sacrifice; do
    check_asset ui "ast-ui-005-soul-$n" svg
done
for n in pass fail pending; do check_asset ui "ast-ui-008-setup-$n" svg; done

# --- 2. SVG rules ------------------------------------------------------------
svgs=$(find assets -name '*.svg' -not -path 'assets/source/*' 2>/dev/null | sort)
for f in $svgs; do
    grep -qiE '<image([[:space:]>/])' "$f" && bad "$f: contains <image> (no embedded raster)"
    # any href that does not point inside the file
    if grep -iE 'href[[:space:]]*=[[:space:]]*["'"'"']' "$f" | grep -viE 'href[[:space:]]*=[[:space:]]*["'"'"']#' >/dev/null; then
        bad "$f: external href"
    fi
    grep -qiE 'url\([[:space:]]*["'"'"']?(https?:|file:|//)' "$f" && bad "$f: external url() reference"
    if grep -qiE '<text([[:space:]>/])' "$f"; then
        case $f in
        *.placeholder.svg) warn_text=$((warn_text + 1)) ;;
        *) bad "$f: contains <text> (convert text to paths)" ;;
        esac
    fi
done

# --- 3. PNG rules ------------------------------------------------------------
u32() { od -An -tu1 -j "$2" -N4 "$1" | awk '{ print $1 * 16777216 + $2 * 65536 + $3 * 256 + $4 }'; }
u8() { od -An -tu1 -j "$2" -N1 "$1" | tr -d ' '; }
pngs=$(find assets -name '*.png' -not -path 'assets/source/*' 2>/dev/null | sort)
for f in $pngs; do
    sig=$(od -An -tx1 -N8 "$f" | tr -d ' \n')
    if [ "$sig" != "89504e470d0a1a0a" ]; then bad "$f: not a PNG"; continue; fi
    w=$(u32 "$f" 16) h=$(u32 "$f" 20) depth=$(u8 "$f" 24) ctype=$(u8 "$f" 25) interlace=$(u8 "$f" 28)
    [ "$depth" = 8 ] && [ "$ctype" = 6 ] || bad "$f: must be 8-bit RGBA (bit depth $depth, colour type $ctype)"
    [ "$interlace" = 0 ] || bad "$f: must not be interlaced"
    [ "$w" = "$h" ] || bad "$f: not square (${w}x$h)"
    size=$(basename "$f" | sed -n 's/^.*-\([0-9][0-9]*\)\(\.placeholder\)\{0,1\}\.png$/\1/p')
    if [ -n "$size" ] && [ "$w" != "$size" ]; then bad "$f: file name says $size px, image is ${w}x$h"; fi
done

# --- 4. Valve files ------------------------------------------------------------
valve=$(find . \( -path ./target -o -path ./.git -o -path ./private-reference -o -path '*/node_modules' \) -prune -o \
    -type f \( -name '*.vpk' -o -name '*.dem' -o -name '*.bcast' \) -print 2>/dev/null)
for f in $valve; do bad "Valve file in tree: $f"; done
if command -v git >/dev/null 2>&1 && git rev-parse --git-dir >/dev/null 2>&1; then
    for f in $(git ls-files -- private-reference | grep -v '^private-reference/\.gitkeep$'); do
        bad "tracked file under private-reference/: $f"
    done
    for f in $(git ls-files -- '*.vpk' '*.dem' '*.bcast'); do bad "tracked Valve file: $f"; done
fi

# --- result --------------------------------------------------------------------
svg_n=$(printf '%s\n' "$svgs" | grep -c .)
png_n=$(printf '%s\n' "$pngs" | grep -c .)
echo "assets-check: $svg_n SVG and $png_n PNG files checked"
[ "$warn_placeholder" -gt 0 ] && echo "WARN: $warn_placeholder Alpha asset(s) are still placeholders (allowed until v1)"
[ "$warn_text" -gt 0 ] && echo "WARN: $warn_text placeholder SVG(s) contain <text>, exempt from the no-text rule (doc 13 conflict: placeholders must show the asset ID)"
if [ "$release" = 1 ] && [ "$warn_placeholder" -gt 0 ]; then
    bad "release mode: $warn_placeholder placeholder asset(s) remain"
fi
if [ "$fail" -gt 0 ]; then
    echo "assets-check: FAILED ($fail finding(s))"
    exit 1
fi
echo "assets-check: passed"
