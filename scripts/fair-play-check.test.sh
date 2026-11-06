#!/usr/bin/env bash
# Tests scripts/fair-play-check.sh symbol exceptions and DEC-079 source guards.
# Runs the real script against temporary trees via FAIR_PLAY_ROOT; the real source is untouched.
here=$(cd "$(dirname "$0")" && pwd)
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
fail=0

newroot() { # $1 = name
    r="$tmp/$1"
    mkdir -p "$r/scripts" "$r/crates/x" "$r/target/release"
    cp "$here/fair-play-patterns.txt" "$here/fair-play-allow.txt" "$r/scripts/"
    : > "$r/Cargo.lock"
    echo "$r"
}
run() { FAIR_PLAY_ROOT=$1 CARGO_TARGET_DIR=$1/target bash "$here/fair-play-check.sh" 2>&1; }
expect() { # name want(0|1) root
    out=$(run "$3"); rc=$?
    [ "$rc" -ne 0 ] && rc=1
    if [ "$rc" -ne "$2" ]; then echo "FAIL $1: exit $rc, want $2"; echo "$out"; fail=1; fi
    LAST=$out
}

r=$(newroot a); printf 'LoadLibraryExW\0SendInput\0' > "$r/target/release/stalemate.exe"
expect "stalemate.exe LoadLibraryExW+SendInput passes" 0 "$r"

r=$(newroot b); printf 'LoadLibraryExW\0SendInput\0' > "$r/target/release/stalemate-overlay.exe"
expect "overlay SendInput fails" 1 "$r"
case $LAST in *"stalemate-overlay.exe imports SendInput"*) ;; *) echo "FAIL b: SendInput not reported"; fail=1 ;; esac
case $LAST in *"imports LoadLibraryExW"*) echo "FAIL b: LoadLibraryExW should be allowed"; fail=1 ;; esac

r=$(newroot c); printf 'CreateRemoteThread\0' > "$r/target/release/stalemate.exe"
expect "CreateRemoteThread fails" 1 "$r"

r=$(newroot d); printf 'LoadLibraryW\0' > "$r/target/release/stalemate"
expect "ELF named stalemate gets no LoadLibrary exemption (no nm output, so passes)" 0 "$r"

r=$(newroot e); printf 'fn f() { window.bring_to_front(); }\n' > "$r/crates/x/a.rs"
expect "bring_to_front fails" 1 "$r"

for snippet in 'component A { MenuBar { } }' 'ContextMenuArea { }' 'Menu {' 'MenuItem {' 'full-screen: true;'; do
    r=$(newroot f); printf '%s\n' "$snippet" > "$r/crates/x/a.slint"
    expect "slint '$snippet' fails" 1 "$r"
done
for snippet in 'w.set_fullscreen(true);' 'w.focus_window();'; do
    r=$(newroot g); printf '%s\n' "$snippet" > "$r/crates/x/a.rs"
    expect "rust '$snippet' fails" 1 "$r"
done

r=$(newroot h); printf '// bring_to_front is banned\nlet menu_bar = 1; MenuBarSize\n' > "$r/crates/x/a.rs"
expect "comments and unrelated identifiers pass" 0 "$r"

[ $fail -eq 0 ] && echo "fair-play-check tests passed"
exit $fail
