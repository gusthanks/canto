#!/usr/bin/env bash
# Debug compile of the canto tests, the same package selection every time, so the
# next step reuses what this one built. Shows every warning and fails on any warning.
#
# Usage: scripts/dev/check.sh
# The full cargo output goes to $CANTO_OUT/check.log (default $TMPDIR/canto-dev).
# Exit code: cargo's own when it fails, 1 when it passes with warnings, 0 when clean.
# Settings: see scripts/dev/lib.sh.
set -uo pipefail

# shellcheck source=scripts/dev/lib.sh
. "$(dirname "${BASH_SOURCE[0]}")/lib.sh"
canto_load_env
cd "$CANTO_REPO" || exit 1

out="${CANTO_OUT:-${TMPDIR:-/tmp}/canto-dev}"
mkdir -p "$out"
log="$out/check.log"
rm -f "$log"

canto_lock "check.sh"

mapfile -t jobs < <(canto_jobs_args)
canto_heavy "$CARGO" test "${CANTO_PACKAGES[@]}" "${jobs[@]}" --no-run --color never > "$log" 2>&1
rc=$?

# Cargo replays the warnings of units it did not rebuild, so a clean log means clean code
warnings=$(grep -c '^warning' "$log")
if [[ $warnings -gt 0 ]]; then
    grep -E -A14 '^warning' "$log" | grep -v '^--$' | head -200
fi
if [[ $rc -ne 0 ]]; then
    grep -E -A14 '^error' "$log" | head -120
    echo "CHECK-FAIL: cargo exited with $rc (full output: $log)"
    exit "$rc"
fi
grep -E '^ *Finished' "$log"
if [[ $warnings -gt 0 ]]; then
    echo "CHECK-FAIL: $warnings warning line(s) (full output: $log)"
    exit 1
fi
echo "CHECK-OK: no warnings"
