//! Command line: every colour, string and icon the overlay draws.
//!
//! The values here are **already resolved**. This tool deliberately knows nothing
//! about Noctalia's theme: not the palette file, not the colour-role tokens, not
//! the config directory. The plugin side reads the palette through
//! `noctalia.getColor(role)`, so it resolves roles itself and passes plain
//! `#rrggbb` and plain strings here. That keeps this crate free of the shell's
//! config layout, and means a theme change needs no change here.
//!
//! Defaults are the values that used to be hardcoded, sampled from a stock
//! Noctalia palette, so running with no arguments draws what it always did.

use clap::{Parser, ValueEnum};

use crate::icons;

/// What the pointer looks like while the overlay is up.
///
/// Mirrors the QML overlay's `enableCross`
/// (`ScreenShotOverlayCommon.qml:115`: `cursorShape: enableCross ? Qt.CrossCursor
/// : Qt.ArrowCursor`), whose default is `true` — so the crosshair is the default
/// here too.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Cursor {
    /// The crosshair. It makes the selection point obvious, which is the job.
    Crosshair,
    /// Whatever the compositor was already showing.
    Default,
}

/// Screenshot region overlay, ported from Noctalia's V4 plugin.
#[derive(Parser, Debug)]
#[command(name = "screenshot-overlay", version, about, long_about = None)]
pub struct Args {
    /// Where the finished PNG goes. The **caller** chooses: this binary never
    /// invents a location and never writes the image anywhere else. Defaults to
    /// `~/Pictures/Screenshots/screenshot_<ms>_<output>.png` when omitted.
    ///
    /// `-` is rejected, not a stream — see `parse_output`. The path is echoed back
    /// in the report on **stdout**, beside the button and the region.
    #[arg(short = 'o', long = "output", value_name = "PATH", value_parser = parse_output)]
    pub output: Option<String>,

    /// Report the selected region and write no image.
    ///
    /// For a caller with no use for the pixels — the plugin's recording targets
    /// hand the rectangle straight to `wf-recorder` — so the crop and the PNG
    /// encode, which are most of what a release costs, never happen. `-o` would
    /// have nothing to name, so clap refuses the two together.
    #[arg(long, conflicts_with = "output")]
    pub region_only: bool,

    /// What the pointer looks like over the overlay. Needs the compositor to speak
    /// `wp_cursor_shape_v1`; without it the pointer is left as it is.
    #[arg(long, value_enum, default_value = "crosshair")]
    pub cursor: Cursor,

    /// Bar label text. There is no default: the string is the plugin's
    /// translation, and it passes one on every launch. Omit it and the bar draws
    /// the icon and the close button only.
    #[arg(long)]
    pub label: Option<String>,

    /// Bar's icon: a Tabler name (`text-recognition`) or a codepoint (`U+F201`).
    /// There is no default — which icon stands for a target is the plugin's
    /// table, not this binary's. Names resolve against the vendored
    /// `assets/tabler.json`, so **any** Tabler icon can be asked for.
    #[arg(long, value_parser = parse_icon)]
    pub target_icon: Option<char>,

    /// Close button's icon: a Tabler name (`x`) or a codepoint (`U+EB55`).
    #[arg(long, value_parser = parse_icon, default_value = "x")]
    pub close_icon: char,

    /// Accent: the close button while pressed.
    #[arg(long, value_parser = parse_hex, default_value = "#a9c7ff")]
    pub primary: [u8; 3],

    /// Content on `--primary`.
    #[arg(long, value_parser = parse_hex, default_value = "#003063")]
    pub on_primary: [u8; 3],

    /// Close button fill at rest.
    #[arg(long, value_parser = parse_hex, default_value = "#121316")]
    pub surface: [u8; 3],

    /// Bar card fill.
    ///
    /// The Noctalia role was renamed from `surface_variant` to `surface_container`
    /// at plugin API 43; the plugin now passes the new name, and the old spelling
    /// stays accepted so an older caller does not break.
    #[arg(
        long = "surface-container",
        alias = "surface-variant",
        value_parser = parse_hex,
        default_value = "#1e2023"
    )]
    pub surface_variant: [u8; 3],

    /// Bar label and icon colour.
    #[arg(long, value_parser = parse_hex, default_value = "#e3e2e6")]
    pub on_surface: [u8; 3],

    /// Card and button outlines.
    #[arg(long, value_parser = parse_hex, default_value = "#44474e")]
    pub outline: [u8; 3],

    /// Close button fill while hovered.
    #[arg(long, value_parser = parse_hex, default_value = "#dcbce1")]
    pub hover: [u8; 3],

    /// Content on `--hover`.
    #[arg(long, value_parser = parse_hex, default_value = "#3f2845")]
    pub on_hover: [u8; 3],

    /// Selection border, crosshair and drag size label.
    #[arg(long, value_parser = parse_hex, default_value = "#cccccc")]
    pub selection_color: [u8; 3],

    /// Window highlight, drawn at three alphas.
    #[arg(long, value_parser = parse_hex, default_value = "#ffffff")]
    pub window_color: [u8; 3],

    /// Colour the frozen frame is darkened toward, outside the selection.
    #[arg(long, value_parser = parse_hex, default_value = "#111111")]
    pub dim_color: [u8; 3],

    /// How far the frozen frame is darkened, 0..=1.
    #[arg(long, default_value_t = 0.533, value_parser = parse_alpha)]
    pub dim_alpha: f32,

    /// Bar card fill opacity, 0..=1.
    #[arg(long, default_value_t = 0.94, value_parser = parse_alpha)]
    pub card_alpha: f32,

    /// How round the bar's corners are, as a multiple of the native radii.
    ///
    /// This is Noctalia's `shell.corner_radius_scale` — Settings' "corner
    /// roundness" — which is the **one** style value a user can actually turn:
    /// every other metric the bar uses is a compile-time constant in the shell's
    /// `style.h`. `Style::scaledRadius` is `r * localScale * cornerRadiusScale`,
    /// and the buffer/logical `ratio` is the `localScale`, so this is the last
    /// factor. The plugin reads it with `noctalia.getSetting` and passes it, which
    /// keeps the bar's corners as round as the shell's own controls.
    #[arg(long, default_value_t = 1.0, value_parser = parse_radius_scale)]
    pub corner_radius_scale: f32,

    /// PNG compression level for the saved crop: `0` stores the pixels, `9` gets
    /// as small as the encoder can. The QML plugin exposed the same knob as
    /// `pngCompressionLevel`, whose default was 1 — and `1` here is the fastest
    /// deflate level, the same thing the encoder used before this flag existed.
    #[arg(long, value_name = "0-9", default_value_t = 1, value_parser = clap::value_parser!(u8).range(0..=9))]
    pub png_compression: u8,

    /// Turn off window hover detection: nothing lights up under the pointer and a
    /// click without a drag selects nothing. QML's `enableWindowsSelection`,
    /// inverted, so that the default — detection on — needs no flag.
    #[arg(long)]
    pub no_window_detection: bool,
}

impl Args {
    /// Whether window detection should run at all.
    ///
    /// A method rather than reading the negative flag at the call site, so the
    /// double negative lives in exactly one place.
    pub fn window_detection(&self) -> bool {
        !self.no_window_detection
    }

    /// The bar's label: `--label`, or nothing at all.
    ///
    /// There is deliberately no fallback string — see the flag.
    pub fn bar_label(&self) -> Option<&str> {
        self.label.as_deref()
    }

    /// The bar's icon: `--target-icon`, or nothing at all.
    ///
    /// There is deliberately no fallback — see the flag.
    pub fn bar_icon(&self) -> Option<char> {
        self.target_icon
    }
}

/// `#rgb` or `#rrggbb`. The shell also accepts `#rgba` / `#rrggbbaa`; alpha has
/// its own flags here, so a colour argument stays just a colour.
fn parse_hex(raw: &str) -> Result<[u8; 3], String> {
    let s = raw.trim();
    let Some(digits) = s.strip_prefix('#') else {
        return Err(format!("colour {raw:?} must start with '#'"));
    };
    let nib = |c: char| c.to_digit(16).map(|v| v as u8);
    let mut it = digits.chars();
    let bytes = match digits.len() {
        3 => {
            let mut out = [0u8; 3];
            for (i, slot) in out.iter_mut().enumerate() {
                let c = it.next().unwrap();
                let v =
                    nib(c).ok_or_else(|| format!("colour {raw:?} has a non-hex digit {c:?}"))?;
                *slot = v * 17; // #abc expands to #aabbcc
                let _ = i;
            }
            out
        }
        6 => {
            let mut out = [0u8; 3];
            for slot in out.iter_mut() {
                let hi = nib(it.next().unwrap());
                let lo = nib(it.next().unwrap());
                match (hi, lo) {
                    (Some(h), Some(l)) => *slot = h * 16 + l,
                    _ => return Err(format!("colour {raw:?} has a non-hex digit")),
                }
            }
            out
        }
        n => {
            return Err(format!(
                "colour {raw:?} must be #rgb or #rrggbb, got {n} digits"
            ))
        }
    };
    Ok(bytes)
}

/// An icon on the command line: a Tabler/Noctalia name or a codepoint. See
/// [`crate::icons::resolve`] for the order the two readings are tried in.
fn parse_icon(raw: &str) -> Result<char, String> {
    icons::resolve(raw)
}

/// An opacity: a fraction in `0..=1`.
fn parse_alpha(raw: &str) -> Result<f32, String> {
    bounded_float(raw, 0.0, 1.0)
}

/// The user's corner-roundness preference, in Noctalia's own range.
///
/// `shell.corner_radius_scale`, whose schema range is `kCornerRadiusScaleRange`
/// (0.0 ..= 2.0, step 0.05). The shell *clamps* a value outside it rather than
/// refusing, but a caller that got here has a bug, and a usage error says so
/// where a silently clamped bar would just look wrong.
fn parse_radius_scale(raw: &str) -> Result<f32, String> {
    bounded_float(raw, 0.0, 2.0)
}

/// A decimal bounded to `lo..=hi`, inclusive.
///
/// clap's own ranged parsers are integer-only — `RangedI64ValueParser` and
/// `RangedU64ValueParser` — so every float bound in this crate is a parser of our
/// own. It is still clap's mechanism, and that is the point: the bound is checked
/// while the flag is read, so the failure names the flag and carries clap's usage
/// text.
///
/// `NaN` fails the range test, so `--dim-alpha nan` is refused rather than
/// silently making every "is this pixel inside" comparison false.
fn bounded_float(raw: &str, lo: f32, hi: f32) -> Result<f32, String> {
    let value: f32 = raw
        .parse()
        .map_err(|_| format!("`{raw}` is not a number"))?;
    if !(lo..=hi).contains(&value) {
        return Err(format!("must be between {lo} and {hi}, got {value}"));
    }
    Ok(value)
}

/// The path the finished PNG goes to. `-` is refused here rather than treated as
/// stdout, which is the convention everywhere else: stdout carries the report and
/// nothing else, so the image is always a file. Catching it at the flag also means
/// a caller cannot silently create a file literally named `-`.
fn parse_output(raw: &str) -> Result<String, String> {
    if raw == "-" {
        return Err(
            "`-` is not a stream; pass a real path (the caller chooses where the image goes)"
                .to_string(),
        );
    }
    Ok(raw.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_forms() {
        assert_eq!(parse_hex("#cccccc").unwrap(), [0xcc, 0xcc, 0xcc]);
        assert_eq!(parse_hex("#abc").unwrap(), [0xaa, 0xbb, 0xcc]);
        assert_eq!(parse_hex(" #123456 ").unwrap(), [0x12, 0x34, 0x56]);
        assert!(parse_hex("cccccc").is_err(), "missing # must be rejected");
        assert!(parse_hex("#12345").is_err());
        assert!(parse_hex("#gggggg").is_err());
    }

    /// The icon arguments take a Tabler name as readily as a codepoint; the
    /// resolution order itself is `icons::resolve`'s tests.
    #[test]
    fn icon_arguments_take_names_or_codepoints() {
        let by_name = Args::parse_from(["screenshot-overlay", "--target-icon", "photo-search"]);
        assert_eq!(by_name.bar_icon(), Some('\u{F364}'));

        let by_code = Args::parse_from(["screenshot-overlay", "--target-icon", "0xF364"]);
        assert_eq!(by_code.bar_icon(), Some('\u{F364}'));

        assert!(
            Args::try_parse_from(["screenshot-overlay", "--target-icon", "not-an-icon"]).is_err()
        );
    }

    /// The defaults must stay exactly what the overlay drew before they were
    /// configurable, or a no-argument run changes its appearance.
    #[test]
    fn defaults_match_the_previous_hardcoded_values() {
        let args = Args::parse_from(["screenshot-overlay"]);
        assert_eq!(args.output, None, "no -o means the default location");
        assert_eq!(args.label, None, "no --label means no text at all");
        assert_eq!(
            args.target_icon, None,
            "no --target-icon means no icon either"
        );
        // What a no-argument run actually draws with: nothing on the bar but the
        // close button. Icon and text are both the caller's to supply.
        assert_eq!(args.bar_label(), None);
        assert_eq!(args.bar_icon(), None);
        assert_eq!(args.close_icon, '\u{EB55}');
        assert_eq!(args.primary, [0xa9, 0xc7, 0xff]);
        assert_eq!(args.on_primary, [0x00, 0x30, 0x63]);
        assert_eq!(args.surface, [0x12, 0x13, 0x16]);
        assert_eq!(args.surface_variant, [0x1e, 0x20, 0x23]);
        assert_eq!(args.on_surface, [0xe3, 0xe2, 0xe6]);
        assert_eq!(args.outline, [0x44, 0x47, 0x4e]);
        assert_eq!(args.hover, [0xdc, 0xbc, 0xe1]);
        assert_eq!(args.on_hover, [0x3f, 0x28, 0x45]);
        assert_eq!(args.selection_color, [0xcc, 0xcc, 0xcc]);
        assert_eq!(args.window_color, [0xff, 0xff, 0xff]);
        assert_eq!(args.dim_color, [0x11, 0x11, 0x11]);
        assert!((args.dim_alpha - 0.533).abs() < 1e-6);
        assert!((args.card_alpha - 0.94).abs() < 1e-6);
        assert!(
            (args.corner_radius_scale - 1.0).abs() < 1e-6,
            "Noctalia's own default, so a bare run keeps the radii it hardcoded"
        );
        assert_eq!(args.png_compression, 1, "QML's pngCompressionLevel default");
        assert!(
            args.window_detection(),
            "window detection is on unless --no-window-detection is passed"
        );
    }

    /// The two flags added for the QML port: the PNG level is bounded, and the
    /// negative flag really flips the accessor the drawing code asks.
    #[test]
    fn the_port_flags_parse() {
        let args = Args::parse_from(["screenshot-overlay", "--png-compression", "9"]);
        assert_eq!(args.png_compression, 9);
        assert!(args.window_detection());

        let args = Args::parse_from(["screenshot-overlay", "--no-window-detection"]);
        assert!(!args.window_detection());
        assert_eq!(args.png_compression, 1);

        // 10 is not a deflate level, and clap must say so rather than pass it on.
        assert!(Args::try_parse_from(["screenshot-overlay", "--png-compression", "10"]).is_err());
        assert!(Args::try_parse_from(["screenshot-overlay", "--png-compression", "-1"]).is_err());
    }

    /// The bounded floats are bounded at the flag, not after the whole line has
    /// parsed: `parse_alpha` and `parse_radius_scale` are value parsers, so clap
    /// rejects the value itself. Both ranges are Noctalia's — a 0..=1 alpha, and
    /// `kCornerRadiusScaleRange`'s 0..=2 for the corner roundness.
    #[test]
    fn bounded_floats_are_checked() {
        for flag in ["--dim-alpha", "--card-alpha"] {
            for value in ["1.5", "-0.1", "nan", "inf", "not-a-number"] {
                let argv = ["screenshot-overlay", flag, value];
                assert!(
                    Args::try_parse_from(argv).is_err(),
                    "{flag} {value} must be refused"
                );
            }
            // The ends of the range are inclusive, and reach the field.
            for (value, expected) in [("0", 0.0), ("1", 1.0), ("0.5", 0.5)] {
                let argv = ["screenshot-overlay", flag, value];
                let args = Args::parse_from(argv);
                let got = if flag == "--dim-alpha" {
                    args.dim_alpha
                } else {
                    args.card_alpha
                };
                assert!((got - expected).abs() < 1e-6, "{flag} {value} -> {got}");
            }
        }

        // Same parser shape, a wider range — a radius scale is not an alpha, so
        // sharing one bound would be wrong in both directions.
        for value in ["2.1", "-0.1", "nan", "not-a-number"] {
            let argv = ["screenshot-overlay", "--corner-radius-scale", value];
            assert!(
                Args::try_parse_from(argv).is_err(),
                "--corner-radius-scale {value} must be refused"
            );
        }
        for (value, expected) in [("0", 0.0), ("1", 1.0), ("2", 2.0), ("1.25", 1.25)] {
            let argv = ["screenshot-overlay", "--corner-radius-scale", value];
            let got = Args::parse_from(argv).corner_radius_scale;
            assert!((got - expected).abs() < 1e-6, "{value} -> {got}");
        }
    }

    /// `-o` must reach `output` in both forms, and a bare `-` must be refused
    /// rather than quietly creating a file called `-`.
    #[test]
    fn output_flag_forms() {
        let short = Args::parse_from(["screenshot-overlay", "-o", "/tmp/a.png"]);
        assert_eq!(short.output.as_deref(), Some("/tmp/a.png"));

        let long = Args::parse_from(["screenshot-overlay", "--output", "/tmp/b.png"]);
        assert_eq!(long.output.as_deref(), Some("/tmp/b.png"));

        // `-` means stdout in every other tool, so the refusal has to say why —
        // and it is clap saying it now, with the usage text and exit code 2.
        let err = Args::try_parse_from(["screenshot-overlay", "-o", "-"]).unwrap_err();
        let text = err.to_string();
        assert!(text.contains("not a stream"), "{text}");
        assert!(text.contains("path"), "{text}");
    }

    /// Both presentation flags are the caller's, and neither has a default: an
    /// icon is any name in the vendored table (or a codepoint), and the label is
    /// whatever string the caller passes. There is no target to override — which
    /// icon stands for what is the plugin's table, not this binary's.
    #[test]
    fn presentation_flags_are_the_callers() {
        let bare = Args::parse_from(["screenshot-overlay"]);
        assert_eq!(bare.bar_icon(), None, "no icon is invented");
        assert_eq!(bare.bar_label(), None, "and no text either");

        // Any name in the table, not a fixed set of five.
        let named = Args::parse_from(["screenshot-overlay", "--target-icon", "photo-search"]);
        assert_eq!(named.bar_icon(), Some('\u{F364}'));

        let both = Args::parse_from([
            "screenshot-overlay",
            "--label",
            "Text",
            "--target-icon",
            "text-recognition",
        ]);
        assert_eq!(both.bar_label(), Some("Text"));
        assert_eq!(both.bar_icon(), Some('\u{F204}'));

        // A name that is not in the table is refused at parse time, so a typo
        // fails loudly instead of drawing an empty box.
        assert!(
            Args::try_parse_from(["screenshot-overlay", "--target-icon", "not-an-icon"]).is_err()
        );
    }
    /// The pointer defaults to the crosshair, matching the QML `enableCross`
    /// default, and the arrow is reachable for anyone who wants it.
    #[test]
    fn the_cursor_defaults_to_a_crosshair() {
        let args = Args::parse_from(["screenshot-overlay"]);
        assert_eq!(args.cursor, Cursor::Crosshair, "QML's enableCross default");

        assert_eq!(
            Args::parse_from(["screenshot-overlay", "--cursor", "default"]).cursor,
            Cursor::Default
        );
        assert_eq!(
            Args::parse_from(["screenshot-overlay", "--cursor", "crosshair"]).cursor,
            Cursor::Crosshair
        );
        assert!(Args::try_parse_from(["screenshot-overlay", "--cursor", "nope"]).is_err());
    }

    /// The action flags are gone. Deciding what happens to the image belongs to
    /// the plugin, which learns the outcome from the report instead.
    #[test]
    fn the_action_flags_no_longer_exist() {
        for argv in [
            vec!["screenshot-overlay", "--mode", "save"],
            vec!["screenshot-overlay", "--editor", "swappy"],
            vec!["screenshot-overlay", "--keep-source"],
        ] {
            assert!(
                Args::try_parse_from(&argv).is_err(),
                "{argv:?} must no longer parse"
            );
        }
    }
}
