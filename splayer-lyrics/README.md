# SPlayer-Next Lyrics

Show the current SPlayer-Next lyric in the Noctalia 5 bar. The bridge is started by the plugin and needs no systemd service.

![Bar widget](docs/bar.png)

## Plugin

| Field | Value |
| --- | --- |
| ID | `pulsar/splayer-lyrics` |
| Entries | Bar widget: `widget`; service: `bridge` |

## Requirements

- Noctalia 5 (tested with 5.4.1).
- SPlayer-Next desktop (tested with 1.1.0) and the companion `splayer-plugin/splayer-noctalia-lyrics.js`.
- `python3` (3.9+, standard library only) on Noctalia's `PATH`; runs `bridge.py` and the `/quit` helper.
- `setsid` (util-linux) starts the bridge in its own session.
- `nohup` (coreutils) ignores SIGHUP.

The companion SPlayer-Next plugin must declare `@type control`; without it SPlayer-Next 1.1.0 sends heartbeats but no playback events. The web player at splayer-next.imsyy.top has no plugin host and cannot import the file.

## Usage

1. Install the plugin. For a manual install, copy `plugin.toml`, `bridge.py`, `bridge.luau`, `widget.luau`, and `translations/` into `~/.local/share/noctalia/plugins/splayer-lyrics/`, then enable it:

   ```bash
   noctalia msg plugins enable pulsar/splayer-lyrics
   ```

2. In SPlayer-Next, open **Settings → Plugin Management → Local import** and choose `splayer-plugin/splayer-noctalia-lyrics.js`. The plugin should be **Ready**; keep the default port `33550`.
3. Add the **SPlayer-Next Lyrics** widget to a bar in Noctalia under **Settings → Bar**.

Enabling the plugin starts the `bridge` service; no systemd unit is installed. The widget is display-only: it adds no tooltip and handles no clicks. Left click is free for the user's own binding; middle click opens the widget settings (built-in Noctalia behaviour).

Verify:

```bash
curl -s http://127.0.0.1:33550/health
curl -s http://127.0.0.1:33550/state
```

## Settings

The main/secondary line choice happens in Noctalia; the SPlayer-Next plugin sends the current line, translation, background, duet, and next-line candidates. When the current line is a production credit, the widget falls back to the previous real lyric and keeps its translation; credits never become the secondary line.

Noctalia plugin settings:

| Setting | Type | Default | Description |
| --- | --- | --- | --- |
| `host` | string | `127.0.0.1` | Bridge listen address; do not change to `0.0.0.0`. |
| `port` | int | `33550` | Bridge port; must match the SPlayer-Next plugin. |

Noctalia widget `widget`:

| Setting | Type | Default | Description |
| --- | --- | --- | --- |
| `prefix` | string | empty | Fixed text before the lyric. |
| `max_chars` | int | `40` | Maximum characters; 0 means no limit. |
| `poll_ms` | int | `300` | Poll interval for `/state`, in milliseconds. |
| `extra_mode` | select | `background-duet` | Secondary-line mode: `none`, `background-duet`, `duet-translation`, `translation-fallback-next`, `translation`, or `next`. |
| `hide_when_idle` | bool | `true` | Hide while offline, paused, or without lyrics. |
| `show_when_paused` | bool | `false` | Keep the last line while paused. |
| `show_title_when_no_lyric` | bool | `true` | Show the track title when no lyric is available. |

`extra_mode` values:

- `none`: no secondary line.
- `background-duet`: background/duet first, then translation, then the next line.
- `duet-translation`: for a duet line, use the duet line as the main line and its translation as the secondary line; other lines use translation, falling back to the next line.
- `translation-fallback-next`: translation, falling back to the next line.
- `translation`: translation only.
- `next`: next line only.

Companion SPlayer-Next plugin:

| Setting | Type | Default | Description |
| --- | --- | --- | --- |
| `host` | text | `127.0.0.1` | Bridge address. |
| `port` | number | `33550` | Bridge port. |
| `filterCredits` | switch | `true` | Filter production-credit lines. |
| `creditKeywords` | text | built-in list | Credit keywords, separated by commas, semicolons, or newlines. |
| `filterTrackNames` | switch | `true` | Filter short lines containing the artist or album name. |
| `heartbeatSeconds` | number | `5` | Heartbeat interval in seconds. |

## IPC

```bash
noctalia msg plugin pulsar/splayer-lyrics:bridge all restart
noctalia msg plugin pulsar/splayer-lyrics:bridge all status
noctalia msg plugin pulsar/splayer-lyrics:widget focused status
```

`restart` stops and relaunches the bridge; `status` prints the bridge path, host/port, and owner pid; the widget `status` prints the latest `/state`.

## Notes

- The bridge listens only on `127.0.0.1:33550` and has no authentication; any local process can read or write the lyric state. Do not change `host` to `0.0.0.0`.
- The plugin spawns `python3`, `setsid`, and `nohup`; the bridge writes no files and keeps no history. It stops when the plugin is disabled or Noctalia exits.
- No external network requests are made; the SPlayer-Next plugin only POSTs to the local bridge.
- Internal design, protocol, and lifecycle: [docs/DESIGN.md](docs/DESIGN.md); full configuration: [docs/CONFIGURATION.md](docs/CONFIGURATION.md); troubleshooting: [docs/TROUBLESHOOTING.md](docs/TROUBLESHOOTING.md).
