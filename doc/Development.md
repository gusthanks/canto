# Development Guide

This guide walks you from a clean checkout to a running debug build of Canto, then covers how the codebase is organized, expectations of your first Pull Request, and the tools available for development. Start at [`AGENTS.md`](../AGENTS.md) for the commands and the hard rules; if the two disagree, `AGENTS.md` wins.

## Quick Setup

Get a development build running in three steps. Run these from the repository root.

```sh
# 1. Install dependencies (detects dnf, apt-get, pacman, or zypper)
./scripts/setup-dev.sh

# 2. Build a debug build in build/ and install to ~/.local (close Canto first)
./scripts/build.sh dev --install

# 3. Run with debug logging
RUST_LOG='canto=debug,librespot=error' ~/.local/bin/canto
```

The generated Spotify Web API crate (`generated/spotify-api/`) is committed, so there is nothing to generate first. See [Generated Code](#generated-code) for when to regenerate it.

For a release build, use `./scripts/build.sh release --install`. See the [detailed setup and build reference](#appendix-detailed-reference) below for manual dependency lists, Meson/Justfile usage, and GNOME Builder.

## Architecture Overview

Canto is a GTK4/libadwaita application written in Rust, built with Meson (Ninja backend). The core follows a unidirectional data flow:

- **AppState** is the single source of truth, mutated only via dispatched **actions**.
- Actions produce **events** that listeners use to update GTK widgets.
- All dispatching flows through a `futures::channel::mpsc` channel consumed on the GLib main loop.
- The app state is only readable from the main thread.

See [`Design.md`](Design.md) for the full data flow explanation.

### Directory Structure

| Directory | Responsibility |
| --- | -- |
| `crates/` | Canto crates used by the main application - see below. |
| `crates/canto-config/` | Shared constants, settings and feature flags for the Canto workspace. |
| `crates/canto-auth/` | Authentication: OAuth2 (PKCE) flow and keyring-backed credential store. |
| `crates/canto-api/` | Data layer for Canto: Web API, lyrics, scrobbling and caching. |
| `crates/canto-api/src/providers/` | Provider-neutral music service trait and per-vendor implementations. |
| `crates/canto-api/src/providers/spotify/` | Spotify provider: domain types and Spotify-to-provider-neutral conversion. |
| `crates/canto-api/src/http/` | Shared HTTP connection pool and retry logic. |
| `crates/canto-core/` | App logic between the UI and the API layer, free of UI dependencies: navigation history and the play queue. |
| `crates/canto-audio-engine/` | Audio engine: DSP chain between librespot and the audio backend (EQ, mono, pan, pitch, mix, limiter, level meter). |
| `crates/canto-api/src/cache/` | Caching layer: disk TTL cache, texture LRU, domain object LRU. |
| `src/` | Rust source code and bundled UI assets. |
| `src/app/` | Core application logic and state. |
| `src/app/state/` | Centralized application state and reducers. |
| `src/app/models/` | Presentation models binding state to UI. |
| `src/app/components/` | GTK widget wrappers and event listeners. |
| `src/app/components/gtk_tests/` | GTK tests, one file per area (run with `scripts/dev/gtk-tests.sh`). |
| `src/app/dev_tools/` | Dev menu (debug builds only). |
| `src/connect/` | Spotify Connect device support. |
| `src/player/` | librespot session and local playback management. |
| `src/dbus/` | MPRIS / D-Bus integration. |
| `data/` | Application data installed on the system (icons, desktop file, appstream, GSchema). |
| `po/` | Translations (gettext). |
| `scripts/` | Dependency setup and build helpers (`setup-dev.sh`, `build.sh`), plus the subfolders below. |
| `scripts/checks/` | Quality checks run in CI (formatting, blueprints, ShellCheck, icons, translations). |
| `scripts/dev/` | The development loop: compile, GTK tests, suite and a full round. |
| `scripts/hooks/` | A pre-commit hook that checks rustfmt on staged files. |
| `scripts/api/` | Web API spec fetching, crate generation and the terminal API client. |
| `scripts/packaging/` | Flatpak packaging helpers. |
| `scripts/perf/` | Resource measurement and scripts that drive a running Canto (browsing, resizing). |
| `flatpak/` | Flatpak packaging manifests and cargo sources. |
| `generated/spotify-api/` | The generated Spotify Web API crate (committed; never edit by hand). |
| `doc/`, `docs/` | Developer documentation. |
| `subprojects/` | Meson subproject wrap files. |
| `.github/` | GitHub Actions workflows, issue templates, dependabot config. |

## PR Guidelines

Canto uses trunk-based development on `main`. Work happens on short-lived `canto/<topic>` branches and merges into `main` in small, complete increments. Open pull requests against `main`. Releases come from version tags (see [`Release.md`](Release.md)).

- **Prefer complete features over half-finished ones.** A PR should land a feature that works end to end. If a feature is not ready, gate it behind a [feature flag](#feature-flags) rather than merging a partial, user-visible implementation.
- **Squashing commits is recommended.** Collapse a PR into a focused set of commits (often a single commit) before merge so history stays readable.
- **Long commits are fine when the feature is pinpointed.** A large diff is acceptable as long as the commit represents one clearly scoped feature or change. Avoid mixing unrelated changes into the same commit.
- **Code comments are encouraged.** Explain intent and non-obvious decisions, especially around state, events, and the audio engine.
- **Unit tests are encouraged.** Add tests alongside new features and bug fixes. See [Testing and Debugging](#testing-and-debugging).
- **Show the test failing first.** A new or changed check should fail with its fix undone and pass with it; say so in the commit message.

### Code Style

- Rust code follows `rustfmt` with the config in `rustfmt.toml`. Turn on the pre-commit hook with `git config core.hooksPath scripts/hooks`.
- Indent with spaces: 4 for Rust, 2 for `.blp`, `.yml`, `.css`, and `.ui` files (see `.editorconfig`).
- UI updates must follow [GNOME HIG](https://developer.gnome.org/hig/) conventions.

## Feature Flags

Incomplete or experimental features are gated behind runtime feature flags rather than merged in a half-finished, always-on state. This keeps trunk-based development safe: a feature can land on `main` while remaining hidden until it is ready.

Flags are GSettings booleans defined in `crates/canto-config/src/feature_flags.rs` and toggled in Preferences, under "Experimental Features" on the General page.

### Creating a New Feature Flag

Adding a flag takes two coordinated changes: the Rust enum and the GSettings schema.

1. In `crates/canto-config/src/feature_flags.rs`, add a variant to the `FeatureFlag` enum, add it to `FeatureFlag::ALL`, and fill in the `key()`, `title()`, and `description()` match arms:

   ```rust
   pub enum FeatureFlag {
       // ...
       MyFeature,
   }

   // in key():   FeatureFlag::MyFeature => "feature-my-feature",
   // in title(): FeatureFlag::MyFeature => "My Feature",
   ```

2. In `data/io.github.gusthanks.Canto.gschema.xml`, add a matching boolean key (the name must equal `key()`):

   ```xml
   <key name="feature-my-feature" type="b">
     <default>false</default>
     <summary>Enable my experimental feature</summary>
   </key>
   ```

3. Rebuild so the schema is recompiled (`./scripts/build.sh dev --install` runs `glib-compile-schemas` via meson's post-install). Then gate the feature in code with `is_enabled(FeatureFlag::MyFeature)`.

### Using Feature Flags in Code

```rust
use canto_config::feature_flags::{is_enabled, FeatureFlag};

if is_enabled(FeatureFlag::DeviceSelector) {
    // show device selector
}
```

### Overriding Flags During Development

Use `gsettings` to toggle flags without opening the Settings dialog:

```sh
gsettings set io.github.gusthanks.Canto feature-device-selector true
gsettings set io.github.gusthanks.Canto feature-pinned-objects false
```

## Testing and Debugging

### Tests

The scripts in `scripts/dev/` are the everyday loop. They need a configured `build/` dir (`./scripts/build.sh dev`), and the GTK tests and the suite need the UI resources built (`ninja -C build src/canto.gresource`).

```sh
scripts/dev/round.sh                    # compile, resources + Blueprint lint, GTK tests with PNGs, suite, gates
SKIP="check gresource" scripts/dev/round.sh
scripts/dev/check.sh                    # compile the tests, fail on any warning
scripts/dev/gtk-tests.sh lyrics         # one GTK area (or one test: lyrics::lyrics_options)
scripts/dev/gtk-tests.sh --dump DIR     # also write the PNG renders to DIR
scripts/dev/tests.sh                    # the rest of the canto suite
cargo test -p canto-api                 # another crate's tests
```

The GTK tests are `#[ignore]` in a plain `cargo test`: they need a headless Broadway display, which `gtk-tests.sh` starts in an isolated session. `./scripts/build.sh test` runs `meson test`: `cargo test --workspace`, Clippy and the metainfo check, without the GTK tests. Machine settings (niceness, jobs, the cargo to use, a lock file) come from env; see `scripts/dev/lib.sh`.

`cargo test` needs `src/config.rs`, which meson writes when it configures the build. Without a meson build, create a placeholder first:

```sh
echo > src/config.rs
```

### Lint and Quality Checks

Run the five CI gates (rustfmt, blueprints, shellcheck, icons, translations) before committing, or name some of them:

```sh
./scripts/checks/check-quality.sh
./scripts/checks/check-quality.sh rustfmt translations
```

### Debug Tools (Dev Menu)

Debug builds include a developer menu in the navigation panel header, also opened with `Ctrl+Shift+D`. It is compiled only in debug builds and lives in `src/app/dev_tools/`. The menu provides:

| Tool | What It Does |
| --- | --- |
| Force Skeleton | Adds the `force-skeleton` CSS class to the window so all skeleton/loading states render simultaneously. Useful for testing loading UI without slow network conditions. |
| Debug CSS | Toggles an overlay stylesheet that highlights alignment and rendering issues. |
| Panel Sizes | Overlays each major panel (navigation panel, header, content stack, playback bar, utility panel) with its current pixel dimensions in a distinct color, and shows the window's size in the center. |
| Simulate Offline | Blocks all HTTP calls and kills the librespot session. The connection-lost banner appears naturally once requests fail, mirroring how a real network drop is detected. |
| Kill Player | Sends `DevKillPlayer` to force-terminate the playback session. |
| Kill Librespot Session | Sends `DevKillSession` to destroy the librespot TCP session without stopping local playback state. |
| Inject API Error | Forces every Spotify Web API request to fail with a chosen error (429 rate limit, 401 unauthorized, 500 server error, or empty response). Toggle buttons select the error type; "Off" clears it. |
| Expire OAuth Token | Backdates the cached token and forces an immediate refresh cycle. |
| Test Toast | Fires a notification to verify the toast overlay is working and positioned correctly. |
| Reload CSS | Reloads stylesheets from source files on disk (not bundled gresource copies). CSS edits apply immediately without restarting the app. |
| Simulate Unavailable Track | Injects one "track failed to load" event; click repeatedly to trip the stop threshold. |
| Dump State to Log | Logs a summary of the current application state: logged-in user, playlist count, browser screen/nav depth, selection state, and playback state. |
| Reset DRM Verification | Forgets that this account has played a track and re-arms the DRM detection. |
| Use Official Client | Hands playback off to the hidden official Spotify client. |

### Logs

Every session writes a debug log to `~/.local/state/canto/canto.log`, and the session before it is kept as `canto.prev.log` (see `src/session_log.rs`). Opening Canto rotates the files, so copy them before reopening after a crash. Authentication internals stay at warn level, so tokens never reach the file.

Canto uses the `env_logger` crate. `RUST_LOG` replaces the default filter, and the log then also goes to stderr:

```sh
# App debug, silence librespot
RUST_LOG='canto=debug,librespot=error' canto

# Trace-level for the API layer
RUST_LOG='canto_api=trace,canto=info' canto

# Everything at trace (very noisy)
RUST_LOG=trace canto
```

### HTTP Proxy

Canto uses [reqwest](https://github.com/seanmonstar/reqwest) for HTTP, which honors the usual proxy variables:

```sh
https_proxy=http://localhost:8080 canto
```

Certificate checks stay on. For a local MITM proxy with a self-signed certificate (mitmproxy, Charles, etc.), build a debug build with `--features canto-api/insecure-tls` and set `CANTO_ACCEPT_INVALID_CERTS=1`; release builds cannot turn checks off.

### Cache

Canto caches images and HTTP responses in `~/.cache/canto/`. Clear this directory to force fresh fetches.

<div style="page-break-after: always;"></div>

---

# Appendix: Detailed Reference

## Installing Dependencies

`./scripts/setup-dev.sh` detects your package manager (dnf, apt-get, pacman, zypper), lists packages, and asks before installing. On Debian/Ubuntu it also installs Rust via rustup since the packaged toolchain is too old.

To install manually, you need:

- Rust (stable toolchain)
- Meson (>= 0.59) and Ninja
- GTK4 and libadwaita development libraries
- GLib and OpenSSL development libraries
- GStreamer and GStreamer base plugins development libraries
- ALSA and PulseAudio development libraries
- libcurl development libraries
- `blueprint-compiler` (UI files)
- `gettext` (translations)
- `libxml2` / `libxml2-utils`
- `pkg-config`

## Generated Code

`generated/spotify-api/` is generated from `spotify-openapi.yaml` and committed. Regenerate it with `./scripts/api/generate-spotify-api.sh` (requires Java) only when the spec changes (`./scripts/api/fetch-spotify-spec.sh`), and never edit it by hand.


## Building

`./scripts/build.sh` handles the common workflows (debug/release builds, install, test, clean) and takes flags like `--install`, `--no-check`, and `--features`. Run `./scripts/build.sh --help` for the full list of modes and options.

Alternatives to the build script:

```sh
# Meson directly (use -Dbuildtype=release for an optimized build)
meson setup build -Dbuildtype=debug -Doffline=false --prefix="$HOME/.local"
ninja install -C build

# Justfile
just init-debug  # meson setup, debug build, in build/
just init        # meson setup, release build, in build/
just compile
just install
just run       # install + run with debug logging
```

GNOME Builder: open the project, make `flatpak/io.github.gusthanks.Canto.snapshots.json` active, and build. Requires `org.freedesktop.Sdk.Extension.rust-stable` matching the Freedesktop SDK GNOME uses.

## UI Development (Blueprint)

Widget layouts are written in [Blueprint](https://gnome.pages.gitlab.gnome.org/blueprint-compiler/) (`.blp`) files that live next to the Rust component that drives them (for example `src/app/components/shell/playback/playback_controls.blp`). At build time `blueprint-compiler` compiles every `.blp` into a `.ui` file, which is bundled into a GResource and loaded by the component's GObject template.

To add or change UI:

1. Edit the relevant `.blp` file, or add a new one.
2. If you added a file, register it in two places:
   - the `blueprints` `custom_target` input list in `src/meson.build`, so it gets compiled; and
   - `src/canto.gresource.xml`, so the resulting `.ui` (and any `.css`) is bundled. Reference the compiled `.ui` name, not the `.blp`.
3. Bind the template to a Rust widget with the usual `gtk::CompositeTemplate` pattern, using the resource path from `canto.gresource.xml`. A custom widget wrapper needs `@implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget`, and a new template type goes in the `composite_templates_build` list in `src/app/components/gtk_tests/resources_icons.rs`.
4. Rebuild. Blueprint and resource compilation run as part of the meson build; `ninja -C build src/canto.gresource` rebuilds only the resources.

The dev menu's "Reload CSS" reloads stylesheets from disk without a restart, but `.blp` changes require a rebuild.

## Adding a GSetting

Persistent settings are GSettings keys defined in `data/io.github.gusthanks.Canto.gschema.xml`. To add one:

1. Add a `<key>` (and an `<enum>` above the schema if it is an enumerated value) to the gschema. Boolean example:

   ```xml
   <key name="my-setting" type="b">
     <default>false</default>
     <summary>Short description of the setting</summary>
   </key>
   ```

2. Rebuild and install so the schema is recompiled (`./scripts/build.sh dev --install`; meson's post-install runs `glib-compile-schemas`). Reading a key that is not yet in the compiled schema aborts the app, so the schema must be installed before the code that uses it runs.
3. Read or write it via `gio::Settings::new("io.github.gusthanks.Canto")`.

See [Creating a New Feature Flag](#creating-a-new-feature-flag) for the flag-specific variant of this workflow.

## Translations

Strings live in `.blp` and `.rs` files listed in `po/POTFILES`. Call `gettext()` and `ngettext()` unqualified (`use gettextrs::gettext;`): older xgettext misses `gettextrs::gettext(...)`. Translations are `.po` files in `po/`, changed through pull requests; there is no Weblate project.

After adding, changing or removing translatable strings, regenerate the template and the `.po` files in the same change. The translations CI gate (`scripts/checks/check-translations.sh`) fails until both are up to date:

```sh
./scripts/build.sh pot        # ninja -C build canto-pot: regenerate po/canto.pot
./scripts/build.sh update-po  # ninja -C build canto-update-po: merge it into every po/*.po
```

`canto-update-po` guesses translations for new strings from similar ones and marks them `#, fuzzy`. Review every fuzzy entry: fix the translation and remove the flag, or clear it. `scripts/dev/po-set.py LANG PAIRS.json` sets translations for one language from a JSON map of msgid to msgstr (plural entries are set by hand).

## Spotify CLI

`scripts/api/spotify-cli.py` is an authenticated Spotify Web API client for the terminal. It shares credentials with Canto through the system keyring, so once you have logged in to the app you can hit the API directly to inspect responses while developing.

```sh
spotify-cli v1/me                     # GET the current user
spotify-cli v1/me/playlists           # list playlists (interactive pagination)
spotify-cli -X PUT v1/me/player/play -d '{"uris":["spotify:track:..."]}'
spotify-cli --check-auth              # verify stored credentials
spotify-cli --logout                  # clear stored credentials
spotify-cli --setup-completions       # enable bash/zsh tab completion
```

Run it with `uv` from the `scripts/` directory (`uv sync` first), or install it with `pip install .`. Output is colorized JSON.
