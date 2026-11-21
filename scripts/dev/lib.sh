# shellcheck shell=bash
# Shared setup for the scripts in scripts/dev/. Source it, do not run it.
#
# Machine settings come from the environment, with neutral defaults:
#   CANTO_NICE           niceness for heavy steps (default 0)
#   CANTO_JOBS           parallel jobs for cargo and ninja (default: the tool's own)
#   CARGO                the cargo to run (default: cargo)
#   CANTO_BUILD_WRAPPER  a command prefix for heavy steps, e.g. a CPU-limited scope
#                        (default: none; split on spaces)
#   CANTO_LOCK           the lock file heavy steps share (default: canto-dev.lock in
#                        $XDG_RUNTIME_DIR, or in /tmp)
#   CANTO_BUILDDIR       the meson build dir (default: build)
#   CANTO_DEV_STATE      where round.sh appends its timings
#                        (default: $XDG_STATE_HOME/canto-dev)
# A local env file, ${XDG_CONFIG_HOME:-~/.config}/canto-dev/env (or the file named by
# CANTO_DEV_ENV), is read when it exists. It is a shell file of KEY=value lines and is
# never committed. Variables already set in the environment win over the file.
# CARGO_HOME is <repo>/cargo when that exists, the same place the meson build uses
# (CANTO_CARGO_HOME overrides it).

CANTO_REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
export CANTO_REPO

# One fixed package selection for every cargo call in these scripts. A different
# selection can unify dependency features differently and rebuild a lot: the other
# crates' tests run with their own cargo test -p <crate>.
# shellcheck disable=SC2034
CANTO_PACKAGES=(-p canto)

canto_load_env() {
    local file="${CANTO_DEV_ENV:-${XDG_CONFIG_HOME:-$HOME/.config}/canto-dev/env}"
    if [[ -f "$file" ]]; then
        local saved
        saved="$(declare -px)"
        set -a
        # shellcheck disable=SC1090
        . "$file"
        set +a
        # What the caller exported wins over the file (-g: not local to this function)
        eval "${saved//declare -/declare -g}" 2>/dev/null || true
    fi
    : "${CANTO_NICE:=0}"
    : "${CANTO_JOBS:=}"
    : "${CARGO:=cargo}"
    : "${CANTO_BUILD_WRAPPER:=}"
    : "${CANTO_LOCK:=${XDG_RUNTIME_DIR:-/tmp}/canto-dev.lock}"
    : "${CANTO_BUILDDIR:=build}"
    : "${CANTO_DEV_STATE:=${XDG_STATE_HOME:-$HOME/.local/state}/canto-dev}"
    # The meson build always sets CARGO_HOME to <repo>/cargo when it builds. A different
    # CARGO_HOME (a toolchain manager often exports one) changes every dependency's
    # path and rebuilds them all, so use the same one. CANTO_CARGO_HOME overrides it.
    if [[ -n "${CANTO_CARGO_HOME:-}" ]]; then
        CARGO_HOME="$CANTO_CARGO_HOME"
    elif [[ -d "$CANTO_REPO/cargo" ]]; then
        CARGO_HOME="$CANTO_REPO/cargo"
    fi
    export CANTO_NICE CANTO_JOBS CARGO CANTO_BUILD_WRAPPER CANTO_LOCK CANTO_BUILDDIR CANTO_DEV_STATE
    [[ -n "${CARGO_HOME:-}" ]] && export CARGO_HOME
    if [[ -n "$CANTO_JOBS" ]]; then
        export CARGO_BUILD_JOBS="$CANTO_JOBS"
    fi
    # A meson reconfigure finds cargo on PATH: put the chosen one first
    if [[ "$CARGO" == */* ]]; then
        PATH="$(dirname "$CARGO"):$PATH"
        export PATH
    fi
}

# Cargo's -j flag, or nothing
canto_jobs_args() {
    if [[ -n "$CANTO_JOBS" ]]; then
        printf '%s\n' "-j" "$CANTO_JOBS"
    fi
}

# Runs a heavy command with the wrapper and niceness from the settings
canto_heavy() {
    local wrapper=()
    if [[ -n "$CANTO_BUILD_WRAPPER" ]]; then
        read -r -a wrapper <<< "$CANTO_BUILD_WRAPPER"
    fi
    "${wrapper[@]}" nice -n "$CANTO_NICE" "$@"
}

# The gresource the tests load. Refused (non-zero, with the reason) when missing or
# older than any file it is built from.
canto_gresource() {
    local gres="${CANTO_GRESOURCE:-$CANTO_REPO/$CANTO_BUILDDIR/src/canto.gresource}" newer
    if [[ ! -s "$gres" ]]; then
        echo "$gres is missing or empty: build it with ninja -C $CANTO_BUILDDIR src/canto.gresource"
        return 1
    fi
    newer=$(cd "$CANTO_REPO" && find src \( -name '*.blp' -o -name '*.css' -o -name '*.svg' \
        -o -name '*.png' -o -name '*.ui' -o -name '*.gresource.xml' \) -newer "$gres" -print | head -5)
    if [[ -n "$newer" ]]; then
        echo "$gres is older than: $(echo "$newer" | tr '\n' ' ')- rebuild it with ninja -C $CANTO_BUILDDIR src/canto.gresource"
        return 1
    fi
    printf '%s\n' "$gres"
}

# Starts a test session that never touches the desktop: a free Broadway display, the
# repo's settings schema compiled into a temp dir with in-memory settings, no portals,
# no Wayland, X11 or compositor variables. Fills CANTO_SESSION_ENV (arguments for env)
# and stops the display on exit. Run the tests with:
#   env "${CANTO_SESSION_ENV[@]}" dbus-run-session -- cargo test ...
canto_session_start() {
    local gres="$1" n
    CANTO_SESSION_TMP="$(mktemp -d "${TMPDIR:-/tmp}/canto-session.XXXXXX")" || return 1
    CANTO_BROADWAY_PID=""
    trap canto_session_stop EXIT
    mkdir -p "$CANTO_SESSION_TMP/schemas"
    if ! cp "$CANTO_REPO/data/io.github.gusthanks.Canto.gschema.xml" "$CANTO_SESSION_TMP/schemas/" \
        || ! glib-compile-schemas "$CANTO_SESSION_TMP/schemas"; then
        echo "could not compile the repo gschema"
        return 1
    fi
    # broadwayd exits at once when its port or socket is taken
    local display=""
    for n in $(seq 11 40); do
        gtk4-broadwayd ":$n" > "$CANTO_SESSION_TMP/broadway.log" 2>&1 &
        CANTO_BROADWAY_PID=$!
        sleep 1.5
        if kill -0 "$CANTO_BROADWAY_PID" 2>/dev/null; then
            display=":$n"
            # broadwayd leaves its socket behind when stopped
            CANTO_BROADWAY_SOCKET="${XDG_RUNTIME_DIR:-/tmp}/broadway$((n + 1)).socket"
            break
        fi
        wait "$CANTO_BROADWAY_PID" 2>/dev/null
        CANTO_BROADWAY_PID=""
    done
    if [[ -z "$display" ]]; then
        echo "no free Broadway display between :11 and :40"
        return 1
    fi
    echo "Broadway display $display"
    # shellcheck disable=SC2034  # read by the scripts that source this file
    CANTO_SESSION_ENV=(
        -u WAYLAND_DISPLAY -u DISPLAY -u HYPRLAND_INSTANCE_SIGNATURE -u SWAYSOCK -u CANTO_TEST_DUMP_DIR
        GDK_BACKEND=broadway BROADWAY_DISPLAY="$display"
        GDK_DEBUG=no-portals GIO_USE_VFS=local
        GSETTINGS_SCHEMA_DIR="$CANTO_SESSION_TMP/schemas" GSETTINGS_BACKEND=memory
        CANTO_TEST_GRESOURCE="$gres"
    )
}

canto_session_stop() {
    if [[ -n "${CANTO_BROADWAY_PID:-}" ]]; then
        kill "$CANTO_BROADWAY_PID" 2>/dev/null
        wait "$CANTO_BROADWAY_PID" 2>/dev/null
        [[ -S "${CANTO_BROADWAY_SOCKET:-}" ]] && rm -f "$CANTO_BROADWAY_SOCKET"
    fi
    [[ -n "${CANTO_SESSION_TMP:-}" ]] && rm -rf "$CANTO_SESSION_TMP"
    return 0
}

# Takes the shared lock for the rest of this shell, so heavy work from two agents or
# two terminals runs one after the other. A script started by one that already holds
# the lock (CANTO_LOCK_HELD) does not take it again. The holder is written next to the
# lock, so whoever waits is told who holds it.
canto_lock() {
    local what="$1"
    if [[ "${CANTO_LOCK_HELD:-}" == "$CANTO_LOCK" ]]; then
        return 0
    fi
    mkdir -p "$(dirname "$CANTO_LOCK")"
    exec {CANTO_LOCK_FD}>>"$CANTO_LOCK"
    if ! flock -n "$CANTO_LOCK_FD"; then
        # The holder writes its name just after it gets the lock
        sleep 1
        echo "Waiting for the dev lock $CANTO_LOCK, held by: $(cat "$CANTO_LOCK.owner" 2>/dev/null || echo unknown)"
        flock "$CANTO_LOCK_FD"
        echo "Got the dev lock."
    fi
    printf '%s (pid %s, started %s, in %s)\n' "$what" "$$" "$(date '+%F %T')" "$CANTO_REPO" > "$CANTO_LOCK.owner"
    CANTO_LOCK_HELD="$CANTO_LOCK"
    export CANTO_LOCK_HELD
}
