#!/usr/bin/env bash
# Runs the GTK tests (ignored in a plain cargo test) on a headless Broadway display.
#
# Usage: scripts/dev/gtk-tests.sh [filter] [--dump DIR]
#   filter      only the GTK tests whose name, inside the GTK test module, starts with it:
#               an area (lyrics, queue, library, now_panel, shell_layout, playback_bar,
#               settings, resources_icons, dumps) or one test (lyrics::lyrics_options)
#   --dump DIR  also write the PNG renders there (a new or empty folder)
#
# The tests never touch the desktop session: a free Broadway display of their own, the
# repo's settings schema compiled into a temp dir with in-memory settings, a private
# D-Bus session, no portals, and no Wayland, X11 or compositor variables.
# Refuses a gresource older than any .blp, .css, icon or resource list: rebuild it with
#   ninja -C build src/canto.gresource
# The tests run one at a time, in name order; each ends with a "GTK test <name>: ok in
# <s>" (or FAILED) line, which is what this script shows of them. The full output goes to
# $CANTO_OUT/gtk-tests.log (default $TMPDIR/canto-dev).
# Exit code: the test run's own; 1 also when no test ran. Settings: see scripts/dev/lib.sh.
set -uo pipefail

# shellcheck source=scripts/dev/lib.sh
. "$(dirname "${BASH_SOURCE[0]}")/lib.sh"
canto_load_env
cd "$CANTO_REPO" || exit 1

# The module that holds the GTK tests. The filter always starts with it, so
# --include-ignored never starts an unrelated ignored test (like a large download).
GTK_MODULE="app::components::gtk_tests::"

filter=""
dump=""
while [[ $# -gt 0 ]]; do
    case "$1" in
        --dump)
            [[ $# -ge 2 ]] || { echo "--dump needs a folder" >&2; exit 2; }
            dump="$2"
            shift 2
            ;;
        -h | --help)
            awk 'NR > 1 && /^#/ { print; next } NR > 1 { exit }' "$0"
            exit 0
            ;;
        -*)
            echo "Unknown option: $1" >&2
            exit 2
            ;;
        *)
            [[ -z "$filter" ]] || { echo "Only one filter" >&2; exit 2; }
            filter="$1"
            shift
            ;;
    esac
done

fail() {
    echo "GTK-FAIL: $*"
    exit 1
}

out="${CANTO_OUT:-${TMPDIR:-/tmp}/canto-dev}"
mkdir -p "$out"
log="$out/gtk-tests.log"
rm -f "$log"

gres="$(canto_gresource)" || fail "$gres"
if [[ -n "$dump" ]]; then
    mkdir -p "$dump" || fail "could not create $dump"
    [[ -z "$(ls -A "$dump")" ]] || fail "$dump is not empty: renders from another run would mix in"
    dump="$(cd "$dump" && pwd)"
fi

canto_lock "gtk-tests.sh ${filter:-all}"
canto_session_start "$gres" || fail "could not start the test session"
if [[ -n "$dump" ]]; then
    CANTO_SESSION_ENV+=(CANTO_TEST_DUMP_DIR="$dump")
fi

mapfile -t jobs < <(canto_jobs_args)
canto_heavy env "${CANTO_SESSION_ENV[@]}" dbus-run-session -- \
    "$CARGO" test "${CANTO_PACKAGES[@]}" "${jobs[@]}" --color never "$GTK_MODULE$filter" \
    -- --include-ignored --nocapture --test-threads=1 2>&1 \
    | tee "$log" \
    | grep -E "^GTK test |test result|panicked|^error|^\s+-->"
rc=${PIPESTATUS[0]}

if [[ -n "$dump" ]]; then
    echo "$(find "$dump" -name '*.png' | wc -l) PNG renders in $dump"
fi
if [[ $rc -ne 0 ]]; then
    echo "GTK-FAIL: cargo test exited with $rc (full output: $log)"
    exit "$rc"
fi
grep -Eq '^test result: ok\. [1-9][0-9]* passed' "$log" \
    || fail "no GTK test ran${filter:+ for filter \"$filter\"} (full output: $log)"
echo "GTK-OK"
