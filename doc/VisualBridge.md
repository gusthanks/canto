# Visual Bridge

Visual Bridge is an opt-in local runtime interface in Canto for status bars,
desktop widgets, and external visualizers. It is **off by default** and
completely dormant when disabled (zero timers, zero disk writes, zero lyrics
fetches, and zero audio-thread metering cost).

## Enabling

Open **Preferences -> Integrations -> Visual Bridge** and turn on **Enable
Visual Bridge**.

While enabled, Canto atomically writes the current playback, synced lyric,
theme, and audio-level snapshot to:

```text
$XDG_RUNTIME_DIR/canto/state.json
```

Writes go to a sibling temporary file first and replace the target via rename,
so readers never see a half-written JSON file. Updates run at roughly 15 Hz
while music is playing, immediately on track, play-state, seek, lyric-offset,
or theme changes, and once with `"active": false` when Visual Bridge is turned
off.

## JSON Format (`VisualBridgeState`)

```json
{
  "app": "canto",
  "version": 1,
  "active": true,
  "playing": true,
  "track": {
    "id": "0VjIjW4GlUZAMYd2vXMi3b",
    "title": "Blinding Lights",
    "artists": ["The Weeknd"],
    "album": "After Hours",
    "duration_ms": 200040,
    "progress_ms": 42150,
    "artwork_url": "https://i.scdn.co/image/..."
  },
  "lyric": "And I said, ooh, I'm blinded by the lights",
  "audio": {
    "rms": 0.34,
    "peak": 0.78
  },
  "theme": {
    "kind": "omarchy",
    "is_dark": true,
    "background": "#141414",
    "surface": "#1e1d1a",
    "foreground": "#e6e1d6",
    "accent": "#d4b483",
    "highlight": "#d4b483"
  },
  "updated_at_ms": 1732236000000
}
```

### Fields

| Field | Type | Notes |
| --- | --- | --- |
| `app` | string | Always `"canto"`. |
| `version` | integer | Schema version (`1`). |
| `active` | boolean | `true` while Visual Bridge is enabled; `false` on the final frame when disabled. |
| `playing` | boolean | `true` while audio is playing. |
| `track` | object or `null` | Active track metadata (`id`, `title`, `artists`, `album`, `duration_ms`, `progress_ms`, `artwork_url`), or `null` when stopped/inactive. |
| `lyric` | string or `null` | Currently active synced lyric line (respecting your lyrics sync offset), or `null` when unavailable, unsynced, on an instrumental break, or inactive. |
| `audio` | object | Normalized `rms` and `peak` energy levels (`0.0` to `1.0`) when playing through Canto's built-in engine; `0.0` when paused or playing on an external/remote device. |
| `theme` | object | Active theme snapshot (`kind`, `is_dark`, `background`, `surface`, `foreground`, `accent`, `highlight`). |
| `updated_at_ms` | integer | Unix timestamp in milliseconds when the frame was written. |

Implementation: `src/app/components/visual_bridge.rs`.

## Examples

### Waybar

Add a custom module to your Waybar config (requires `jq`):

```jsonc
"custom/canto": {
  "interval": 1,
  "return-type": "json",
  "exec-if": "test -s \"$XDG_RUNTIME_DIR/canto/state.json\"",
  "exec": "jq -rc 'if .active and .track then {text: (.lyric // (.track.title + \" - \" + (.track.artists | join(\", \")))), tooltip: (.track.title + \"\\n\" + (.track.artists | join(\", \"))), class: (if .playing then \"playing\" else \"paused\" end)} else empty end' \"$XDG_RUNTIME_DIR/canto/state.json\"",
  "max-length": 48
}
```

### Quickshell

Watch `$XDG_RUNTIME_DIR/canto/state.json` reactively with QML and Quickshell IO:

```qml
import QtQuick
import Quickshell
import Quickshell.Io

Text {
    id: root
    property var state: null
    FileView {
        path: Quickshell.env("XDG_RUNTIME_DIR") + "/canto/state.json"
        watchChanges: true
        onFileChanged: reload()
        onLoaded: try { root.state = JSON.parse(text()) } catch (e) {}
    }
    visible: !!(state && state.active && state.track)
    color: state?.theme?.accent ?? "#ffffff"
    text: state?.lyric ?? (state?.track ? `${state.track.title} - ${state.track.artists.join(", ")}` : "")
}
```

### Eww

Poll `$XDG_RUNTIME_DIR/canto/state.json` in `yuck` and style with the active theme accent:

```yuck
(defpoll canto :interval "1s"
  `cat "${XDG_RUNTIME_DIR}/canto/state.json" 2>/dev/null || echo '{"active":false,"track":null,"lyric":null,"theme":{"accent":"#ffffff"}}'`)

(defwidget canto-now []
  (box :visible {canto.active && canto.track != "null"}
       :class "canto-now"
    (label :limit-width 45
           :style "color: ${canto.theme.accent};"
           :text {canto.lyric != "null"
                  ? canto.lyric
                  : (canto.track != "null" ? "${canto.track.title} - ${canto.track.artists[0]}" : "")})))
```
