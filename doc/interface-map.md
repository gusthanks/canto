# Interface map

Where each part of Canto's window lives in the code, and what was already
decided about it. Read the section of an area before changing it.

- Paths are from the repo root unless a section names its folder. Names are
  widget ids from the `.blp` files, Rust symbols, CSS classes, icons and actions.
  There are no line numbers: search for the name.
- "Decided" items carry the date of the decision. Do not redo them without a
  new decision from the maintainer.
- When a part moves or is renamed, update this file in the same commit.
  `scripts/checks/check-doc-refs.py` (a quality gate) fails when a name in
  backticks here no longer exists in the code.
- Widths are in sp (GTK's text-scaled pixels) unless they say px.

## 1. The window and its three cards

- `src/window.blp` holds the top bar, the toast overlay and two nested
  `Adw.OverlaySplitView`s: `navigation_split` (the library on the start side)
  and `utility_split` (the side panel on the end side), with the player bar
  below.
- The library, the page and the side panel are rounded cards over the bars'
  color (`@headerbar_bg_color`), with an 8 px gap between them. Both split views
  carry `canto-cards`; the rules are at the end of `src/app.css` (margins,
  `border-radius`, `@view_bg_color` on `.sidebar-pane` and `.content-pane`).
- libadwaita creates the panes, so their corners are clipped in code:
  `clip_card_panes` in `src/app/components/shell/layout/mod.rs` sets
  `gtk::Overflow::Hidden` on them.
- The layout code is `src/app/components/shell/layout/`: `mod.rs` decides,
  `fit.rs` holds `utility_mode` and `LibraryFit`, `breakpoints.rs` holds
  `add_window_breakpoints`, `panel.rs` sizes each panel, and `constants.rs`
  holds every width with its reasoning. The layout refits on every width change,
  in the first frame at the new width.
- A new window opens 1200 wide, so the side panel starts beside the page.

### The side panel by window width (`utility_mode`)

| Width | `UtilityMode` | What the side panel does |
| --- | --- | --- |
| 1100 and up (`UTILITY_DOCK_MIN_WIDTH_SP`) | `Beside` | Sits beside the page and takes its width from it |
| 551 to 1099 | `Drawer` | Floats over the dimmed page as a card 360 wide, or leaving 96 of the window uncovered (`UTILITY_DRAWER_PEEK`, `drawer_width`); the split view gets `utility-split--floating` |
| 550 and under (`MOBILE_WIDTH_SP`) | `FullWindow` | Fills the window, flush |

- Narrowing the window with the panel open closes the drawer; widening past
  1100 brings the panel back beside the page.
- The floating panel closes with its button, with Esc from anywhere (a text
  field or an open popover takes Esc first) and with a click on the dimmed
  page. All three close through `set_utility_open(false)`; `hide_utility`
  moves the keyboard focus back to the page widget that had it, with the
  page's scroll-to-focus paused (`focus_page`), so the page never jumps.
- One thing over the page at a time: opening the side panel closes the
  library opened over the page, and the other way round
  (`open_navigation_overlay`, `opening_navigation_overlay`). With the queue
  in its bottom sheet, the sheet and the floating panel close each other
  (`set_sheet_open`, `toggle_view` in `src/app/components/shell/panel_manager/mod.rs`).
- Going to another page while the drawer is open closes it, so the new page
  can take the keyboard focus.
- Tests: `assert_drawer_layout`, `assert_drawer_behavior` and
  `assert_drawer_follows_resizes` in `src/app/components/gtk_tests/shell_layout.rs`.

### The library's rail of covers (`auto_rail`)

- The library collapses to a rail of covers (`NAVIGATION_RAIL_WIDTH`).
  The rail the user chooses is saved (`navigation-rail`, `rail_chosen`); the
  rail the layout picks on its own is not.
- `AutoRail::Narrow`: the window is too narrow for the full library beside
  the page. The library icon on the rail opens the full library over the page
  (`open_navigation_overlay`) without touching the saved choice.
- `AutoRail::BesidePanel`: from 1100 up, with the side panel beside the page,
  a full library that would leave the page card under 656
  (`DOCKED_CONTENT_MIN_WIDTH_SP`) shrinks to its rail. 656 keeps an album's
  header beside its cover and the album column of track rows. The full
  library comes back when the panel closes (after the panel has slid out, so
  the page never squeezes) or when the window grows enough.
- The rail's sidebar carries `navigation-panel--rail`; a row created while
  the rail shows reads the class when it is born. The top bar's margins over
  the rail are properties set in `apply_rail`, not CSS, so the main menu
  button is measured again before it is allocated.
- Tests: `assert_library_rails_beside_panel` (`shell_layout.rs`).

## 2. The top bar

- `Adw.HeaderBar` `top_bar` in `src/window.blp`, above `toast_overlay`, is the
  only bar with window controls; the page bar and the side panel's header set
  `show-start-title-buttons` and `show-end-title-buttons` to false. It has no
  close button either: it expects the window manager's shortcut.
- Order: the main menu (`user`, `canto-more-symbolic`, F10), back, forward,
  home, and the large search in the middle. Back and forward move into
  `top_history` (`set_history_slot`, `place_history_buttons`); home is
  `app.nav_albums`; the search pill is `search_button`, and the search entry
  moves into `top_search_slot` (`set_top_search`).
- Under 764 (`NARROW_WIDTH_SP`) the search becomes a round icon
  (`top_search_icon`, `app.search`) and `top_center` aligns to the start, both
  in the same `narrow` breakpoint of `add_window_breakpoints`. With the full
  library open in a narrow window, home and search end where the library ends
  (`align_top_center_with_sidebar`).
- Own arrows (`canto-back-symbolic`, `canto-forward-symbolic`); top bar and
  player bar icons are 20 px.

## 3. The page bar

- `$AppHeaderBar` sits in an `Overlay` over the `content_stack`
  (`src/app/components/shell/headerbar/`). Album, playlist and artist pages
  call `PageHeader::set_floating(true)` and scroll up under it.
- `apply_floating` keeps the bar see-through (`page-bar--clear`) while the
  cover shows. Once the title shows in the bar it takes the page's background
  with a thin edge (`page-bar--solid`), so the list never crosses the title.
  Other pages get a top margin of the bar's height.
- The page's list starts 1 px under the card's top edge, and the page fills
  that row (the tint while the bar is see-through, nothing once it is solid):
  at scale 1.6 the songs showed through the edge (`keep_top_edge_clean`).
- Tests: `assert_page_bar_covers_the_list` (`shell_layout.rs`),
  `assert_details_page_top_edge_clean` (`library.rs`).

## 4. The player bar

Files: `src/app/components/shell/playback/` (`playback_widget.blp` and `.rs`,
`volume_scroll.rs`, `playback_info.blp`, `playback_controls.rs`, `playback.css`).

- The end of the bar, by default: device, queue, lyrics, volume, mini player,
  More Options (the gear, `eq_button`, `canto-settings-symbolic`) and the side
  panel button (`side_panel_button`, `win.utility-panel`,
  `canto-side-panel-symbolic`). The default order is `PLAYER_BAR_DEFAULT_ORDER`
  in `crates/canto-config/src/settings.rs`, equal to the gschema default of
  `player-bar-order` (a test fails when they differ).
- Customizable: each optional button sits in a slot tied to a key
  (`PLAYER_BAR_*` constants: `player-bar-lyrics`, `player-bar-queue`,
  `player-bar-device`, `player-bar-volume`, `player-bar-mini-player`,
  `player-bar-side-panel`). "Show in the Bar" opens with a right click on
  `bar_end` or from More Options > "Customize Bar…" (`setup_customizable_bar`):
  a check to show each button and arrows to move it. The gear has no key: it
  never leaves, as the way back. A new bar button needs a slot, a gschema key,
  a constant and a row in `items`.
- More Options holds the sleep timer (`more_sleep_timer`, which slides to its
  choices inside the menu), the equalizer (`eq_open_presets`), the audio
  settings (`eq_open_settings`, `win.preferences-audio`) and "Customize
  Bar…". The round timer button shows only while a timer counts down. The
  equalizer acts only on the built-in player (librespot), so `eq_open_presets`
  is hidden (`set_equalizer_available`, `equalizer_available`) whenever
  playback uses Soloist, the hidden Spotify app, or a remote Spotify Connect
  device.
- Device (`device_selector`, `src/app/components/shell/device_selector/`): an
  icon only, the name in the tooltip. `device-selector--away` paints it in the
  accent color when the sound plays on another device. `device_icon` picks
  `canto-device-{speaker,computer,phone,tv}-symbolic` (our own drawings, no
  brand shapes), on the button and beside each device in the list.

### Tiers by width (`setup_bar_tiers`)

The bar stays on one line at every width, and the title and artists keep about
100 sp (`width-chars` 10 and 12 in `playback_info.blp`). As the window narrows
it drops buttons in steps (`BAR_TIER_WIDTHS`):

| Width and under | What changes |
| --- | --- |
| 860 | The volume slider becomes `volume_button`; device and mini player leave (both stay in Ctrl+K) |
| 660 | Shuffle and repeat move into More Options (`more_shuffle`, `more_repeat`); shorter times |
| 560 | Lyrics and queue leave (the side panel has them as tabs) |
| 486 | Like (`like_slot`) and the volume button leave |
| 400 | Previous and the times leave |

- `bar_tier_condition` is "max-width: N px or N sp": icons keep their px at
  any text scale, so larger text widens the steps. `retune_bar_tiers` adds the
  room of a running countdown, fixed while it runs, so a ticking digit never
  flips a tier (which would close an open menu).
- Each breakpoint carries the setters of the tiers above it, since libadwaita
  applies only the last match. Tiers change the parts inside the slots, never
  a slot: `follow_slot` shows a slot while its key is on and its child is
  visible.
- Tracks of an hour or more read `1h02` in a tight bar instead of pushing it
  wider.
- `MiniPlayer` (`src/app/components/shell/mini_player.rs`): shows a 2 px
  seekable progress bar along the bottom edge, natural-scroll volume
  (`scroll_steps_from_event`, `volume_after_scroll`), and an optional 1-line
  live synced lyric strip controlled by `mini-player-lyrics` and refreshed on
  `AppEvent::LyricsUpdated`.
- Tests: `assert_bar_tiers` and `assert_more_options_rows`
  (`src/app/components/gtk_tests/playback_bar.rs`).

### Volume

- `volume_box` (`width-request` 168, `spacing` 0) with `.volume-slider`
  (`padding-left: 6px` in `playback.css`, since the theme pads the slider 12
  px on each side).
- The wheel or the touchpad over the speaker, the slider, `volume_button` or
  its popover changes the volume 5% a step, the step of Ctrl+Up and Ctrl+Down
  (`VOLUME_STEP`). Capture-phase `gtk::EventControllerScroll`s sit on all three
  before the sliders' own scrolling; `volume_after_scroll` follows the hand
  with natural scrolling. The tooltip says the level and the keys
  (`volume_tooltip`).
- Test: `assert_volume_scrolls_in_steps` (`playback_bar.rs`).

## 5. The side panel

Files: `src/app/components/shell/utility_panel/` (tabs and frame) and
`src/app/components/shell/now_panel/` (Now Playing, About, Stats).

- Tabs, icons only with the name in the tooltip: Now Playing, Queue, Lyrics,
  About, Stats (`now_tab`, `queue_tab`, `lyrics_tab`, `about_tab`,
  `stats_tab`) in `switcher_box` (`linked`, `utility-panel__tabs`) of
  `utility_panel.blp`.
- The Now Playing tab shows playing bars (`now_indicator`, `.playback-indicator`);
  `utility-panel__now-indicator--idle` holds them still while paused
  (`UtilityPanel::set_now_playing`, called by the `PanelManager`).
- Lyrics is a quote bubble, `canto-lyrics-symbolic`, the same in the player
  bar and Ctrl+K.
- It opens and closes with the player bar's side panel button and, while
  open, with `close_panel` in its header.

### Now Playing cards (`now_panel/mod.rs`)

- Cards: Lyrics, Next in Queue, Your Plays, Credits. A card is a summary that
  opens the full view (`make_card_activate`): Lyrics `win.show-lyrics`, Next in
  Queue `win.show-queue`, Your Plays `win.show-stats` (the Stats tab), Credits
  `win.show-about` (the About tab). A new tab on the subject of a card follows
  the same pattern, without repeating its content.
- Credits shows only performers, producers and writers.

### About and Stats tabs (`now_panel/about.rs`, `stats.rs`, `info_page.rs`)

- About comes from Genius (`canto-genius-symbolic`, a filled 4-point star).
  Stats come from Last.fm (`canto-stats-symbolic`, a rising line, unlike the
  playing bars).
- They load only while the tab is open; each section hides without data;
  without a key or a page, a short message with a button.
- Shared pieces in `info_page.rs`: `action_row` (a row with a position, a
  name, a detail and any click), `flow` and `chips` (a `libadwaita::WrapBox`,
  which wraps like text; a `gtk::FlowBox` aligns columns and leaves gaps).
- A song row plays it (`open_pick` in `stats.rs`): a scoped search, then
  `best_match`, then it plays like `play_track` in
  `src/app/components/shell/navigation_panel/model.rs`. An artist opens its
  page; nothing found opens the search.
- "See Your Top 10" in the Stats tab opens the listening stats dialog
  (`win.listening-stats`, section 9).
- Test: `assert_about_and_stats_tabs_fill_every_card`
  (`src/app/components/gtk_tests/now_panel.rs`).

### Queue and Recently Played (`src/app/components/pages/queue/`)

- Recently Played folds above the queue (`recent_section`, `recent_toggle`):
  the last 50 songs read like a timeline, oldest on top and the newest next to
  Now Playing; it opens scrolled to its end and follows it until a scroll by
  hand. Unfolded it shows three songs (`RECENT_VISIBLE_ROWS`,
  `fit_recent_rows`), measured from the rows. Each song has a heart.
- `save_playlist_button` (compact icon-only button in `recent_header`, beside
  `recent_chevron_button`, handled by `save_queue_as_playlist`) saves the
  playable queue tracks into a new playlist through
  `PlaylistDialog::for_saving_queue`. Movable rows expose Move Up and Move Down
  in the row menu (`menu_opening_for` enables each direction lazily on
  `Device::Local`). Queueing while on `Device::Connect` emits
  `PlaybackEvent::TracksQueuedRemote`, forwarded via
  `ConnectCommand::AddToQueue` to `player_add_to_queue`. Recently Played tracks
  finished or skipped songs immediately in session memory and merges them via
  `merge_recent_tracks` across Connect `TrackSynced`, Local, and Subsonic
  sources.
- Tests: `assert_recently_played`, `assert_recently_played_opens_at_newest` and
  `assert_save_queue_as_playlist_button` (`src/app/components/gtk_tests/queue.rs`).

## 6. Lyrics (`src/app/components/pages/lyrics/`)

- The lyrics bar (`lyrics_page.blp`): the translate capsule `translate_group`
  (`linked`, `lyrics-translate-group`: translate, then the language arrow),
  `sync_button` (a stopwatch, `canto-lyrics-sync-symbolic`, with a popover of
  -100 / 0 / +100 ms; any shift lights it with `lyric-sync-button--shifted`),
  reload and full screen.
- One quiet line above the lyrics says where they came from (`lyrics_source`
  inside `lyrics_source_row`, `lyrics-source`), in the panel and in full
  screen. LRCLIB comes first; Genius only when LRCLIB has none, matched
  strictly on title and artist. When the shown lyrics are not synced,
  `sync_plain_lyrics_button` sits beside the source label and opens
  `add_lyrics::open_for_sync` pre-filled with the current lines.
- With no lyrics from any source, under "No Lyrics Available":
  - `web_search_button` "Search the Web" opens a web search for the main
    artist, the title without edition notes and the word for lyrics in the
    app's language (`web_lyrics_search_url`).
  - `add_lyrics_button` "Add Lyrics" opens `add_lyrics.rs` (`open_with_playback`,
    `SyncPlayback`): a Write / Paste editor and the 1-button "Sync While
    Listening" studio (`SyncSession`, `SyncLine`; `Space` stamps without
    bubbling to global play/pause, `Backspace` undoes, `Left` rewinds 3 s),
    both feeding `parse_draft` (`LyricsDraft`) before the confirmation dialog
    and LRCLIB proof-of-work publish (`crates/canto-api/src/lyrics/publish.rs`,
    `publish_lyrics`). Nothing is sent without that confirmation.
- Full screen (`src/app/components/shell/fullscreen_lyrics.rs`): a sharp cover
  on the left with title and artist under it, the lyrics on the right, and a
  solid background from the cover's color
  (`extract_cover_accent_from_texture`) mixed with near black.
- Tests: `assert_lyrics_options`, `assert_lyrics_source_line`,
  `assert_add_lyrics_window` and `assert_lyrics_sync_studio`
  (`src/app/components/gtk_tests/lyrics.rs`).

## 7. Your Library (`src/app/components/shell/navigation_panel/`)

- The list is rebuilt whole in `apply_navigation_panel_items`: Liked Songs,
  pinned items, playlists, albums and artists, each with a type subtitle.
  Filter pills and "+" come from `add_library_controls`; refills group into
  one every 250 ms; the item playing is highlighted (`is_playing_from`).
- `library_tools` under the filters: search by name or owner, and "Sort by"
  (Recents, Recently Added, Alphabetical, Creator; `library-sort`). Liked
  Songs is always first, above the pins and whatever the sort (a search
  still hides it when it doesn't match; `arrange_keeping_pins`). Pinned
  items stay on top except while searching or filtering. Sorting and search
  happen only in `arrange`.
- "View as" List or Grid at the end of the sort popover (`library-view`,
  `LibraryView`). The grid is a `FlowBox` beside the `ListBox` in the same box
  (`hold_list_and_grid`), over a model without section titles
  (`covers_only`); the tile playing takes the accent (`set_tile_playing` from
  `mark_playing`). The rail ignores the choice and shows covers (`apply_view`).
- On the rail a double click plays from the start (`play_destination`); one
  click opens the page.
- Recents follow Spotify's history (`get_recent_contexts`,
  `recent_context_key`), at most every 3 minutes, and what starts playing
  moves up at once (`bump_playing_to_recents`). The first request goes out
  before login and gets `NoToken`, so it is asked again when the library loads.
- Tests: `assert_sidebar_grid`, `assert_library_keeps_rows_on_play`
  (`src/app/components/gtk_tests/library.rs`).

## 8. Pages, headers and cards

- Album and playlist header (`src/app/components/widgets/details_page/header.blp`):
  rounding on the `image_clip` overlay, shadow on `image_box`, nothing clipped
  around them; the header keeps its cover above the text until cover and
  buttons fit side by side. Clicking a loaded cover
  (`.details-header__image-box--clickable`) opens `present_fullscreen_artwork`
  (`ArtworkViewerSource` in
  `src/app/components/widgets/details_page/artwork_viewer.rs`) with high-res
  artwork over a dark tinted backdrop, closed with `Esc`, backdrop click, or the
  close button.
- An artist's releases are filtered by buttons under "Releases"
  (`FilterChips` in `src/app/components/widgets/card_list/filter_chips.rs`,
  pills with `library-filter` in a `filter-chips` row, built by
  `create_embedded_card_list`). Clicking the active one shows everything; a
  button shows only while its kind is in the list (`categories_in`), and the
  row only with two kinds or more. The view menu keeps size and order only.
- The card grid aligns only visible cards (`align_cells` in
  `src/app/components/widgets/card_list/card_grid.rs`), so a filter never
  piles cards up.
- Under an album's or a song's cover, each artist's name is a link to the
  artist's page: `artist_links_markup` fills the card's `subtitle-markup`, and
  the card's label opens it through `app.view_artist`; other links go to the
  label. The names read dim, like the player bar's (`track-links`).
- Track lists have no lines between songs: continuous single-card track lists
  paint `@card_bg_color` once on `.playlist--single-card` (`sync_single_card_class`
  in `track_list.rs`) to prevent fractional display-scale subpixel seams.
- `create_track_list` in `src/app/components/widgets/details_page/component.rs`
  adds an inline filter toggle, `.details-section__filter-entry` (focused via
  map-scoped `<Ctrl>F` when no modal dialog is open) and `.details-section__sort`
  (`TrackSortOrder`) controlled through `TrackListController`.
- Row context menus are grouped into four logical sections (playback/queue,
  library, explore, utility actions) built in `track_list.rs` and rendered with
  16 px symbolic icons plus an inline `PlaylistPicker` stack page by
  `build_song_menu_popover` (`src/app/components/widgets/track_list/song_menu.rs`).
  The Track Details item opens `present_track_details`
  (`src/app/components/widgets/track_list/track_details.rs`) for any track.
- Tests: `assert_release_filters_are_buttons`,
  `assert_filtered_card_grid_keeps_its_columns`, `assert_card_artist_links`,
  `assert_no_lines_between_songs`, `assert_album_page_fits`,
  `assert_track_list_filter_and_sort`, and
  `assert_track_menu_sections_icons_and_details_dialog` (`library.rs`).

## 9. Ctrl+K and the stats dialog

- Ctrl+K (`src/app/components/shell/command_palette/widget.rs`) is an
  `Adw.Dialog`. On any typed query, `build_library_entries` searches saved
  playlists, albums and artists immediately from `HomeState`, while
  `merge_palette_results` deduplicates matching remote catalog results (`PlayTrack`,
  `ViewAlbum`, `ViewArtist`, `ViewPlaylist`). Statistics actions distinguish
  "Current Song Stats" (`PaletteAction::ShowStats`) from "Your Top Stats"
  (`PaletteAction::ListeningStats`). Spotify results show their cover
  (`PaletteEntry`, round for artists). A click outside closes it (a capture
  `GestureClick` on the window, removed on `closed`). A new action shortcut: a
  `PaletteAction` variant, its arm calling `activate_action("win.<x>")`, and a
  `PaletteEntry` with keywords.
- Listening stats (`src/app/components/shell/stats_dialog.rs`,
  `win.listening-stats`, registered in `src/app/mod.rs`), from the main menu
  ("Your Stats"), Ctrl+K and the Stats tab. Periods 7 days, month, year, all
  time; totals and top 10 artists, songs and albums
  (`LastFmClient::listening_stats`). Last.fm has only album art: artist and
  song images come from a Spotify search, which also decides the click.

## 10. Theme, preferences, login and small things

- Preferences (`src/app/components/pages/settings/settings.blp`, `settings.rs`,
  `settings_model.rs`) has built-in search enabled and is split into three
  pages: `general_page` (Appearance, Music Source switcher in
  `music_source_group` / `music_source_combo` for `music-source`,
  `local-music-folders`, `subsonic-url`, `subsonic-username`, and
  `subsonic-password`, Playback, Caching, Experimental Features), `audio_page`
  (`win.preferences-audio`, holding `soloist_group`, `crossfade_seconds` for
  `crossfade-seconds` via `MixController`, plus the librespot/audio engine
  normalisation, equalizer, pan, pitch and network controls; Spotify-only sections
  hide on Local/Subsonic), and `integrations_page`
  (`win.preferences-integrations`).
- Soloist onboarding: when an account needs Soloist and has no API key saved,
  `soloist_banner_revealer` under `top_bar` in `src/window.blp` shows a
  dismissible contextual banner (`should_show_soloist_banner` in
  `src/app/components/shell/window/mod.rs`) whose action (`win.soloist-guide`)
  opens `present_soloist_guide_dialog`. On non-x86_64 architectures,
  `status_text` in `src/connect/soloist.rs` notes that automatic downloads are
  unavailable.
- Login (`src/app/components/pages/login/login.blp`, `login.rs`,
  `login_model.rs`) includes a collapsed-by-default advanced option
  (`custom_client_id_toggle`, `custom_client_id_entry`) backed by
  `spotify-client-id` and overridden by `CANTO_SPOTIFY_CLIENT_ID`
  (`resolve_client_id` in `crates/canto-auth/src/oauth2.rs`), as well as
  login-free entry buttons for local music folders (`use_local_folder_button`)
  and OpenSubsonic / Navidrome (`connect_subsonic_toggle`,
  `subsonic_connect_button`).
- "Follow System" (the first option of `app_theme` in
  `src/app/components/pages/settings/settings.blp`) is `ThemeKind::Omarchy` in
  `src/app/theme.rs`: `resolve_system_palette` checks `SystemThemePaths` in
  order (Omarchy via `omarchy_colors_path` / `parse_omarchy_colors_toml`,
  matugen via `parse_matugen_colors_json`, pywal via `parse_pywal_colors_json`,
  and wallust via `parse_wallust_colors_json`), watching present palette
  directories and their immediate parents for live creation, move, or
  replacement updates, and falls back to the native system theme via
  `AdwStyleManager` (`None`, respecting system light/dark preference and
  desktop accent color) rather than a fixed preset. The other themes are fixed
  palettes in the same file.
- `adaptive_cover_colors` (`adaptive-cover-colors`) tints Now Playing and
  Lyrics surfaces from the playing album art; the dependent
  `adaptive_cover_accent` switch (`adaptive-cover-accent`) optionally applies
  a WCAG contrast-safe cover accent (`compute_accessible_cover_accent`,
  `build_theme_css_with_options`) to UI accent tokens on both dark and light
  themes.
- Album, playlist and artist pages get a gradient of the cover's color
  behind their header (`set_tint`, `src/app/components/widgets/details_page/tint.rs`).
  `readable_tint` darkens it in dark themes and lightens it in light ones
  until the title keeps 3:1 contrast over its top (a white cover made the
  title vanish); the tints are rewritten when the theme turns light or dark.
- `VisualBridge` (`src/app/components/visual_bridge.rs`, enabled via
  `visual_bridge_switch` / `visual-bridge-enabled`) publishes
  `VisualBridgeState` including the live synced lyric line
  (`active_synced_lyric`, refreshed on `AppEvent::LyricsUpdated`) to
  `$XDG_RUNTIME_DIR/canto/state.json`, writes a final inactive frame on disable
  or `Drop`, and stays dormant when off (see `doc/VisualBridge.md`).
- Scrollbars: GTK thickens the overlay scrollbar on hover and draws a dark
  trough over the content. A global rule in `src/app.css`
  (`scrollbar.overlay-indicator`) keeps it thin and transparent; widgets at
  the row's edge keep a margin from it. Test: `assert_scrollbar_stays_put`.
- The playing bars are drawn icons (`playback-0-symbolic` to
  `playback-16-symbolic`, plus `playback-paused-symbolic`) behind
  `.playback-indicator`; they move with the sound only when it plays through
  this computer.
- Spotify links: a copied `open.spotify.com` link is offered when the window
  gains focus (`ClipboardImport`, `src/app/components/shell/clipboard_import/mod.rs`).
  Links from outside (the desktop's `spotify:` handler in `src/main.rs`,
  MPRIS `OpenUri` in `src/dbus/mpris.rs`) send `AppAction::OpenLink`; the
  state answers `LinkOpenRequested` and `ClipboardImport` opens it the same
  way: albums, artists and playlists open their page, a song opens its album.

## 11. Decided, do not redo

- 2026-11-15: the library collapses to a rail of covers, remembered across
  restarts.
- 2026-11-16: no "Expand library" button. The home button already opens the
  library as a grid (`Route::Home`, the `saved_*` pages).
- 2026-11-17: two pillars until the first release: colors follow the system,
  and a player bar each user arranges. A new feature passes one filter: does it
  serve people who want everything in one place, their way?
- 2026-11-17: no Spotify branding: the green theme is called "Green", icons
  are our own.
- 2026-11-17: no handle on the page's right edge to open the side panel (it
  was removed: the button at the end of the player bar is the way in). The
  bar's "⋯" became a gear, so the window never shows two "⋯".
- 2026-11-17: full screen lyrics on a solid color from the cover, never a
  small cover blown up and blurred.
- 2026-11-18: side panel tabs as icons, the name in the tooltip.
- 2026-11-19: 8 px gap around the cards. Recently Played with the newest song
  at the bottom, next to Now Playing.
- 2026-11-20: under 1100 the side panel floats as a drawer; beside the panel
  the library shrinks to its rail under a 656 page.
- 2026-11-20: the player bar stays on one line, title readable; shuffle and
  repeat move into More Options when narrow.
- 2026-11-20: tab order Now Playing, Queue, Lyrics, About, Stats; Recently
  Played shows three songs.
- 2026-11-20: volume by wheel and touchpad in 5% steps, no fixed +/- buttons.
- 2026-11-20: lyrics with no source: "Search the Web", then "Add Lyrics" to
  LRCLIB with a confirmation. No lyrics from Spotify through librespot (a risk
  for the account).
- 2026-11-21: release filters as buttons under "Releases"; Compilations shows
  only when the artist has one.
- 2026-11-21: the GTK tests fail on any GTK layout warning ("exceeds",
  a negative allocation, a critical).
