//! Screen capture via libwayshot (zwlr_screencopy) and crop output.

use std::path::{Path, PathBuf};

use image::codecs::png::{CompressionType, FilterType, PngEncoder};
use image::{ImageEncoder, RgbaImage};
use libwayshot::WayshotConnection;

use crate::render::{Base, Dim};

/// Where a finished crop is written.
///
/// There is deliberately no stream variant: this binary writes nothing to
/// stdout. The caller chooses the path — including a scratch path under a
/// temporary directory, if that is what it wants — and the path comes back in
/// the report. The plugin side could not consume a byte stream anyway, since
/// `runAsync`'s callback hands Luau a string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Destination {
    /// `-o <path>`: exactly this file. Its parent directory is the caller's
    /// business — see `prepare_output_dir`.
    File(PathBuf),
    /// No `-o`: `~/Pictures/Screenshots/screenshot_<ms>_<output>.png`, the name
    /// the QML plugin writes.
    Default,
}

impl Destination {
    /// Map the `-o` argument. A literal `-` is refused by `parse_output`, the
    /// flag's value parser, not here — so the refusal names the flag and carries
    /// clap's usage text.
    pub fn from_arg(output: Option<&str>) -> Self {
        match output {
            None => Self::Default,
            Some(path) => Self::File(PathBuf::from(path)),
        }
    }
}

/// Grab one output as precomputed base images (raw + dimmed).
///
/// This runs before the overlay surface is mapped, so the frozen frame is the
/// plain desktop and does not contain the overlay itself.
pub fn grab(wc: &WayshotConnection, name: &str, dim: Dim) -> Result<Base, String> {
    let info = wc
        .get_all_outputs()
        .iter()
        .find(|o| o.name == name)
        .cloned();
    let Some(info) = info else {
        let names: Vec<&str> = wc
            .get_all_outputs()
            .iter()
            .map(|o| o.name.as_str())
            .collect();
        return Err(format!("output {name:?} not found (have {names:?})"));
    };

    let rgba = wc
        .screenshot_single_output(&info, false)
        .map_err(|e| e.to_string())?
        .to_rgba8();
    Ok(Base::from_rgba(&rgba, dim))
}

/// Crop the region `(x, y, w, h)` out of the frozen frame, as RGBA.
///
/// `Base` holds BGRA, so this is also the channel swap a PNG needs.
fn crop(base: &Base, x: u32, y: u32, w: u32, h: u32) -> Option<RgbaImage> {
    // Bounds-check up front. The caller clamps today, but reading past the frame
    // would panic on the slice index rather than fail, and a half-read image is
    // worse than a clear error.
    if w == 0 || h == 0 {
        return None;
    }
    if x as usize + w as usize > base.w() || y as usize + h as usize > base.h() {
        return None;
    }

    let src = base.raw();
    let mut buf = Vec::with_capacity(w as usize * h as usize * 4);
    for row in 0..h {
        let o = ((y + row) as usize * base.w() + x as usize) * 4;
        for col in 0..w as usize {
            let p = o + col * 4;
            buf.extend_from_slice(&[src[p + 2], src[p + 1], src[p], src[p + 3]]);
        }
    }
    RgbaImage::from_raw(w, h, buf)
}

/// The default output location, named the way the QML plugin names it:
/// `screenshot_<epoch_ms>_<sanitised-output>.png`.
fn default_path(name: &str) -> PathBuf {
    let dir = std::env::var("HOME")
        .map(|h| format!("{h}/Pictures/Screenshots"))
        .unwrap_or_else(|_| "/tmp".to_string());
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let safe: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    PathBuf::from(format!("{dir}/screenshot_{ms}_{safe}.png"))
}

/// Encode a crop of the frozen frame as PNG bytes.
///
/// Split from writing on purpose: the app crops while the overlay is still up,
/// then does the I/O *after* the surface is gone, so nothing is written while the
/// overlay is still covering the screen.
///
/// `compression` is the level `--png-compression` took, 0..=9. It is passed
/// through to the encoder rather than clamped away here: clap already bounds it,
/// and a silent clamp would make a wrong value look like it worked.
pub fn png_bytes(
    base: &Base,
    x: u32,
    y: u32,
    w: u32,
    h: u32,
    compression: u8,
) -> Result<Vec<u8>, String> {
    let Some(cropped) = crop(base, x, y, w, h) else {
        return Err(format!("crop {w}x{h}+{x}+{y} is outside the frozen frame"));
    };
    let mut png = Vec::new();
    PngEncoder::new_with_quality(
        &mut png,
        compression_type(compression),
        FilterType::Adaptive,
    )
    .write_image(
        cropped.as_raw(),
        cropped.width(),
        cropped.height(),
        image::ExtendedColorType::Rgba8,
    )
    .map_err(|e| format!("PNG encode failed: {e}"))?;
    Ok(png)
}

/// The encoder's level for a `0..=9` deflate level.
///
/// `Level(1)` is the fastest deflate pass and is what the default in
/// `--png-compression` asks for; `0` means "store", not "level zero of nine".
fn compression_type(level: u8) -> CompressionType {
    match level {
        0 => CompressionType::Uncompressed,
        n => CompressionType::Level(n.min(9)),
    }
}

/// Write encoded PNG bytes to `dest`, returning the path they landed on — which
/// is the thing the caller needs, since the report carries it back.
///
/// An error means the region the user selected produced no file.
pub fn write_png(png: &[u8], name: &str, dest: &Destination) -> Result<PathBuf, String> {
    let path = path_for(name, dest);
    prepare_output_dir(dest, &path)?;
    std::fs::write(&path, png).map_err(|e| format!("saving {} failed: {e}", path.display()))?;
    Ok(path)
}

/// The file `dest` names. Both variants name one, so this cannot fail.
pub fn path_for(name: &str, dest: &Destination) -> PathBuf {
    match dest {
        Destination::File(path) => path.clone(),
        Destination::Default => default_path(name),
    }
}

/// Create this tool's own output directory — and only that one.
///
/// An explicit `-o <path>` is taken as given: a missing parent there is the
/// caller's mistake, and papering over it by creating a tree at a path they may
/// have mistyped is worse than failing. The default location is this tool's own
/// (`~/Pictures/Screenshots`, which the QML `capture.sh` also creates), so that
/// one is made on demand.
pub fn prepare_output_dir(dest: &Destination, path: &Path) -> Result<(), String> {
    if !matches!(dest, Destination::Default) {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("mkdir {} failed: {e}", parent.display()))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_arg_maps_to_a_destination() {
        assert_eq!(Destination::from_arg(None), Destination::Default);
        assert_eq!(
            Destination::from_arg(Some("/tmp/x.png")),
            Destination::File(PathBuf::from("/tmp/x.png"))
        );
        // There is no stream variant to map onto; `-` never gets this far,
        // because clap's `parse_output` refuses it at the flag.
        assert_eq!(
            Destination::from_arg(Some("-")),
            Destination::File(PathBuf::from("-"))
        );
    }

    /// `path_for` must name the caller's file verbatim — that path is the whole
    /// hand-off, so it cannot be rewritten on the way through.
    #[test]
    fn path_for_passes_the_callers_path_through() {
        assert_eq!(
            path_for("DP-1", &Destination::File(PathBuf::from("/tmp/a b.png"))),
            PathBuf::from("/tmp/a b.png")
        );
        assert_eq!(
            path_for("DP-1", &Destination::Default),
            default_path("DP-1")
        );
    }

    /// An explicit `-o` is taken as given: its parent directory is **not**
    /// created, so a mistyped path fails loudly instead of growing a tree. The
    /// write must therefore surface the error rather than pretend to succeed.
    #[test]
    fn an_explicit_output_does_not_get_its_directory_made() {
        let root = std::env::temp_dir().join(format!("ssr-dirs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();

        let explicit = root.join("missing/deep/x.png");
        prepare_output_dir(&Destination::File(explicit.clone()), &explicit).unwrap();
        assert!(
            !explicit.parent().unwrap().exists(),
            "explicit -o must not create its parent"
        );
        assert!(
            write_png(
                b"not really a png",
                "DP-1",
                &Destination::File(explicit.clone())
            )
            .is_err(),
            "the write must fail rather than silently succeed"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    /// The QML name is `screenshot_<stamp>_<safe_output>.png`; anything outside
    /// `[A-Za-z0-9_-]` in the connector name becomes `_`.
    #[test]
    fn default_path_sanitises_the_output_name() {
        let path = default_path("DP-1/hdmi:0");
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        assert!(name.starts_with("screenshot_"), "{name}");
        assert!(name.ends_with("_DP-1_hdmi_0.png"), "{name}");
    }

    /// `crop` must undo the BGRA packing `Base` uses and select exactly the
    /// requested rectangle — a channel swap here would silently recolour every
    /// screenshot.
    #[test]
    fn crop_swaps_back_to_rgba_and_selects_the_rect() {
        // 3x2, with a different value in every channel of every pixel, so an
        // accidental swap or an off-by-one row cannot pass.
        let mut img = RgbaImage::new(3, 2);
        for (i, px) in img.pixels_mut().enumerate() {
            let i = i as u8;
            *px = image::Rgba([10 + i, 100 + i, 200 + i, 250 - i]);
        }
        let base = Base::from_rgba(
            &img,
            Dim {
                rgb: [0, 0, 0],
                alpha: 0.0,
            },
        );

        assert_eq!(crop(&base, 0, 0, 3, 2).unwrap(), img, "whole frame");

        let sub = crop(&base, 1, 0, 2, 1).unwrap();
        assert_eq!(sub.dimensions(), (2, 1));
        assert_eq!(sub.get_pixel(0, 0), img.get_pixel(1, 0));
        assert_eq!(sub.get_pixel(1, 0), img.get_pixel(2, 0));

        // Out of range fails instead of panicking on the slice index.
        assert!(crop(&base, 2, 1, 2, 2).is_none(), "overhangs both edges");
        assert!(
            crop(&base, 3, 0, 1, 1).is_none(),
            "starts past the right edge"
        );
        assert!(crop(&base, 0, 0, 0, 2).is_none(), "empty width");
    }

    /// `--png-compression` must actually reach the encoder, and must only ever
    /// change the *size*: a level that altered the pixels would silently
    /// corrupt every screenshot. Levels 0 and 9 are the extremes, so a level
    /// that were dropped on the floor and never used would show up as two
    /// byte-identical files.
    #[test]
    fn the_compression_level_reaches_the_encoder() {
        // Noise, not a flat fill: a solid colour compresses to almost nothing at
        // every level, so the sizes would not separate.
        let mut img = RgbaImage::new(64, 64);
        let mut seed = 0x1234_5678u32;
        for px in img.pixels_mut() {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            *px = image::Rgba([(seed >> 24) as u8, (seed >> 16) as u8, 255, 255]);
        }
        let base = Base::from_rgba(
            &img,
            Dim {
                rgb: [0, 0, 0],
                alpha: 0.0,
            },
        );

        let stored = png_bytes(&base, 0, 0, 64, 64, 0).unwrap();
        let packed = png_bytes(&base, 0, 0, 64, 64, 9).unwrap();

        let decode = |bytes: &[u8]| image::load_from_memory(bytes).unwrap().to_rgba8();
        assert_eq!(
            decode(&stored),
            decode(&packed),
            "the compression level must not change the pixels"
        );
        assert_eq!(decode(&stored), img, "and the crop must survive it");
        assert!(
            packed.len() < stored.len(),
            "level 9 ({}) should beat level 0 ({}) on noise",
            packed.len(),
            stored.len()
        );

        // A level past 9 is clamped rather than handed to libpng, which asserts
        // on it: clap bounds the flag, but this function should not be the one
        // thing that turns a bad level into an abort.
        assert!(png_bytes(&base, 0, 0, 64, 64, 200).is_ok());
    }
}
