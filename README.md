<p align="center">
  <img src="data/icons/canto.png" width="128" height="128" alt="Canto icon">
</p>

<h1 align="center">Canto</h1>

<p align="center">
  Canto is a minimal GTK4 and libadwaita music client that puts your library, synced lyrics, and listening flow front and center without visual clutter.
</p>

<p align="center">
  <sub>Canto is a fork of <a href="https://github.com/Diegovsky/riff">Riff</a> by Diego Augusto and contributors (descended from Spot by Alexandre Trendel), released under the MIT license.</sub>
</p>

---

> [!NOTE]
> Requires a Spotify Premium account for playback. Some accounts may encounter Spotify's proprietary PlayPlay DRM restriction on local playback; Spotify Connect control remains available.

## Features

- **Minimalist Native Interface:** GTK4 and libadwaita layout with responsive side/bottom utility panels, live color themes, and optional subtle background tinting sampled from album artwork.
- **Omarchy Live Theme Integration:** Automatically watches the active Omarchy desktop theme palette and applies changes live without restarting. Also includes curated built-in themes: Neutral (Adwaita), Spotify Green, OLED Dark, Warm Vinyl, and Purple Night.
- **Built-in DSP Engine:** Gapless playback with a 10-band equalizer (plus quick presets), stereo balance/pan, mono fold-down, and pitch shift.
- **Synced Lyrics & Liner Notes:** Real-time synced lyrics via LRCLIB with clickable line seeking and `-100 ms` / `+100 ms` live offset adjustment. Includes **Genius** integration with a direct track search fallback and an optional **About Track** view (producers, writers, featured artists, sample connections, and liner notes).
- **`Ctrl+K` Command Palette:** Keyboard-first quick launcher for navigation, theme switching, Spotify Connect device transfer, Sleep Timer presets, Track Radio, and debounced catalog search.
- **Resilient Track Radio & Smart Queue Continuation:** Launches queues of similar tracks from any track menu, the playback bar, or `Ctrl+K`, with automatic top-tracks and catalog search fallback if `/v1/recommendations` is restricted for newer Spotify apps.
- **Scrobbling (Last.fm & ListenBrainz):** Tracks active listening time (immune to pauses and Spotify Connect poll ticks) with Now Playing updates and a bounded 500-item persistent offline retry queue.
- **Sleep Timer:** Discreet timer in the player bar (`Off`, `End of current track`, `15`, `30`, `45`, or `60 min`) that cleanly pauses local or Spotify Connect playback.
- **Opt-in Visual & Audio Bridge:** Optional lock-free RMS/peak meter and atomic `$XDG_RUNTIME_DIR/canto/state.json` output for external visualizers such as Chaosphere (strictly zero runtime overhead when disabled).
- **Modular Provider Architecture:** Decoupled `MusicProvider` capability abstractions ready for additional backends (see [docs/providers-architecture.md](docs/providers-architecture.md)).

## Keyboard Shortcuts

| Shortcut | Action |
| --- | --- |
| `Ctrl+K` | Open Command Palette |
| `Ctrl+F` | Search catalog |
| `Ctrl+,` | Open Preferences |
| `Ctrl+?` | Show Keyboard Shortcuts |
| `Space` | Play / Pause |
| `Ctrl+[` / `Ctrl+]` | Previous / Next track |
| `Ctrl+Left` / `Ctrl+Right` | Seek backward / forward 5s |
| `Ctrl+Up` / `Ctrl+Down` | Volume up / down |
| `Ctrl+M` | Mute / Unmute |
| `Ctrl+S` / `Ctrl+R` | Toggle Shuffle / Repeat |
| `Alt+1` .. `Alt+5` | Switch home views |
| `F9` | Toggle sidebar |
| `Ctrl+Shift+D` | Toggle Developer Tools menu (debug builds) |

## Hyprland Tip: Silent Spotify Connect Background Mode

If you want Canto running silently at login as a ready Spotify Connect receiver on Hyprland without interrupting your current workspace, assign it to a special workspace silently:

```conf
# ~/.config/hypr/hyprland.conf
windowrule = workspace special:canto silent, match:class ^(dev\.diegovsky\.Riff(\.Devel)?)$
exec-once = [workspace special:canto silent] canto
```

Toggle the scratchpad whenever you want to bring Canto forward (for example `bind = SUPER, M, togglespecialworkspace, canto`).

## BYOK API Setup Tutorials

Canto works out of the box for Spotify browsing, LRCLIB synced lyrics, and zero-key Genius web search links. All extra integrations below are Bring-Your-Own-Key (BYOK): tokens stay local in your GSettings / keyring and are never sent anywhere except their respective official APIs.

### 1. Spotify OAuth & Connect

Canto uses Spotify's OAuth 2.0 PKCE flow (`crates/riff-auth/src/oauth2.rs`) and listens on the exact local redirect URI:

```text
http://127.0.0.1:8898/login
```

Default builds ship with an OAuth Client ID (`782ae96ea60f4cdf986a766049607005`) pre-configured. If you are building Canto with your own Spotify Developer application:

1. Open the [Spotify Developer Dashboard](https://developer.spotify.com/dashboard) and click **Create app**.
2. Under **Redirect URIs**, add `http://127.0.0.1:8898/login` and select **Web API**.
3. Copy your **Client ID** into `CLIENT_ID` in `crates/riff-auth/src/oauth2.rs` and rebuild.
4. Launch Canto and click **Log in with Spotify**. Approve in your browser and it will redirect back to `127.0.0.1:8898/login` automatically.

Note: Spotify restricted `/v1/recommendations` and `/v1/artists/{id}/related-artists` for many newer Client IDs starting in November 2024. Canto detects 403/404 responses automatically and builds Track Radio from artist top tracks and catalog searches with deduplication.

### 2. Genius (Credits, Samples & Liner Notes)

Even without an API token, clicking the **Genius** pill in the Lyrics panel cleans edition tags (such as `(Remastered)` or `[Live]`) and opens a clean web search for the playing track.

To display producers, writers, featured artists, release dates, sample relationships, and liner notes directly inside **About Track**:

1. Visit [genius.com/api-clients](https://genius.com/api-clients) and sign in.
2. Click **New API Client**, enter an app name (for example `Canto`) and any local App Website / Redirect URI (such as `http://localhost`), then save.
3. Click **Generate Access Token**, copy the **Client Access Token**, and paste it into **Preferences -> Integrations -> Genius**.

Results are cached locally on disk so revisiting a track requires zero repeated API calls.

### 3. Last.fm (Scrobbling)

1. Open [last.fm/api/account/create](https://www.last.fm/api/account/create) and create an API application (a callback URL is optional for desktop auth).
2. Copy your **API Key** and **Shared Secret** into **Preferences -> Integrations -> Scrobbling**.
3. Click **1. Open Auth** to open Last.fm in your browser and approve Canto, then click **2. Complete** back in Canto to save your persistent session key.
4. Turn on **Enable Scrobbling** (and optionally **Scrobble Connect Devices** if you also want remote Connect speakers/phones tracked).

Canto tracks active listening duration only (ignoring pauses and duplicate Connect poll ticks) and scrobbles after `min(duration / 2, 240s)` for tracks at least 30 seconds long. Offline plays are queued locally (up to 500 entries) and retried automatically.

### 4. ListenBrainz (Scrobbling)

1. Open [listenbrainz.org/settings](https://listenbrainz.org/settings/) while signed in.
2. Copy your **User Token** and paste it into **Preferences -> Integrations -> ListenBrainz User Token**.
3. Turn on **Enable Scrobbling**.

You can enable Last.fm, ListenBrainz, or both simultaneously.

## Building from Source

```sh
./scripts/setup-dev.sh    # install system build dependencies
./scripts/build.sh dev    # compile debug build with Meson + Cargo
./scripts/build.sh test   # run workspace tests and Meson checks
```

For production installation into `~/.local`:

```sh
./scripts/build.sh release --install
```

See the [Development Guide](doc/Development.md) for architecture notes, feature flags, and developer tools (`Ctrl+Shift+D`).

## Code of Conduct

This project follows the [GNOME Code of Conduct](https://conduct.gnome.org).
