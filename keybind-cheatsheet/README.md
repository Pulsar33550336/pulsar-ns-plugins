# Keybind Cheatsheet for Noctalia

Hyprland keybind cheatsheet plugin for Noctalia that reads keybindings from `hyprctl binds -j` and displays them with description tag support.

![Preview](preview.png)

## Features

- **Hyprland-only** — reads live keybinds via `hyprctl binds -j`
- **Description tags** — use `[Category N] description` format in your Hyprland config comments to organise binds into categories
- **Hidden binds** — tag a bind with `[hidden]` to exclude it from the cheatsheet
- **Without Description section** — binds with no description are shown, not dropped
- **Full per-category color customization** — background + text color for every key category, with live preview and clipboard quick-paste
- **Search filter** — type to filter keybindings in the panel
- **Smart key formatting** — XF86 keys display as readable names (Vol Up, Bright Down, etc.)
- **Flexible column layout** (1-4 columns) with auto-height
- **IPC support** — `toggle` and `refresh` for global hotkeys

## Installation

```bash
cp -r keybind-cheatsheet ~/.config/noctalia/plugins/
```

## Usage

### Bar Widget
Add the plugin to your bar configuration in Noctalia settings. Click the keyboard icon to open the cheatsheet.

### Global Hotkey

**Hyprland:**
```bash
bind = $mod, F1, exec, qs -c noctalia-shell ipc call plugin:keybind-cheatsheet toggle
```

### IPC Commands

| Command | Effect |
|---------|--------|
| `qs -c noctalia-shell ipc call plugin:keybind-cheatsheet toggle` | Open / close the cheatsheet panel |
| `qs -c noctalia-shell ipc call plugin:keybind-cheatsheet refresh` | Force a re-parse of your keybindings |

`refresh` is useful after editing your config — bind it to a key to reload without restarting the shell.

## Description Tags

Add a description as the third parameter in `bindd` using the format `[Category N] description`:

```bash
bindd = SUPER, Super_L, [Launcher/Shell 1] Launcher, exec, $ipc launcher toggle
bindd = Super, V, [Launcher/Shell hidden] Clipboard history >> clipboard, exec, $ipc launcher clipboard
bindd = Super, J, [Launcher/Shell 8] Toggle bar, exec, $ipc bar toggle
```

- `Category` — group name displayed in the cheatsheet
- `N` — optional priority number for ordering
- `description` — the text shown next to the keybinding
- `[hidden]` — hides the bind from the cheatsheet

## Special Key Formatting

| Raw Key | Display |
|---------|---------|
| `XF86AudioRaiseVolume` | Vol Up |
| `XF86AudioLowerVolume` | Vol Down |
| `XF86AudioMute` | Mute |
| `XF86MonBrightnessUp` | Bright Up |
| `XF86MonBrightnessDown` | Bright Down |
| `Print` | PrtSc |
| `Prior` / `Next` | PgUp / PgDn |

## Color Customization

Every key category has independently themeable **background** and **text** colors:
`Super`, `Ctrl`, `Shift`, `Alt`, `XF86`, `Print`, numeric, mouse and default letter keys — plus the description text color.

- **Two-pill rows** per category: left pill = background, right pill = label text on that background. Click a pill to open the color picker.
- **Theme-aware defaults**: `Super` / `Ctrl` / `Shift` use an empty sentinel meaning "use the Material theme accent" (`mPrimary` / `mSecondary` / `mTertiary`), so themed setups stay untouched unless you deliberately override.
- **Clipboard quick-paste**: copy a `#RRGGBB` / `#RRGGBBAA` hex and a paste icon appears in each pill — one click applies it (clipboard polled via `wl-paste`).
- **Live preview + revert**: changes preview immediately; closing Settings without Save restores the snapshot taken when the panel opened.
- **Per-row reset** and a **"Reset all colors"** action restore theme defaults.

## Settings

Access settings via the gear icon in the panel header:

- **Window width / height** (auto or manual) and **column count** (1-4)
- **Show binds without a description** toggle
- **Per-category colors** — background + text for every category, with clipboard paste and reset
- **Symbol display** — macOS-style, function key, and mouse Nerd Font symbol toggles
- **Super key text** — custom text or Nerd Font codepoint
- **Refresh** — force reload keybindings

## Requirements

- Noctalia Shell 3.6.0+
- Hyprland
- `hyprctl` on `PATH`
- `wl-paste` (wl-clipboard) for the color clipboard quick-paste feature

## License

MIT
