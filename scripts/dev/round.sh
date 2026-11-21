#!/usr/bin/env bash
# One development round, one heavy step at a time, stopping at the first failure:
#   check     debug test compile, no warnings allowed       (scripts/dev/check.sh)
#   gresource the UI resources, then the Blueprint lint
#   gtk       the GTK tests on Broadway, with PNG renders   (scripts/dev/gtk-tests.sh)
#   tests     the rest of the canto suite                   (scripts/dev/tests.sh)
#   gates     the five CI quality gates                     (scripts/checks/check-quality.sh)
# No release build. The last line is the verdict: ROUND-OK, or <STEP>-FAIL.
#
# Usage: [SKIP="check gresource"] scripts/dev/round.sh
#   SKIP  steps not to run; the verdict then says "ROUND-OK (skipped: ...)".
# Each round writes to a new folder, $CANTO_ROUNDS/<date-time> (default
# $TMPDIR/canto-rounds): round.log, each step's log and the renders in dump/.
# Each step's seconds, with CPU MHz, power source, load and temperature, are appended
# to $CANTO_DEV_STATE/rounds.tsv. The whole round holds the dev lock, so two rounds
# started together run one after the other. Settings: see scripts/dev/lib.sh.
set -uo pipefail

# shellcheck source=scripts/dev/lib.sh
. "$(dirname "${BASH_SOURCE[0]}")/lib.sh"
canto_load_env
cd "$CANTO_REPO" || exit 1
dev="$CANTO_REPO/scripts/dev"

if [[ $# -gt 0 ]]; then
    awk 'NR > 1 && /^#/ { print; next } NR > 1 { exit }' "$0"
    [[ "$1" == -h || "$1" == --help ]] && exit 0
    echo "round.sh takes no arguments (use SKIP=...)" >&2
    exit 2
fi

STEPS=(check gresource gtk tests gates)
SKIP="${SKIP:-}"
for s in $SKIP; do
    [[ " ${STEPS[*]} " == *" $s "* ]] || { echo "Unknown step in SKIP: $s (steps: ${STEPS[*]})" >&2; exit 2; }
done

base="${CANTO_ROUNDS:-${TMPDIR:-/tmp}/canto-rounds}"
id="$(date +%Y%m%d-%H%M%S)"
dir="$base/$id"
n=1
mkdir -p "$base" || { echo "could not create $base" >&2; exit 1; }
# mkdir without -p fails when the folder exists, so two rounds never share one
until mkdir "$dir" 2>/dev/null; do
    dir="$base/$id-$n"
    n=$((n + 1))
    [[ $n -le 100 ]] || { echo "could not create a round folder in $base" >&2; exit 1; }
done
CANTO_OUT="$dir"
export CANTO_OUT
exec > >(tee -a "$dir/round.log") 2>&1

canto_lock "round.sh $dir"
echo "Round $dir at $(git rev-parse --short HEAD 2>/dev/null)$(git diff --quiet HEAD 2>/dev/null || echo ' plus uncommitted changes')"

# Machine state next to each step's time, to tell slow code from a slow machine
cpu_mhz() { awk -F: '/^cpu MHz/ { s += $2; n++ } END { if (n) printf "%.0f", s / n; else printf "-" }' /proc/cpuinfo; }
power() {
    local p state="-"
    for p in /sys/class/power_supply/*; do
        [[ "$(cat "$p/type" 2>/dev/null)" == Mains ]] || continue
        if [[ "$(cat "$p/online" 2>/dev/null)" == 1 ]]; then state=ac; else state=battery; fi
    done
    printf '%s' "$state"
}
temp_c() { cat /sys/class/thermal/thermal_zone*/temp 2>/dev/null | sort -n | tail -1 | awk '{ printf "%.0f", $1 / 1000 }'; }
load1() { cut -d' ' -f1 /proc/loadavg; }
record() {
    local step="$1" secs="$2" result="$3"
    mkdir -p "$CANTO_DEV_STATE" 2>/dev/null || return 0
    local tsv="$CANTO_DEV_STATE/rounds.tsv"
    [[ -s "$tsv" ]] || printf 'time\tround\tstep\tseconds\tresult\tcpu_mhz\tpower\tload1\ttemp_c\n' > "$tsv"
    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$(date -Iseconds)" "$dir" "$step" "$secs" "$result" \
        "$(cpu_mhz)" "$(power)" "$(load1)" "$(temp_c)" >> "$tsv"
}

verdict() {
    echo "$1"
    exit "${2:-0}"
}

step_gresource() {
    local jobs=()
    [[ -n "$CANTO_JOBS" ]] && jobs=(-j "$CANTO_JOBS")
    canto_heavy ninja -C "$CANTO_BUILDDIR" "${jobs[@]}" src/canto.gresource > "$dir/gresource.log" 2>&1 \
        || { tail -20 "$dir/gresource.log"; return 1; }
    scripts/checks/lint-blueprints.sh
}

step_gtk() {
    # A busy machine can upset a timing-sensitive check now and then: one retry,
    # with fresh renders, and the round log says so
    "$dev/gtk-tests.sh" --dump "$dir/dump" && return 0
    echo "gtk: retrying once"
    mv "$dir/gtk-tests.log" "$dir/gtk-tests-first.log" 2>/dev/null
    mv "$dir/dump" "$dir/dump-first" 2>/dev/null
    sleep 20
    "$dev/gtk-tests.sh" --dump "$dir/dump"
}

not_run=""
step_gates() {
    local gates=(rustfmt blueprints shellcheck icons translations)
    if ! command -v shellcheck > /dev/null 2>&1; then
        echo "WARNING: shellcheck is not installed: that gate does NOT run here (CI runs it)."
        gates=(rustfmt blueprints icons translations)
        not_run="shellcheck"
    fi
    scripts/checks/check-quality.sh "${gates[@]}"
}

declare -A FAIL_NAME=([check]=CHECK-FAIL [gresource]=GRES-FAIL [gtk]=GTK-FAIL [tests]=TESTS-FAIL [gates]=GATES-FAIL)
for s in "${STEPS[@]}"; do
    if [[ " $SKIP " == *" $s "* ]]; then
        echo "== $s skipped"
        continue
    fi
    echo "== $s $(date +%H:%M:%S) ($(cpu_mhz) MHz, $(power), load $(load1), $(temp_c) C)"
    start=$SECONDS
    case $s in
        check) "$dev/check.sh" ;;
        gresource) step_gresource ;;
        gtk) step_gtk ;;
        tests) "$dev/tests.sh" ;;
        gates) step_gates ;;
    esac
    rc=$?
    secs=$((SECONDS - start))
    if [[ $rc -ne 0 ]]; then
        record "$s" "$secs" fail
        echo "$s failed after ${secs}s (logs in $dir)"
        verdict "${FAIL_NAME[$s]}" 1
    fi
    record "$s" "$secs" ok
    echo "$s ok in ${secs}s"
done

echo "== summary"
grep -h '^test result' "$dir/gtk-tests.log" "$dir/tests.log" 2>/dev/null
[[ -d "$dir/dump" ]] && echo "$(find "$dir/dump" -name '*.png' | wc -l) PNG renders in $dir/dump"
notes=()
SKIP_TRIM="$(echo "$SKIP" | xargs)"
[[ -n "$SKIP_TRIM" ]] && notes+=("skipped: $SKIP_TRIM")
[[ -n "$not_run" ]] && notes+=("not run: $not_run")
if [[ ${#notes[@]} -gt 0 ]]; then
    echo "WARNING: not everything ran (${notes[*]}). It counts only if it passed elsewhere on this same code."
    verdict "ROUND-OK ($(IFS=';'; echo "${notes[*]}" | sed 's/;/; /g'))"
fi
verdict "ROUND-OK"
