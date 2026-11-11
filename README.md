# Pulsar's Noctalia Plugins

V5 plugins for [Noctalia](https://github.com/noctalia-dev/noctalia), plus the small
Rust helper they shell out to.

## Layout

- `screen-shot-and-record/` — the plugin: `plugin.toml`, Luau entries, translations.
- `screenshot-overlay/` — the standalone Wayland region-selection binary the plugin
  launches. It ships outside the plugin because a plugin repository cannot carry a
  compiled binary.

## Adding a plugin

One directory per plugin at the repository root, with a `plugin.toml`, a `README.md`,
`translations/en.json` and a 960x540 `thumbnail.webp`. CI validates the manifest
(`.github/workflows/validate-plugins.yml`) and rebuilds `catalog.toml` on push to
`main`; `catalog.toml` is generated, do not edit it by hand.

## The overlay

```sh
nix build ./screenshot-overlay
```
