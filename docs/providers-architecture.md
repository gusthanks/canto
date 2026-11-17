# Music Provider Architecture in Canto

Canto separates UI, queueing, metadata caching, authentication, and audio streaming behind provider-neutral interfaces so additional backends such as YouTube Music can plug in cleanly beside Spotify.

## Layer Responsibilities

1. **Provider Metadata & Capability Contract (`crates/canto-api/src/providers/provider.rs`)**
   - `ProviderKind`: identifies the active catalog backend (`Spotify`, `YouTubeMusic`).
   - `ProviderCapabilities`: advertises feature support (`supports_connect`, `supports_recommendations`, `supports_audio_engine_dsp`, `supports_lyrics`, `supports_search`, `supports_library_sync`).
   - `MusicProvider`: async trait consumed by `ApiService`. It exposes `kind()` and `capabilities()` with backward-compatible defaults alongside catalog, library, playlist, search, and playback transfer methods.
   - `ApiService` (`crates/canto-api/src/service.rs`) handles request coalescing, in-memory LRU caching, disk TTL caching, LRCLIB synced lyrics, Genius credits, and Track Radio synthesis on top of any `MusicProvider`.

2. **Authentication Boundary (`crates/canto-auth`)**
   - Credentials and refresh lifecycles stay independent of catalog queries via `TokenProvider` (`crates/canto-api/src/token.rs`).
   - Spotify uses PKCE OAuth (`http://127.0.0.1:8898/login`) plus librespot session credentials.
   - A YouTube Music provider can implement an independent OAuth device/client-credentials or cookie/InnerTube token provider and expose it through `Arc<dyn TokenProvider>` without modifying `ApiService` or the GTK UI.

3. **Audio Backend & DSP Pipeline (`crates/canto-audio-engine` + `src/player/`)**
   - Audio playback runs through `ProcessorChain` inside `CaptureSink` before writing PCM frames to PulseAudio or ALSA.
   - Built-in DSP stages (`EqProcessor`, `MonoProcessor`, `PanProcessor`, `PitchProcessor`, `MixProcessor`, `LevelMeterProcessor`) operate strictly on `AudioBuffer` (`Vec<f64>` samples, `channels`, `sample_rate`) and know nothing about Spotify or librespot.
   - A YouTube Music audio player only needs to decode streams into PCM frames (`f64` interleaved samples) and feed them through the same `ProcessorChain`, preserving EQ, mono/pan/pitch controls, and the lock-free `LevelMeterController` for external visualizers.

## Adding a YouTube Music Provider

1. Create `crates/canto-api/src/providers/youtube_music/` implementing `MusicProvider` with:
   - `kind() -> ProviderKind::YouTubeMusic`
   - `capabilities() -> ProviderCapabilities::youtube_music()` (`supports_connect: false`, catalog/search/radio/library `true`)
   - Mappings from YouTube Music browse/search/playlist endpoints into `canto-api::models` (`Track`, `Album`, `Artist`, `Playlist`, `SearchResults`).
2. Add a service constructor `youtube_music_service(...) -> ApiService` alongside `spotify_service(...)`.
3. Wrap the stream decoder (e.g., GStreamer or Symphonia/HTTP DASH) in a player service that passes decoded frames through `ProcessorChain` and emits the same `PlaybackAction` / `AppPlayerDelegate` callbacks used by `App`.
4. Gate Connect-only UI affordances (remote device switching, remote scrobble toggle) on `api.provider_capabilities().supports_connect`.
