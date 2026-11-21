# AGENTS.md

The starting point for anyone changing Canto: AI agents and human contributors.
If another doc disagrees with this file, this file wins. Fix the other doc.

## 1. What Canto is

Canto is an unofficial Spotify client for the Linux desktop, written in Rust with
GTK4 and libadwaita and built with Meson. It is a fork of Riff (itself from Spot).
The UI follows one data flow: widgets dispatch actions, `AppState` changes, and
listeners update widgets from the events (see `doc/Design.md`).

| Path | What lives there |
| --- | --- |
| `src/` | The app crate `canto` (edition 2018): UI, playback, D-Bus |
| `src/app/state/` | `AppState`, actions and events |
| `src/app/components/` | Widgets and their listeners; each `.blp` and `.css` sits next to its `.rs` |
| `src/app/components/gtk_tests/` | GTK tests, one file per area |
| `src/connect/` | Spotify Connect (remote devices) |
| `src/player/` | librespot session and local playback |
| `src/dbus/` | MPRIS |
| `src/app/dev_tools/` | Dev menu, debug builds only (`Ctrl+Shift+D`) |
| `crates/canto-api/` | Web API client, providers, HTTP pool, caches, lyrics |
| `crates/canto-auth/` | OAuth2 (PKCE) login and the keyring token store |
| `crates/canto-core/` | Navigation history and play queue, no UI |
| `crates/canto-audio-engine/` | DSP chain between librespot and the audio backend |
| `crates/canto-config/` | Constants, settings, feature flags |
| `generated/spotify-api/` | Generated Web API crate: never edit by hand |
| `data/` | GSettings schema, desktop file, metainfo, icons |
| `po/` | Translations (gettext) |
| `scripts/dev/` | The development loop (below) |
| `scripts/checks/` | The five CI quality gates |
| `flatpak/` | Flatpak manifests and `flatpak/cargo-sources.json` |

## 2. Commands

First time: `./scripts/setup-dev.sh`, then `./scripts/build.sh dev` (a debug build
in the meson dir `build/`). Run everything from the repo root.

| Goal | Command |
| --- | --- |
| A full round: compile, UI resources + Blueprint lint, GTK tests with PNGs, suite, gates | `scripts/dev/round.sh` |
| Skip steps that already passed | `SKIP="check gresource" scripts/dev/round.sh` |
| Compile the tests, fail on any warning | `scripts/dev/check.sh` |
| One GTK area, or one GTK test | `scripts/dev/gtk-tests.sh lyrics`, `scripts/dev/gtk-tests.sh lyrics::lyrics_options` |
| GTK tests with PNG renders | `scripts/dev/gtk-tests.sh [filter] --dump DIR` (DIR new or empty) |
| The rest of the app suite | `scripts/dev/tests.sh [filter]` |
| Another crate's tests | `cargo test -p canto-api` (a separate build: slow the first time) |
| UI resources after a `.blp`, `.css` or icon change | `ninja -C build src/canto.gresource` |
| The five CI gates, or some | `scripts/checks/check-quality.sh [rustfmt blueprints shellcheck icons translations]` |
| Translations | `./scripts/build.sh pot`, then `./scripts/build.sh update-po`, then review every fuzzy entry |
| Set translations from a JSON map | `scripts/dev/po-set.py LANG PAIRS.json` |
| Full build, before any install or merge | `meson compile -C build` |
| Format check on every commit | `git config core.hooksPath scripts/hooks` |

- `round.sh` ends with `ROUND-OK` or `<STEP>-FAIL`. Each round writes to a new
  folder under `$TMPDIR/canto-rounds`. The round has no release build.
- GTK tests are `#[ignore]` in a plain `cargo test`. They need Broadway and the
  gresource, which `gtk-tests.sh` sets up in an isolated session (own display,
  in-memory settings, private D-Bus, no portals). `./scripts/build.sh test`
  (meson test) never runs them.
- `gtk-tests.sh` and `tests.sh` refuse a gresource older than any `.blp`, `.css`
  or icon. Rebuild it as above.
- Machine settings come from env, with neutral defaults: `CANTO_NICE`,
  `CANTO_JOBS`, `CARGO`, `CANTO_BUILD_WRAPPER`, `CANTO_LOCK`, `CANTO_BUILDDIR`,
  `CANTO_DEV_STATE`. A local, never committed `~/.config/canto-dev/env` can set
  them. Details: `scripts/dev/lib.sh`.
- Heavy steps share one lock file, so a second caller waits and is told who
  holds it. Run one heavy job (compile, GTK run, round) at a time.

## 3. Hard rules

Code:
- The app crate is edition 2018. A lone `"{x}"` in `assert!` or `panic!` prints
  literally and `array.into_iter()` yields references; `src/main.rs` makes both
  compile errors. Keep the test build at 0 warnings.
- A custom widget wrapper needs
  `@implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget`.
- A new widget made from a template is listed in `composite_templates_build`
  (`src/app/components/gtk_tests/resources_icons.rs`), which catches a Blueprint
  id that does not match its `#[template_child]`.
- Never hold `last_state` in `src/connect/player.rs` across an `.await`: clone
  it first. Holding the lock once deadlocked the whole Connect thread.
- No new crates without regenerating `flatpak/cargo-sources.json`
  (`./scripts/build.sh cargo-src`): the Flatpak build is offline and cannot
  fetch a crate that file does not list.

UI, resources and translations:
- Call `gettext()` and `ngettext()` unqualified (`use gettextrs::gettext;`).
  List every new file with translatable strings in `po/POTFILES`.
- `update-po` guesses similar translations and marks them fuzzy. Fix or clear
  every fuzzy entry you created; CI also requires the regenerated files.
- Window titles that window-manager rules match stay untranslated (for example
  "Canto Mini Player" in `src/app/components/shell/mini_player.rs`).
- Every icon is bundled in the gresource or shipped by GTK or libadwaita
  (`scripts/checks/check-icons.sh`). Symbolic icons are drawn with fills, no
  strokes, so GTK can recolor them.
- A new GSettings key goes into `data/io.github.gusthanks.Canto.gschema.xml`.
  Reading a key the compiled schema lacks aborts the app; the dev scripts
  compile the repo schema for tests.
- Never edit `.blp`, `.css` or icons while a build runs: the resources are
  bundled at the end of the build and the change would ship mixed.
- Never install over a running Canto: it memory-maps the installed
  `canto.gresource`, and replacing the file can crash it.

Git:
- One topic per commit, in English. Run rustfmt; keep style-only commits apart.
- Stage files one by one. Never `git add -A` or `git add .`.
- Work on a branch. `main` moves, and anything is pushed, only with the
  maintainer's OK.
- Never a bare `git stash`: the stash stack is shared by every worktree. A
  change you did not make belongs to another session: do not stage, stash or
  revert it.
- Never print, log or commit a key or token.

Proof:
- A new or changed check must be seen failing with its fix undone (back up,
  undo, run, restore), then passing. Put the red evidence in the commit message.
- "Done" only after a real run of the check, not a reasoned guess.

## 4. What tests cannot see

- Broadway renders at whole-number scales only. A 1 px line, or anything that
  depends on a fractional scale, shows only on a real screen.
- The Broadway screen is 1024 px wide, and a test window stays near 768 px tall
  whatever it asks for. Timing-dependent layout (a narrow window, an overlay
  opening) is not always settled there: wait for stable readings, never fixed
  sleeps.
- Measure geometry with `compute_bounds`; do not eyeball PNGs.
- A test sample must mount the real parent widgets (see the `sample_for_tests`
  functions). A widget alone in a window can pass while the app cuts it.
- Always say what was not seen on a real screen.

## 5. Product rules

- Canto is unofficial: no Spotify logos or branding in the UI or icons.
- Nothing that puts the user's Spotify account at risk.
- Keys the user brings (BYOK: Last.fm, ListenBrainz, Genius and others) stay on
  the user's machine. The session log (`src/session_log.rs`) never holds tokens.

## 6. Where things go

- A new trap that anyone could hit goes into this file, or into the doc for
  its area, in the same commit as the fix.
- A rule tied to one spot in the code goes in a comment at that line.
- When a UI part moves, update the docs that name it in the same commit.

## 7. More docs

- `CONTRIBUTING.md`: ways to contribute, issues, AI-assisted changes.
- `doc/Development.md`: setup, layout, feature flags, translations, debugging.
- `doc/Design.md`: data flow, the player, the audio engine, components.
- `doc/Release.md`: cutting a release.
- `docs/providers-architecture.md`: the provider layer.
- `README.md`: what Canto does, for users.
