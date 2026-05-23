# keybind-cheatsheet — agent guide

## No build/test/lint/typecheck

This is a QML plugin for Noctalia Shell. There is no build system, package manager, or CI. "Install" means `cp -r keybind-cheatsheet ~/.config/noctalia/plugins/`. There are no tests — QML is dynamically typed.

## Entry points

Defined in `manifest.json`:
- `Main.qml` — calls `hyprctl binds -j`, parses description tags, organises into categories/groups
- `BarWidget.qml` — taskbar icon
- `Panel.qml` — the searchable overlay
- `Settings.qml` — settings popup (General + Appearance tabs)

All QML files are flat in the root. No subdirectories for source.

## Architecture

Hyprland-only. No compositor detection, no config-file parsing (Hyprland Lua, Niri KDL, MangoWC — all removed). The sole source of keybind data is `hyprctl binds -j`. Description tags come from the IPC JSON `description` field.

## Translation i18n

Keys in `i18n/*.json` use dot-separated namespacing (e.g. `panel.title`). Call them WITHOUT the plugin prefix:
```qml
pluginApi.tr("panel.title")     // correct
pluginApi.tr("keybind-cheatsheet.panel.title")  // wrong
```

## Color quirks

- `keyColorSuper`, `keyColorCtrl`, `keyColorShift`: empty string means "use Material theme accent". Stored as `string` type, NOT `color` (QML coerces `""` to `#000000`).
- Colors apply live via `Object.assign` in `_applyPreview()`. A full snapshot is taken on `Component.onCompleted`; reverted on destruction if not applied.
- Other color settings use shadow properties (`edit*`/`value*`) committed on `saveSettings()`.

## Gotchas

- `import "." as Local` required in `Settings.qml` and `ColorPairRow.qml` because Noctalia's popup loader uses `setSource(file://...)` which doesn't add the plugin dir to the implicit import list.
- `cheatsheetDataVersion` counter: QML property bindings don't detect deep array changes in `pluginSettings.cheatsheetData`, so `Main.qml` manually bumps a version counter to force `Panel.qml` re-evaluation.
- Bind override identity (`bindId`): `submap|modmask|key|flags|dispatcher` — deliberately excludes Hyprland Lua registry ref, so overrides survive compositor restarts.
- Sequential bind merging: 3+ consecutive numeric binds with identical modifiers/verbs/description templates get collapsed into ranged entries (e.g. `Super+1-5 -> "Workspace 1-5"`).
- External deps: `hyprctl` (must be on PATH), `wl-paste` (clipboard paste in color picker).
- `keySymbolMap` in `Panel.qml` centralises key-name formatting (e.g. `CTRL` → `Ctrl`, `SUPER` → `Super`).
- Mod key variable is user-configurable in settings but no longer read from config files.
