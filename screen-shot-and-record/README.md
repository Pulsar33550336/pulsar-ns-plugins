# ScreenShot & Record

Region-first screenshots on Wayland: select an area (or hover a window) once, then
copy, edit, OCR, reverse-image-search or record it, all themed from the shell palette.

## Plugin

| Field | Value |
| --- | --- |
| ID | `pulsar/screen-shot-and-record` |
| Entries | Bar widget: `widget`; panel: `panel`; service: `ipc` |

The `ipc` service is the always-live surface for keybinds: panels only exist while
open and a bar entry can have several instances, so a keybind targets the service.

## Requirements

`screenshot-overlay`, `wl-clipboard`, `tesseract`, `curl`, `jq`, `wf-recorder` and,
for the editor hand-off, `swappy`. The plugin reports a missing tool instead of
failing silently.

## Usage

Click the bar icon to open the target list, then pick a target. Or dispatch one
directly from a keybind:

```sh
noctalia msg plugin pulsar/screen-shot-and-record:ipc all screenshot
noctalia msg plugin pulsar/screen-shot-and-record:ipc all ocr
noctalia msg plugin pulsar/screen-shot-and-record:ipc all record
noctalia msg plugin pulsar/screen-shot-and-record:ipc all stoprecord
```

Open the panel directly with:

```sh
noctalia msg panel-toggle pulsar/screen-shot-and-record:panel
```

## Settings

| Setting | Default | Description |
| --- | --- | --- |
| `enable_cross` | `true` | Draw the crosshair in the region overlay. |
| `enable_windows_selection` | `true` | Highlight windows under the pointer. |
| `screenshot_editor` | `swappy` | Editor launched by the edit action. |
| `keep_source_screenshot` | `false` | Keep the pre-edit file next to the edited one. |
| `save_path` | `~/Pictures/Screenshots` | Where saved screenshots go. |
| `recording_save_path` | `~/Videos` | Where recordings go. |
| `png_compression_level` | `1` | PNG compression level, 0-9. |
| `notifications_enabled` | `true` | Show a notification for finished actions. |
| `recording_notifications` | `true` | Show notifications for recording start and stop. |
