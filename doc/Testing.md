# Testing Canto

How the tests are laid out, how to run the GTK tests on a headless display, and
how to write a check that proves something. The commands and the hard rules live
in [`AGENTS.md`](../AGENTS.md); the traps behind most checks are in
[`Pitfalls.md`](Pitfalls.md).

## 1. What runs where

| Tests | Where | How to run |
| --- | --- | --- |
| App unit tests (models, state, parsers) | `src/**`, `#[cfg(test)]` | `scripts/dev/tests.sh [filter]` |
| GTK tests (real widgets, renders) | `src/app/components/gtk_tests/`, one file per area | `scripts/dev/gtk-tests.sh [filter] [--dump DIR]` |
| Web API, lyrics, scrobbling, caches | `crates/canto-api/` | `cargo test -p canto-api` |
| Login and token store | `crates/canto-auth/` | `cargo test -p canto-auth` |
| Queue and navigation | `crates/canto-core/` | `cargo test -p canto-core` |
| DSP chain | `crates/canto-audio-engine/` | `cargo test -p canto-audio-engine` |
| Settings and feature flags | `crates/canto-config/` | `cargo test -p canto-config` |
| Everything for the app crate, plus the CI gates | | `scripts/dev/round.sh` |

- A round runs `check`, `gresource`, `gtk`, `tests` and `gates` and stops at the
  first failure. It covers the `canto` crate only: when you touch another crate,
  run its `cargo test -p` too and say so.
- Each `cargo test -p <crate>` outside `canto` is its own build. The first run is
  slow, and the next round may rebuild more. Run one at a time.
- `./scripts/build.sh test` (meson test) never runs the GTK tests.
- Tests that talk to a live service are `#[ignore]` and run only by name, for
  example `live_keyless` in `crates/canto-api/src/lyrics/translate.rs` or
  `downloads_the_current_build` in `src/connect/soloist.rs`. Run them in an
  empty `HOME` and `XDG_DATA_HOME`, so they cannot find or change a real install.
- Tests that reach the system keyring (`canto-auth`, through `oo7`) run inside
  `dbus-run-session` with `HOME` set to a temp dir, so the keyring daemon never
  writes to the real one.

## 2. The GTK tests

Each GTK check is its own `#[test] #[ignore]`, declared with the `gtk_tests!`
macro in `src/app/components/gtk_tests/mod.rs`. The macro calls `run`, which:

- refuses to start without `CANTO_TEST_GRESOURCE` and `GDK_BACKEND=broadway`, so
  a plain `cargo test --include-ignored` fails with a hint instead of opening
  windows on the desktop;
- runs every check on one shared GTK thread (`gtk::test_synced`), sets the
  process up once (`setup`: libadwaita, the gresource, every style sheet, the
  icons, the custom widget types) and puts the state back after each check
  (`clean_up`: closes windows, restores animations, text size and settings);
- fails the test when libadwaita reports a breakpoint bin overflowing
  ("exceeds"), or GTK allocates a negative size or logs a critical, even if
  every assertion passed;
- prints `GTK test <name>: ok in <s>` (or `FAILED`), which is the line
  `gtk-tests.sh` shows.

Inside a check, prefer `check(what, ok, seen)` over `assert!`: it records the
failure and carries on, so one run reports every broken check with what it
measured. A check never depends on another having run first.

To add one: write `fn assert_<thing>()` in the area's file, add
`<name> => assert_<thing>` to that file's `gtk_tests!` block, and give the
widget a `sample_for_tests` builder if it needs sample data (section 4).

## 3. The headless recipe

`scripts/dev/gtk-tests.sh` builds an isolated session (see `canto_session_start`
in `scripts/dev/lib.sh`):

- a free Broadway display of its own (`gtk4-broadwayd :N`, from `:11` up);
- the repo's `data/*.gschema.xml` compiled into a temp dir, with
  `GSETTINGS_BACKEND=memory`, so a new key works before any install and no test
  touches real settings;
- `GDK_DEBUG=no-portals`, `GIO_USE_VFS=local` and a private D-Bus session
  (`dbus-run-session`);
- `WAYLAND_DISPLAY`, `DISPLAY` and compositor variables unset, so nothing
  reaches the desktop;
- `CANTO_TEST_GRESOURCE` pointing at `build/src/canto.gresource`. The script
  refuses a gresource older than any `.blp`, `.css` or icon:
  `ninja -C build src/canto.gresource`.

Running a subset:

```sh
scripts/dev/gtk-tests.sh                           # every GTK test
scripts/dev/gtk-tests.sh lyrics                    # one area
scripts/dev/gtk-tests.sh lyrics::lyrics_options    # one check
scripts/dev/gtk-tests.sh queue --dump /tmp/q-png   # with PNG renders (new or empty folder)
```

The full output goes to `$CANTO_OUT/gtk-tests.log` (default `$TMPDIR/canto-dev`).
A bare "error: test failed" means the answer is in that log: read it without the
`dbus-daemon` and portal lines.

## 4. Samples and renders

- Give a widget a `sample_for_tests(...)` function under `#[cfg(test)]` that
  builds it with sample data (a song, `sample_cover()` as the cover). Re-export
  it with `pub(crate) use` under `#[cfg(test)]` so it raises no dead-code warning.
- A sample mounts the real parent chain: the same title, `ScrolledWindow`,
  hidden siblings and the same function the app uses to fit it. A grid alone in
  a big window once passed while the app cut it after one row.
- `distinct_colors(widget, "name")` renders the widget and, with
  `CANTO_TEST_DUMP_DIR` set (`--dump`), writes `name.png`. The renders in
  `src/app/components/gtk_tests/dumps.rs` show the main areas with sample data only.
- The window frame without a login: build `window.ui` with
  `gtk::Builder::from_resource`, take `window`, set a size, `present()`, wait.
- To see the end of a scrolled page, set the `ScrolledWindow`'s `vadjustment`
  to its end, wait, and render a second PNG (`<name>-end`).
- A side panel that must be open in a render: open it before `present()`. Opened
  on a window already on screen, the slide does not always finish headless.

## 5. Measure, do not eyeball

- Geometry: `widget.compute_bounds(&window)` gives x, y, width and height. Compare
  them between states (idle and hover, narrow and wide) instead of judging a PNG.
- Hover: add the `hovering` class, wait, measure, remove it. A fresh window's
  first reading can differ from every later one; do one hover pass before
  measuring the idle state.
- Finding the rule to blame: walk the tree from `first_child` and print each
  node's `css_name`, `css_classes`, `width()` and `measure(Horizontal, -1)`.
- Fine detail (a rounded corner, a color): read pixels, not the picture.
  `pixel_at`, `pixels_in` and `brightest_in` in `src/app/components/gtk_tests/mod.rs` read a fresh
  render; a rounded corner means the corner pixel has the background color.
- Find children by CSS class (`with_class`) or by builder id (`by_id`), never by
  position: a new child shifts positions and the check reads the wrong widget.
- A `WidgetPaintable` can come back empty ("no render node"): retry with
  `queue_draw()` and a short pump between tries (the helpers already do).
- A CSS-only before and after: render once with the rule removed, once with it,
  and join the two with `magick before.png after.png +append compare.png`.

## 6. Waiting

Never a fixed sleep. Broadway frames are slow and their pace changes with the
machine's load, so a sleep that passes today fails on a busy machine.

| Helper | Waits until |
| --- | --- |
| `wait_mapped(widget, settle_ms)` | the widget is mapped (up to 5 s), then settles |
| `wait_frames(widget, n)` | `n` frames of the window have started, counted by a tick callback (up to 10 s) |
| `wait_laid_out(widget, relative_to)` | the widget has a size and keeps its bounds for three reads |
| `settled(read)` | `read()` gives the same value four times in a row, 100 ms apart |
| `pump(ms)` | only runs the main loop; use it between steps, not to wait for a result |

- After `present()` or a change that needs a new frame, wait for frames
  (`wait_frames`), not for a value to hold still: a value can hold still for
  100 ms simply because no frame came.
- When the thing under test moves (a resize, a panel opening), wait for the
  reading to settle and for the expected class or state to show, then measure.
- `no_animations()` turns animations off, so a check reads each change's end
  state at once. `force_dark()` gives the look the sample renders use.

## 7. Prove the check fails without the fix

A check that was never red proves nothing. For every new or changed check:

1. Back the fixed file up (`cp file $TMPDIR/file.fixed`).
2. Undo only the fix (for example with `sed`), rebuild what it needs (the
   gresource for `.blp` or `.css`), and run the one check. It must fail, and its
   message should name what broke.
3. Restore the backup, confirm with `git diff` that the tree is as before, run
   the check again (green), then the whole round.
4. Put the red output (the failing check line) in the commit message.

To prove a CSS check without editing `src/`, build a second gresource with
`glib-compile-resources --sourcedir=<dir with the changed css> --sourcedir=src
--sourcedir=build/src src/canto.gresource.xml` and point `CANTO_TEST_GRESOURCE`
at it for one run.

## 8. What the tests cannot see

- Broadway renders at whole-number scales only. A 1 px line, or anything that
  depends on a fractional scale, shows only on a real screen.
- The Broadway screen is 1024 px wide, and a test window stays near 768 px tall
  whatever it asks for. Fix the sidebar widths of both renders when comparing
  before and after.
- Timing-dependent layout (a bar collapsing in a narrow window, an overlay
  opening) does not always settle there. Render it, and confirm the behavior in
  the app instead of asserting it.
- A `GtkStack` with a crossfade can keep showing the old child in a PNG; assert
  `visible_child()` as well.
- Some warnings appear only in the app's log, never in a test: CSS parse
  warnings, and "GtkFlowBox with a model will ignore sort and filter
  functions". Read the session log after a real run.
- Real pointer and touchpad input (natural scrolling, a click on a control that
  answers only to a real pointer) is simulated at best.
- Always say what was not seen on a real screen.
