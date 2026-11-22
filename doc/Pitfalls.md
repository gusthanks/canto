# Pitfalls

Traps that cost real time in Canto, with how to spot them and where the fix
lives. Each entry: what you see, why, what to do, and the file to look at. Add
a new one in the same commit as its fix. How to test is in
[`Testing.md`](Testing.md); the hard rules are in [`AGENTS.md`](../AGENTS.md).

## 1. GTK 4.24 and libadwaita layout

**A CSS class or margin changed during an allocation.** You see "attempt to
allocate GtkToggleButton with width -5" or "gtk_widget_measure: assertion
'for_size >= -1' failed". A class added inside a size-allocate (a container
that refits before allocating its children) brings CSS margins after the child
was measured. Set margins as widget properties (`set_margin_start`), which
queue a fresh measure. See `apply_rail` in `src/app/components/shell/layout/mod.rs`.
The GTK tests fail on any negative allocation (`src/app/components/gtk_tests/mod.rs`).

**A filtered FlowBox child still counts.** Cards a `set_filter_func` hid keep
their old place, and code that walks the children pushes the shown ones a
column over. FlowBox hides filtered children with `set_child_visible(false)`,
which `should_layout()` ignores. Test both: `c.should_layout() &&
c.is_child_visible()`. See `align_cells` in
`src/app/components/widgets/card_list/card_grid.rs`.

**FlowBox or ListBox with a model ignores sort and filter functions.** The test
passes; only the app log says "GtkFlowBox with a model will ignore sort and
filter functions". Filter the model itself (`gtk::FilterListModel` with a
`CustomFilter`) and look items up by their index in that model. See
`src/app/components/shell/navigation_panel/navigation_panel.rs`.

**A hidden child does not pass `vexpand` up.** Swapping a viewport's child for a
box and hiding the list that had `vexpand` makes the `ScrolledWindow` shrink to
its minimum and cut the rest. Put `set_vexpand(true)` on the wrapping box too.

**`overflow` is not a CSS property.** The log says "No property named
overflow". It is a widget property: `overflow: hidden;` in the `.blp`, or
`set_overflow(gtk::Overflow::Hidden)`.

**`border-radius` does not clip a `GtkImage` or `GtkPicture`.** Wrap it in a
`Box` with `overflow: hidden` and put the class on the box.

**A shadow cut into a hard rectangle.** Some parent clips what passes its edge.
Do not clip the widget that has the shadow: round an inner child and keep the
shadow outside. Walk the whole chain: `Adw.BreakpointBin` clips by default
(`overflow: visible` in `src/app/components/widgets/details_page/header.blp`),
and a custom widget may clip in Rust (`grep -rn set_overflow`).

**A user setting and a breakpoint both drive `visible`.** One overwrites the
other. Wrap the control in a `Box` named `<name>_slot`: the setting binds the
slot's `visible`, the breakpoint keeps the inner widget's.

**A popover parented in Rust outlives its parent.** GTK warns at close, or the
test process crashes. Keep it in a `RefCell<Option<gtk::Popover>>` and call
`unparent()` in the widget's `dispose`.

**`Adw.Dialog` opens as its own window.** `present(Some(parent))` hosts the
dialog inside the parent only when the parent is an Adw window
(`libadwaita::Window` or `ApplicationWindow`); a plain `gtk::Window` gets a
separate floating one, so a test sees the wrong layout. Host test dialogs in
an Adw window, as `assert_add_lyrics_window` in `src/app/components/gtk_tests/lyrics.rs` does.

**`AdwBin` ignores your `measure`.** A widget that must be square by its width
(height-for-width) gets a height that ignores the width. Call
`set_layout_manager(None)` in `constructed`, or the Bin's layout manager
measures instead (see `src/app/components/shell/layout/clip_bin.rs`).

**`GtkSpinner` ignores `-gtk-icon-source`.** For an animated icon use a `Box`
with `background-image: -gtk-icontheme("name")`.

**A button inside a button never gets the click.** Use a `gtk::Image` with a
`GestureClick` that claims the sequence (`EventSequenceState::Claimed`) and sets
the pointer cursor.

**A child found by position breaks silently.** Adding a widget to a row shifts
`first_child`/`last_child`, and the old code reads the wrong one (a translation
line vanished). Find children by CSS class; see `child_with_class` in
`src/app/components/pages/lyrics/widget.rs`.

**Moving buttons between header bars scrambles their order.** `pack_start` and
`pack_end` append at the end of the side: remove and repack the neighbours too,
in order. A button outside a header bar needs the `flat` class.

**A selectable label shows fully selected.** It takes the window's first focus,
so its whole text renders highlighted. Do not `set_selectable(true)` on text
that is only read.

**The `pill` class is huge.** Adwaita gives it 32 px of padding. Inline links
and chips use `info-pill` from `src/app.css`, in a `FlowBox` with `hexpand`
whose children have `halign: start`.

**The theme pads a control.** A wide gap next to a slider is the theme's
`scale { padding }`, not the box's `spacing`. Read the real rule from
libadwaita (`strings libadwaita-1.so | grep -oE "scale[^{]{0,60}\{[^}]{0,200}\}"`)
and override it by the widget's class.

**The overlay scrollbar moves on hover.** On hover libadwaita paints a trough
and widens the slider and its margins, so the bar shifts over buttons. Keep the
same trough margin and slider size with and without `.hovering`; never change
the slider's own margin (libadwaita uses it for the click area). Rule in
`src/app.css`, check `assert_scrollbar_stays_put` in `src/app/components/gtk_tests/shell_layout.rs`.

**Panes libadwaita creates are not in the `.blp`.** `.sidebar-pane` and
`.content-pane` of an `OverlaySplitView` exist only at runtime: find them among
the split's children by class and set their overflow in code.

**The `.blp` default shows in tests.** Code that fills a widget may not run in a
test, so the default from the `.blp` renders. When removing a text or icon,
clear the `.blp` default too, not only the Rust setter.

**`Adw.PreferencesDialog` search still shows hidden groups and rows.**
Libadwaita's built-in Preferences search indexes every preference row (`Adw.ActionRow`,
`Adw.SwitchRow`, `Adw.ComboRow`, `Adw.SpinRow`) attached to a visible
`Adw.PreferencesPage`, even after calling `set_visible(false)` on the group or
the row itself: clicking such a result leads nowhere. Detach inactive
groups/rows with `PreferencesPage::remove` / `PreferencesGroup::remove` while
keeping strong Rust references so they can be re-attached later
(`sync_audio_page_layout` in `src/app/components/pages/settings/settings.rs`).

## 2. Events and signals

**`gdk::Event` is not a GObject.** `and_downcast()` does not compile on it. Use
`controller.current_event().and_then(|e| e.downcast::<gdk::ScrollEvent>().ok())`.
See the volume scroll in `src/app/components/shell/playback/playback_widget.rs`.

**Natural scrolling arrives inverted.** With natural scrolling, fingers going
up send a scroll down, so the volume moved against the hand. Read
`ScrollEvent::relative_direction()`: `ScrollRelativeDirection::Inverted` means
flip the steps (`scroll_steps` in the same file). Some Wayland compositors
(such as Hyprland) always send `Identical` even when natural scrolling is on:
read `ScrollEvent::unit()` on the raw event (not `EventControllerScroll::unit()`,
which reports `Wheel` under `DISCRETE`) to tell touchpad (`Surface`) from mouse
wheel (`Wheel`), and check the compositor's `natural_scroll` option via
`hyprctl -j getoption` when `HYPRLAND_INSTANCE_SIGNATURE` is set
(`is_natural_scroll`).

**A label's `activate-link` opens the browser.** The default handler runs
whenever no handler returned `Propagation::Stop`. A test that emits
`activate-link` must connect its own last handler that records the URI and
stops the emission, or the run opens a real browser. See the card test in
`src/app/components/gtk_tests/library.rs`.

**`win.*` actions are out of reach from outside.** `gapplication action`
reaches only `app.*`, and refuses names with an underscore; call
`org.gtk.Actions.Activate` over D-Bus with `gdbus` instead.

## 3. Rust (the app crate is edition 2018)

**`array.into_iter()` yields references.** You get "mismatched types" far from
the call. Use `.iter().copied()`. `src/main.rs` denies `array_into_iter`.

**`"{x}"` in `assert!` or `panic!` prints literally.** Write `"{}", x`. Denied
by `non_fmt_panics` in `src/main.rs`.

**`TryFrom` is not in the prelude.** `u32::try_from(x)` does not compile in the
app crate. Check the sign and cast, or import `std::convert::TryFrom`.

**A `use` added under `#[cfg(...)]` inherits it.** Inserting a `use` at the top
of a file right below an attribute makes the attribute apply to the new line:
the debug tests build, the release build breaks. Check the line above.

**`Bin::child()` is ambiguous with `ButtonExt::child`.** With the gtk prelude in
scope, the error is "IsA<Button> not implemented for Bin". Call
`libadwaita::prelude::BinExt::child(&bin)`.

**A custom widget without `@implements` does not compile.** `impl WidgetImpl`
fails with E0277 on `IsA<Buildable>`. Declare `@extends gtk::Widget,
@implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget`.

**State enums have no `PartialEq`.** Compare events in tests with `matches!`
(`events.iter().any(|e| matches!(e, ...))`). Do not derive it just for a test.

**`"#text"` closes a raw string.** Last.fm JSON in `r#"..."#` ends early at
`"#`, and the error points elsewhere. Use `r##"..."##`.

**rustfmt on a `mod.rs` formats its submodules.** Files you did not touch show
up modified. Put formatting in its own `style:` commit.

## 4. Translations, icons and settings

**A path-qualified `gettextrs::gettext("...")`.** The local xgettext finds it,
the CI's older one does not, and CI fails with "used but not defined". Import
the function and call it plainly; `scripts/checks/check-translations.sh` bars
the qualified form.

**A new file's strings never reach the `.po`.** List every file with
translatable strings in `po/POTFILES`.

**`update-po` guesses.** Similar strings get a wrong translation marked fuzzy.
Review every fuzzy entry; `scripts/dev/po-set.py` does not fill plural entries
(`msgid_plural`): set `msgstr[n]` by hand.

**An icon that only your desktop has.** It renders empty elsewhere and
`scripts/checks/check-icons.sh` fails. Bundle it in `src/canto.gresource.xml`,
or use a name GTK or libadwaita ships. The gate also fails on a bundled icon
GTK already ships (`go-*`, `pan-*`) and on one nothing uses. Some icon themes
replace generic names (queue, menu), so own icons are named `canto-*`, drawn
with fills only.

**A new GSettings key aborts the app.** Reading a key the compiled schema lacks
panics ("key ... not in schema"). Add it to `data/io.github.gusthanks.Canto.gschema.xml`
and validate with `glib-compile-schemas --strict --dry-run data/`.

**A window title that window-manager rules match got translated.** The rule
stops matching. Keep such titles untranslated (`src/app/components/shell/mini_player.rs`).

## 5. Spotify Web API and playback

**411 Length Required.** PUT and POST without a body (pause, next, seek,
volume) are refused. `LoggingMiddleware` in
`crates/canto-api/src/http/api_client.rs` sends an empty body with
`Content-Length: 0`; new calls go through that client.

**500 on `/me/player/play`.** The generated client turns a `None` body into the
text `null`. Send `StartAUsersPlaybackRequest::new()` (`{}`) to resume;
`give_bodyless_writes_a_length` swaps `null` for an empty body everywhere else.
Fix this kind of error once, in the middleware.

**An empty queue while a song plays.** `GET /me/player/queue` sometimes returns
no `currently_playing`. `get_queue_if_changed` in `src/connect/player.rs` fetches
the track and puts it first.

**The queue endpoint has no history.** It lists only the current and next
songs; replacing Canto's queue with it kills "previous". Keep Canto's own queue
when Canto started the context (`kept_own_queue` in
`src/app/state/playback_state.rs`), and fall back to `RemoteSkip`.

**`NoToken` at startup.** Requests made while the app opens can run before the
login. Retry on an event that only comes after the login, not on a timer, and do
not count the error against rate limits.

**Everything 401 after a suspend.** Tokio timers do not advance during suspend,
so a sleep until the token expires wakes late. Wait on the wall clock in short
steps: `wait_until` in `crates/canto-auth/src/oauth2.rs`. Any long wait for a
time of day follows that pattern.

**Playback state also arrives as `*Synced` events.** A widget that listens only
to `PlaybackResumed`, `PlaybackPaused` and `TrackChanged` stays "paused" after a
device switch or when Canto opens on a playing song. Also handle
`PlayStateSynced`, `TrackSynced` and `SwitchedDevice` (see
`src/app/components/level_bars.rs`).

**A Connect thread that freezes.** Holding a `last_state` read guard (or a
reference taken from it) across `.await`, or into a helper that writes state,
deadlocks every command. Clone the state first (`src/connect/player.rs`).

**Asking moves the queue.** `Queue::advance` changes state even when there is no
next song. Ask with `has_next()` (`crates/canto-core/src/play_queue/queue.rs`).

**A transfer that pauses.** `PUT /me/player` with `play: false` arrives paused,
and a sleeping speaker answers 404 for a few seconds. Pass whether it was
playing, skip the transfer when the device is already active, and never drop
the transfer's result.

**Lyrics words blink.** Remote positions arrive late; ignore jumps under
`POSITION_JITTER_MS` on the same playing track (`src/app/state/playback_state.rs`).

## 6. Lyrics, Genius and Last.fm

**A cached struct gains a field and shows it empty for days.** Add
`#[serde(default)]` and bump the cache key: `CACHE_VERSION` in
`crates/canto-api/src/lyrics/cache.rs`, `genius_song_v3_` in
`crates/canto-api/src/genius/mod.rs`.

**A rejected Genius or Last.fm key looks like a Spotify session problem.** Both
arrive as `DomainError::AuthExpired`, the same error as an expired Spotify
session. Handle it where the call was made (credits card, scrobbler) and tell
the user which key was refused; never let it reach the global handler
(`resolve_api_call` in `src/app/components/mod.rs`), which treats it as
Spotify's. A rejected scrobble stays queued, never dropped.

**Last.fm data is noisy.** A track's `wiki` can be about something else,
`duration` can be `0`, and symbols like `+` come back changed: prefer the
album's wiki and the artist's bio, and match names normalized.

**Folding long text with `set_lines` and ellipsize.** With paragraphs the cut
barely shows and "Show more" looks dead. Fold by whole paragraphs and swap the
markup.

## 7. Git and shell

**Partial commits with `git apply --cached --unidiff-zero`.** Line numbers
shift, a hunk lands inside another function, and `main` stops compiling while
the local build (the whole file) passes. Copy the file aside, write the version
without the extra part in place, check it builds, commit, restore the copy.

**`pkill -f <text>` kills your own shell.** It matches the command line that
runs it. List PIDs in one call, kill by number in the next. The same goes for a
`pgrep -f` wait loop on the same line as the command it waits for: put the
loop in a script.
