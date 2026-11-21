#!/usr/bin/env bash
# Runs the canto test suite (the GTK tests stay ignored: gtk-tests.sh runs them) in the
# same isolated session as gtk-tests.sh, since some tests read settings or start GTK.
# One test thread, so a crash (SIGSEGV) points at the last "test ..." line.
#
# Usage: scripts/dev/tests.sh [cargo test filter]
# The full output goes to $CANTO_OUT/tests.log (default $TMPDIR/canto-dev).
# Exit code: the test run's own; 1 also when no test ran. Settings: see scripts/dev/lib.sh.
set -uo pipefail

# shellcheck source=scripts/dev/lib.sh
. "$(dirname "${BASH_SOURCE[0]}")/lib.sh"
canto_load_env
cd "$CANTO_REPO" || exit 1

fail() {
    echo "TESTS-FAIL: $*"
    exit 1
}

out="${CANTO_OUT:-${TMPDIR:-/tmp}/canto-dev}"
mkdir -p "$out"
log="$out/tests.log"
rm -f "$log"

gres="$(canto_gresource)" || fail "$gres"
canto_lock "tests.sh${1:+ $1}"
canto_session_start "$gres" || fail "could not start the test session"

mapfile -t jobs < <(canto_jobs_args)
canto_heavy env "${CANTO_SESSION_ENV[@]}" dbus-run-session -- \
    "$CARGO" test "${CANTO_PACKAGES[@]}" "${jobs[@]}" --color never "$@" -- --test-threads=1 2>&1 \
    | tee "$log" \
    | grep -E "test result|FAILED|panicked|^error|SIGSEGV"
rc=${PIPESTATUS[0]}

if [[ $rc -ne 0 ]]; then
    echo "TESTS-FAIL: cargo test exited with $rc (full output: $log)"
    exit "$rc"
fi
grep -Eq '^test result: ok\. [1-9][0-9]* passed' "$log" \
    || fail "no test ran (full output: $log)"
echo "TESTS-OK"
