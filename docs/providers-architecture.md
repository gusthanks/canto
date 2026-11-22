# Music Provider Architecture in Canto

Canto separates UI, queueing, metadata caching, authentication, and audio streaming behind provider-neutral interfaces so Spotify, local libraries, OpenSubsonic / Navidrome servers, and future backends such as YouTube Music run through one shared frontend and DSP pipeline.

## Layer Responsibilities

1. **Provider Metadata & Capability Contract (`crates/canto-api/src/providers/provider.rs`)**
   - `ProviderKind`: identifies the active catalog backend (`Spotify`, `Local`, `Subsonic`, `YouTubeMusic`).
   - `ProviderCapabilities`: advertises feature support (`supports_connect`, `supports_recommendations`, `supports_audio_engine_dsp`, `supports_lyrics`, `supports_search`, `supports_library_sync`).
   - `MusicProvider`: async trait consumed by `ApiService`. Exposes catalog, library, playlist, search, `stream_url`, `rescan`, playback transfer, and `player_add_to_queue` methods with sensible defaults.
   - `ApiService` (`crates/canto-api/src/service.rs`) stores the active provider behind `RwLock`, supports live `set_provider` and `rescan_provider`, handles request coalescing, in-memory LRU caching, `file://` artwork loading, disk TTL caching, LRCLIB synced lyrics, Genius credits, and Track Radio synthesis.

2. **Implemented Providers (`crates/canto-api/src/providers/`)**
   - `SpotifyDomain` (`crates/canto-api/src/providers/spotify/domain.rs`): Spotify Web API catalog, library, Connect playback, and `player_add_to_queue`.
   - `LocalProvider` (`crates/canto-api/src/providers/local/mod.rs`): recursively indexes FLAC, MP3, OGG/OGA, and WAV folders, parses metadata/durations and embedded cover artwork via `symphonia`, and stores likes and local playlists in JSON.
   - `SubsonicProvider` (`crates/canto-api/src/providers/subsonic/mod.rs`): OpenSubsonic / Navidrome REST client using salted MD5 token authentication, catalog browsing, search, playlists, cover art, and authenticated stream URLs.

3. **Authentication Boundary (`crates/canto-auth`)**
   - Credentials and refresh lifecycles stay independent of catalog queries via `TokenProvider` (`crates/canto-api/src/token.rs`).
   - Spotify uses PKCE OAuth (`http://127.0.0.1:8898/login`) plus librespot session credentials.
   - `LocalProvider` works without authentication, and `SubsonicProvider` uses user-configured server credentials (`subsonic-url`, `subsonic-username`, `subsonic-password`).

4. **Audio Backend & DSP Pipeline (`crates/canto-audio-engine` + `src/player/`)**
   - Audio playback runs through `ProcessorChain` inside `CaptureSink` before writing PCM frames to PulseAudio or ALSA.
   - Built-in DSP stages (`EqProcessor`, `MonoProcessor`, `PanProcessor`, `PitchProcessor`, `MixProcessor`, `LevelMeterProcessor`) operate strictly on `AudioBuffer` (`Vec<f64>` samples, `channels`, `sample_rate`) and know nothing about Spotify or librespot.
   - `StreamPlayer` (`src/player/stream_player.rs`) decodes local files and HTTP streams (`symphonia` for FLAC/MP3/OGG/Vorbis plus RIFF/WAVE reader), resamples to stereo 44.1 kHz (`resample_interleaved_to_stereo_44100`), and feeds the same `ProcessorChain` and `CaptureSink`.

## Adding a YouTube Music Provider

1. Create the provider folder (planned: `crates/canto-api/src/providers/youtube_music/`) implementing `MusicProvider` with:
   - `kind() -> ProviderKind::YouTubeMusic`
   - `capabilities() -> ProviderCapabilities::youtube_music()` (`supports_connect: false`, catalog/search/radio/library `true`)
   - Mappings from YouTube Music browse/search/playlist endpoints into `canto-api::models` (`Track`, `Album`, `Artist`, `Playlist`, `SearchResults`).
2. Add a service constructor or switch helper alongside `spotify_service(...)` / `set_provider`.
3. Reuse `StreamPlayer` (passing stream URLs into `Command::PlayerLoadStream`) so PCM frames feed `ProcessorChain`, EQ, crossfade (`MixProcessor`), and `LevelMeterController`.
4. Gate Connect-only UI affordances on `api.provider_capabilities().supports_connect`.
