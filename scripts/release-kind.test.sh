#!/usr/bin/env bash
# Tests scripts/release-kind.sh.
cd "$(dirname "$0")" || exit 1
fail=0
check() {
  got=$(bash release-kind.sh "$1")
  if [ "$got" != "$2" ]; then echo "FAIL $1: got $got, want $2"; fail=1; fi
}
check 26w45a alpha
check 26w45a2 alpha
check 26w45a13 alpha
check 26w45b invalid
check 2645a invalid
check 26w45 invalid
check 26w45a2x invalid
check x26w45a invalid
check v0.1.0-alpha.1 alpha
check v0.5.0 beta
check v1.0.0 stable
[ $fail -eq 0 ] && echo "release-kind tests passed"
exit $fail
