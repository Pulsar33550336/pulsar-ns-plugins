//! The close-button and target icons.
//!
//! Noctalia draws its UI icons from a webfont it builds with Fontello from
//! [Tabler Icons](https://tabler.io/icons) (MIT, © Paweł Kuna), renaming the
//! family to `noctalia-tabler-icons`. Glyphs are Private Use codepoints
//! addressed by name through a sibling `tabler.json`; the shell resolves a name
//! through `GlyphRegistry::lookup` with a hand-curated alias table
//! (`src/render/text/glyph_registry.cpp`).
//!
//! That font is **vendored here** (`assets/noctalia-tabler.ttf`, ~2.3 MiB) and
//! compiled into the binary, so the tool carries its own icon coverage and does
//! not depend on a Noctalia install being present or findable. The whole set
//! comes along rather than the handful of glyphs used today, because the caller
//! on the plugin side may ask for any icon.
//!
//! Noctalia's build is the one vendored, not an upstream Tabler release: the
//! glyphs match what the shell renders, and the family name above is that
//! build's. Tabler's licence requires the notice to travel with the font, so
//! `assets/tabler-icons-license.txt` ships alongside it — keep the two together.

/// Icon font bytes, embedded at compile time.
pub const FONT: &[u8] = include_bytes!("../assets/noctalia-tabler.ttf");

/// Noctalia's name → codepoint table, vendored beside the font (~470 KiB).
///
/// Same provenance as [`FONT`]: Noctalia generates it from Tabler Icons and
/// addresses glyphs by name through it (`GlyphRegistry::lookup`). Carrying a copy
/// means the caller can name *any* icon (`--target-icon text-recognition`) rather
/// than having to look the codepoint up on the shell side first.
pub const TABLE: &str = include_str!("../assets/tabler.json");

/// Resolve an icon the caller referred to by name or by codepoint.
///
/// Tried in this order:
///
/// 1. a prefixed codepoint — `U+F201`, `u+f201`, `0xF201`
/// 2. a Tabler/Noctalia name — `screenshot`, `text-recognition`
/// 3. a bare codepoint — `F201`
///
/// A **known name wins over a bare hex string**, because two names in the table
/// are themselves valid hex (`abc`, `bed`). Write `U+ABC` to force the codepoint
/// reading of an ambiguous one.
pub fn resolve(raw: &str) -> Result<char, String> {
    let s = raw.trim();
    if s.is_empty() {
        return Err("icon name is empty".to_string());
    }

    for prefix in ["U+", "u+", "0x", "0X"] {
        if let Some(digits) = s.strip_prefix(prefix) {
            return from_hex(digits, raw);
        }
    }

    if let Some(known) = lookup(s) {
        return Ok(known);
    }

    // Not a name in the table; a bare codepoint is still accepted, so a caller
    // that already resolved one keeps working.
    from_hex(s, raw)
}

fn from_hex(digits: &str, raw: &str) -> Result<char, String> {
    let value = u32::from_str_radix(digits, 16)
        .map_err(|_| format!("icon {raw:?} is neither a known icon name nor a hex codepoint"))?;
    char::from_u32(value).ok_or_else(|| format!("icon {raw:?} is not a valid codepoint"))
}

/// Look a name up in the vendored table.
fn lookup(name: &str) -> Option<char> {
    use std::collections::HashMap;
    use std::sync::OnceLock;

    // Parsed once, on the first *named* icon. A run that only passes codepoints
    // never pays for it, and repeating a name does not re-parse 470 KiB.
    static NAMES: OnceLock<HashMap<String, char>> = OnceLock::new();
    let names = NAMES.get_or_init(|| {
        let table: serde_json::Map<String, serde_json::Value> =
            serde_json::from_str(TABLE).expect("the vendored tabler.json must parse");
        table
            .into_iter()
            .filter_map(|(name, entry)| {
                let code = entry.get("codepoint")?.as_str()?;
                let value =
                    u32::from_str_radix(code.trim_start_matches(['U', 'u', '+']), 16).ok()?;
                char::from_u32(value).map(|c| (name, c))
            })
            .collect()
    });
    names.get(name).copied()
}

/// Font family inside the icon font. Note the `-icons` suffix — the family is
/// **not** the file name. Asking for the wrong family silently falls back to a
/// system font, which has no PUA glyph, so the icon draws as nothing.
pub const FAMILY: &str = "noctalia-tabler-icons";

// The two glyphs this overlay draws are codepoints the caller supplies, not
// constants here — see `cli.rs`. For reference, and what the command line
// defaults to: the close affordance is Noctalia's `close` alias
// (`glyph_registry.cpp`) for Tabler `x`, `U+EB55`; the screenshot target is the
// QML `iconForTarget("screenshot")` name, a direct Tabler entry, `U+F201`.

/// Text stand-in used when the icon font could not be registered, so the bar
/// still reads as a close button. Registration only fails if the embedded bytes
/// are unusable, which should not happen — this is depth, not a normal path.
pub const CLOSE_FALLBACK: char = '\u{d7}'; // ×

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codepoint_forms() {
        assert_eq!(resolve("U+F201").unwrap(), '\u{F201}');
        assert_eq!(resolve("u+eb55").unwrap(), '\u{EB55}');
        assert_eq!(resolve("0xF201").unwrap(), '\u{F201}');
        assert_eq!(resolve("F201").unwrap(), '\u{F201}');
        assert!(resolve("U+110000").is_err(), "beyond the last codepoint");
        assert!(resolve("").is_err(), "an empty name is not an icon");
    }

    #[test]
    fn names_resolve_through_the_vendored_table() {
        assert_eq!(resolve("screenshot").unwrap(), '\u{F201}');
        assert_eq!(resolve("text-recognition").unwrap(), '\u{F204}');
        assert_eq!(resolve("x").unwrap(), '\u{EB55}');
        assert_eq!(resolve("  camera  ").unwrap(), '\u{EA54}', "trimmed");
    }

    /// Exactly two names are themselves valid hex. The name must win, and the
    /// `U+` prefix must be the way back to the codepoint reading.
    #[test]
    fn a_known_name_beats_a_bare_hex_string() {
        for (name, hex) in [("abc", '\u{ABC}'), ("bed", '\u{BED}')] {
            let named = resolve(name).unwrap();
            assert_ne!(named, hex, "{name} must resolve as a name, not as hex");
            assert_eq!(named, lookup(name).unwrap());
            assert_eq!(resolve(&format!("U+{:04X}", hex as u32)).unwrap(), hex);
        }
    }

    #[test]
    fn unknown_names_are_rejected_unless_they_are_hex() {
        assert!(resolve("no-such-icon-xyz").is_err());
        assert!(resolve("nope").is_err(), "'nope' is not hex either");
    }

    /// The names `config.rs` relies on must resolve to the codepoints it
    /// hardcodes, so the target table and the vendored name table cannot drift.
    #[test]
    fn target_names_match_their_codepoints() {
        for (name, glyph) in [
            ("screenshot", '\u{F201}'),
            ("text-recognition", '\u{F204}'),
            ("photo-search", '\u{F364}'),
            ("camera", '\u{EA54}'),
            ("camera-spark", '\u{FFBC}'),
        ] {
            assert_eq!(resolve(name).unwrap(), glyph, "{name}");
        }
    }
}
