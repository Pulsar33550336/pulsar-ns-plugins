//! The bottom action bar: a card holding the target label and a close button,
//! centred along the bottom edge of each overlay.
//!
//! The *structure* follows the QML overlay
//! (`ScreenShotOverlayCommon.qml:161-201`: an `NBox` card with an icon, a label
//! and a close `NButton`), while the *control styling* follows the native shell
//! instead of the QML colour literals:
//!
//! - card  = `Box::setCardStyle(scale, 0.94, true)` — `SurfaceVariant` fill at
//!   0.94 alpha, `Outline` border, `radiusXl`
//! - label = caption size in `OnSurface`
//! - button = `ButtonVariant::Outline` — `Surface` fill + `Outline` border with
//!   `OnSurface` content, or `Hover`/`OnHover` with no border while hovered
//!
//! See `~/Projects/noctalia/src/ui/controls/{box,button}.cpp` and
//! `src/ui/style.h` for the tokens these numbers come from.

use crate::cli::Args;
use crate::icons;
use crate::render::{Canvas, Rect, TextRenderer};

// Logical-pixel metrics straight from the native style tokens.
//
// All but the first two are compile-time constants in the shell (`style.h` is
// `inline constexpr` throughout), so there is nothing to read at runtime and no
// reason for a flag: they *are* the native look, which is the point of the bar.
// The radii are the exception — the one style value a user can turn is
// `shell.corner_radius_scale`, so they are multiplied by `--corner-radius-scale`
// (`Style::scaledRadius`: `r * localScale * cornerRadiusScale`, where our `ratio`
// is the `localScale`).
const CARD_RADIUS: f32 = 12.0; // Style::radiusXl, via setCardStyle
const BUTTON_RADIUS: f32 = 6.0; // Style::radiusMd
const BUTTON_SIZE: f32 = 32.0; // Style::controlHeightSm
const BORDER: f32 = 1.0; // Style::borderWidth
const PADDING: f32 = 12.0; // Style::spaceMd
const GAP: f32 = 12.0; // Style::spaceMd
const MARGIN: f32 = 12.0; // Style::spaceMd, from the bottom edge

// One step up the native size ladder from the caption size, which reads too
// small on a 2x output. The glyph keeps the same ratio to the label as
// `baseGlyphSize` has to `fontSizeCaption`.
pub const LABEL_SIZE: f32 = 16.0; // Style::fontSizeTitle
pub const GLYPH_SIZE: f32 = 20.0; // Style::fontSizeHeader

/// One glyph as it will actually be painted: its advance width, the height of
/// its ink, and how far that ink's top sits below the draw origin.
///
/// Centring on this rather than on a font line box is what puts the icon and the
/// label on the same optical line. They come from different fonts, so equal
/// line-box heights still leave their ink at different heights.
#[derive(Clone, Copy, Debug)]
pub struct GlyphBox {
    pub width: i32,
    pub height: i32,
    pub ink_top: i32,
}

impl GlyphBox {
    /// `ink` is `(top, height)` from `TextRenderer`; `none` falls back to a
    /// nominal height so a glyph that fails to measure still occupies a row.
    fn new(width: i32, ink: Option<(i32, i32)>, nominal_height: f32) -> Self {
        match ink {
            Some((top, height)) => Self {
                width: width.max(1),
                height: height.max(1),
                ink_top: top,
            },
            None => Self {
                width: width.max(1),
                height: nominal_height.round().max(1.0) as i32,
                ink_top: 0,
            },
        }
    }
}

/// The bar's three glyphs, measured. Worth hoisting out of the per-frame path:
/// measuring shapes text through the whole font stack.
#[derive(Clone, Copy, Debug)]
pub struct Glyphs {
    /// Target icon. `None` when there is no icon font loaded or no icon to draw,
    /// in which case the icon is dropped outright and no gap is reserved for it.
    pub icon: Option<GlyphBox>,
    /// Bar label. `None` when the caller passed no `--label` — there is no
    /// built-in text — with the same no-reserved-hole treatment.
    pub label: Option<GlyphBox>,
    /// The close button's content.
    pub close: GlyphBox,
}

/// Measure the bar's glyphs at `ratio` buffer pixels per logical pixel. The text
/// and both icons come from the command line.
pub fn measure_glyphs(text: &mut TextRenderer, ratio: f64, args: &Args) -> Glyphs {
    let r = ratio as f32;
    let label_size = LABEL_SIZE * r;
    let glyph_size = GLYPH_SIZE * r;

    let label = args.bar_label().map(|label| {
        GlyphBox::new(
            text.measure(label, label_size).ceil() as i32,
            text.ink_extent(label, label_size),
            label_size * 0.75,
        )
    });
    // Measure whichever close glyph will actually be painted: the configured
    // icon from the icon font, or the text-font fallback.
    let close = if text.has_icons() {
        GlyphBox::new(
            text.measure_icon(args.close_icon, glyph_size).ceil() as i32,
            text.ink_extent_icon(args.close_icon, glyph_size),
            glyph_size * 0.75,
        )
    } else {
        let fallback = icons::CLOSE_FALLBACK.to_string();
        GlyphBox::new(
            text.measure(&fallback, glyph_size).ceil() as i32,
            text.ink_extent(&fallback, glyph_size),
            glyph_size * 0.75,
        )
    };
    let icon = match (text.has_icons(), args.bar_icon()) {
        (true, Some(icon)) => Some(GlyphBox::new(
            text.measure_icon(icon, glyph_size).ceil() as i32,
            text.ink_extent_icon(icon, glyph_size),
            glyph_size * 0.75,
        )),
        _ => None,
    };

    Glyphs { icon, label, close }
}

/// Pointer state for the close button.
#[derive(Clone, Copy, Debug, Default)]
pub struct BarInput {
    pub hovered: bool,
    /// A press landed on the button and has not been released. Not the same as
    /// pressed: the pointer has to still be on the button.
    pub armed: bool,
}

impl BarInput {
    /// The button only reads as pressed while the pointer is still on it.
    fn pressed(self) -> bool {
        self.armed && self.hovered
    }
}

/// Where the bar's pieces sit, in buffer pixels. The `icon` / `label` / `close`
/// rects are *painted* boxes; the matching `*_origin` pairs are the draw origins
/// the fonts need to land their ink there. The `*_logical` rects are the same
/// boxes in logical surface coordinates, for pointer hit-testing.
#[derive(Clone, Copy, Debug)]
pub struct BarLayout {
    pub card: Rect,
    /// Target icon. Empty when no icon font is loaded, in which case the label
    /// starts at the card's padding and no gap is reserved for it.
    pub icon: Rect,
    pub label: Rect,
    pub button: Rect,
    /// The close glyph's painted box, inside the button.
    pub close: Rect,
    pub card_logical: Rect,
    pub button_logical: Rect,
    pub label_size: f32,
    pub glyph_size: f32,
    pub icon_origin: (i32, i32),
    pub label_origin: (i32, i32),
    pub close_origin: (i32, i32),
}

impl BarLayout {
    /// Every rect empty, for use before the first draw has run.
    pub const fn empty() -> Self {
        Self {
            card: Rect::new(0, 0, 0, 0),
            icon: Rect::new(0, 0, 0, 0),
            label: Rect::new(0, 0, 0, 0),
            button: Rect::new(0, 0, 0, 0),
            close: Rect::new(0, 0, 0, 0),
            card_logical: Rect::new(0, 0, 0, 0),
            button_logical: Rect::new(0, 0, 0, 0),
            label_size: 0.0,
            glyph_size: 0.0,
            icon_origin: (0, 0),
            label_origin: (0, 0),
            close_origin: (0, 0),
        }
    }
}

/// Lay the bar out for a surface of `w` x `h` buffer pixels at `ratio` buffer
/// pixels per logical pixel, from glyphs measured at that ratio.
pub fn layout(w: usize, h: usize, ratio: f64, glyphs: &Glyphs) -> BarLayout {
    // Logical -> buffer pixels, never rounding a dimension away entirely.
    let s = |v: f32| ((v * ratio as f32).round() as i32).max(1);
    let (pad, gap, margin) = (s(PADDING), s(GAP), s(MARGIN));
    let btn = s(BUTTON_SIZE);

    let (icon_w, icon_h, icon_ink_top) = glyphs
        .icon
        .map_or((0, 0, 0), |i| (i.width, i.height, i.ink_top));
    let (label_w, label_h, label_ink_top) = glyphs
        .label
        .map_or((0, 0, 0), |l| (l.width, l.height, l.ink_top));

    // The button is the tallest thing on the bar, so it drives the card height.
    let content_h = btn.max(label_h).max(icon_h);
    // Pieces run left to right with a style gap *between* them, so a piece that
    // is absent contributes neither width nor a gap — no reserved hole.
    let icon_gap = if glyphs.icon.is_some() { gap } else { 0 };
    let label_gap = if glyphs.label.is_some() { gap } else { 0 };
    let card_w = pad * 2 + icon_w + icon_gap + label_w + label_gap + btn;
    let card_h = pad * 2 + content_h;
    let card_x = (w as i32 - card_w) / 2;
    let card_y = h as i32 - margin - card_h;

    // Horizontally: icon, then label, then button, separated by the style gap.
    let icon_x = card_x + pad;
    let label_x = icon_x + icon_w + icon_gap;
    let button_x = label_x + label_w + label_gap;

    // Vertically: each piece is centred on its own ink inside the content row.
    let row_y = card_y + pad;
    let icon_ink_y = row_y + (content_h - icon_h) / 2;
    let label_ink_y = row_y + (content_h - label_h) / 2;
    let button_y = row_y + (content_h - btn) / 2;
    let close_ink_x = button_x + (btn - glyphs.close.width) / 2;
    let close_ink_y = button_y + (btn - glyphs.close.height) / 2;

    let to_logical = |v: i32| (v as f64 / ratio).round() as i32;
    BarLayout {
        card: Rect::new(card_x, card_y, card_w, card_h),
        icon: glyphs.icon.map_or(Rect::new(0, 0, 0, 0), |_| {
            Rect::new(icon_x, icon_ink_y, icon_w, icon_h)
        }),
        label: glyphs.label.map_or(Rect::new(0, 0, 0, 0), |_| {
            Rect::new(label_x, label_ink_y, label_w, label_h)
        }),
        button: Rect::new(button_x, button_y, btn, btn),
        close: Rect::new(
            close_ink_x,
            close_ink_y,
            glyphs.close.width,
            glyphs.close.height,
        ),
        card_logical: Rect::new(
            to_logical(card_x),
            to_logical(card_y),
            to_logical(card_w),
            to_logical(card_h),
        ),
        button_logical: Rect::new(
            to_logical(button_x),
            to_logical(button_y),
            to_logical(btn),
            to_logical(btn),
        ),
        label_size: LABEL_SIZE * ratio as f32,
        glyph_size: GLYPH_SIZE * ratio as f32,
        // The ink boxes above are where the pixels must land; the origins are
        // what the fonts have to be told to reach them.
        icon_origin: (icon_x, icon_ink_y - icon_ink_top),
        label_origin: (label_x, label_ink_y - label_ink_top),
        close_origin: (close_ink_x, close_ink_y - glyphs.close.ink_top),
    }
}

/// Paint the bar. `hovered` is whether the pointer is over the close button and
/// `pressed` whether the button is being held down.
pub fn draw(
    cv: &mut Canvas,
    text: &mut TextRenderer,
    l: &BarLayout,
    ratio: f64,
    args: &Args,
    input: BarInput,
) {
    let s = |v: f32| ((v * ratio as f32).round() as i32).max(1);
    // The user's corner-roundness preference, as the shell applies it to its own
    // controls. `ratio` is the other factor, and is already in `s`.
    let radius = |v: f32| s(v * args.corner_radius_scale);
    // `setCardStyle` takes a 0..1 fill opacity; the canvas wants 0..=255.
    let card_alpha = (args.card_alpha.clamp(0.0, 1.0) * 255.0).round() as u8;

    // Card.
    cv.fill_rounded(
        l.card,
        args.surface_variant,
        card_alpha,
        radius(CARD_RADIUS),
    );
    cv.stroke_rounded(l.card, args.outline, 0xff, s(BORDER), radius(CARD_RADIUS));

    // Target icon, in the same OnSurface role as the label. Omitted outright
    // when there is no icon font or no icon to draw (see `layout`).
    if let (Some(icon), false) = (args.bar_icon(), l.icon.is_empty()) {
        text.draw_icon(
            cv,
            icon,
            l.icon_origin.0,
            l.icon_origin.1,
            l.glyph_size,
            args.on_surface,
        );
    }

    // Target label. Omitted outright when the caller passed no `--label`; there
    // is no built-in text to fall back to.
    if let (Some(label), false) = (args.bar_label(), l.label.is_empty()) {
        text.draw_colored(
            cv,
            label,
            l.label_origin.0,
            l.label_origin.1,
            l.label_size,
            args.on_surface,
        );
    }

    // Close button: the native Outline variant's three states. Pressed wins over
    // hovered; both drop the border, as the native states do.
    let content = if input.pressed() {
        cv.fill_rounded(l.button, args.primary, 0xff, radius(BUTTON_RADIUS));
        args.on_primary
    } else if input.hovered {
        cv.fill_rounded(l.button, args.hover, 0xff, radius(BUTTON_RADIUS));
        args.on_hover
    } else {
        cv.fill_rounded(l.button, args.surface, 0xff, radius(BUTTON_RADIUS));
        cv.stroke_rounded(
            l.button,
            args.outline,
            0xff,
            s(BORDER),
            radius(BUTTON_RADIUS),
        );
        args.on_surface
    };

    // The close affordance: the configured glyph from the icon font, or a
    // multiplication sign from the text font when it failed to register. Both
    // are centred on their ink by `layout`, so only the glyph differs.
    if l.close.is_empty() {
        return;
    }
    if text.has_icons() {
        text.draw_icon(
            cv,
            args.close_icon,
            l.close_origin.0,
            l.close_origin.1,
            l.glyph_size,
            content,
        );
    } else {
        let fallback = icons::CLOSE_FALLBACK.to_string();
        text.draw_colored(
            cv,
            &fallback,
            l.close_origin.0,
            l.close_origin.1,
            l.glyph_size,
            content,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    fn shift(l: &BarLayout, dy: i32) -> BarLayout {
        let m = |r: Rect| r.translate(0, dy);
        let p = |(x, y): (i32, i32)| (x, y + dy);
        BarLayout {
            card: m(l.card),
            icon: m(l.icon),
            label: m(l.label),
            button: m(l.button),
            close: m(l.close),
            card_logical: l.card_logical,
            button_logical: l.button_logical,
            label_size: l.label_size,
            glyph_size: l.glyph_size,
            icon_origin: p(l.icon_origin),
            label_origin: p(l.label_origin),
            close_origin: p(l.close_origin),
        }
    }

    /// Draw the bar in all three button states over a grey stand-in desktop and
    /// assert the pixels follow — the card's rounded corners, each state's fill,
    /// the anti-aliasing along the arc, and that both glyphs really rasterize.
    #[test]
    fn render_bottom_bar_states() {
        let (w, h) = (720usize, 440usize);
        let mut px = vec![0u8; w * h * 4];
        for p in px.as_chunks_mut::<4>().0 {
            p.copy_from_slice(&[0x50, 0x50, 0x50, 0xff]);
        }

        // `--label` because there is no built-in text any more: the render test is
        // about a bar with a label, so it has to ask for one like the plugin does.
        let args = Args::parse_from(["screenshot-overlay", "--label", "Screenshot"]);

        let ratio = 2.0f64;
        let mut text = TextRenderer::new();
        // Register the embedded icon font exactly as `app::run` does, so this
        // covers the real icon path.
        let registered = text.load_icon_font(icons::FONT);
        let icon_source = if registered {
            format!(
                "{} ({} KiB embedded)",
                icons::FAMILY,
                icons::FONT.len() / 1024
            )
        } else {
            format!(
                "icon font failed to register, text glyph {}",
                icons::CLOSE_FALLBACK
            )
        };
        let glyphs = measure_glyphs(&mut text, ratio, &args);
        let base = layout(w, h, ratio, &glyphs);
        let states = [(false, false), (true, false), (true, true)];

        {
            let mut cv = Canvas::new(&mut px, w, h);
            for (i, &(hovered, pressed)) in states.iter().enumerate() {
                let l = shift(&base, -(i as i32) * 140);
                draw(
                    &mut cv,
                    &mut text,
                    &l,
                    ratio,
                    &args,
                    BarInput {
                        hovered,
                        armed: pressed,
                    },
                );
            }
        }

        let at = |x: i32, y: i32| {
            let o = (y as usize * w + x as usize) * 4;
            [px[o], px[o + 1], px[o + 2], px[o + 3]]
        };
        let bgra = |rgb: [u8; 3]| [rgb[2], rgb[1], rgb[0], 0xff];

        // Rounded card: the very corner stays background in every state.
        for (i, _) in states.iter().enumerate() {
            let l = shift(&base, -(i as i32) * 140);
            assert_eq!(
                at(l.card.x, l.card.y),
                [0x50, 0x50, 0x50, 0xff],
                "corner {i}"
            );
        }

        // The three Outline states fill the button differently. Sample just
        // inside the top edge, above where the glyph sits.
        let sample = |l: &BarLayout| at(l.button.x + l.button.w / 2, l.button.y + 5);
        assert_eq!(
            sample(&shift(&base, 0)),
            bgra(args.surface),
            "normal = Surface"
        );
        assert_eq!(
            sample(&shift(&base, -140)),
            bgra(args.hover),
            "hover = Hover"
        );
        assert_eq!(
            sample(&shift(&base, -280)),
            bgra(args.primary),
            "pressed = Primary"
        );

        // The corner band the anti-aliasing assertion below samples: the card's
        // top-left corner plus a little slack.
        let (cx, cy) = (base.card.x - 4, base.card.y - 4);
        let (cw, ch) = (48i32, 48i32);

        // The arc must be anti-aliased rather than a hard staircase: count the
        // pixels in the corner band that are a genuine blend of card fill and
        // background, i.e. neither extreme.
        let solid = over_grey(args.surface_variant, card_alpha(&args))[0];
        let mut aa = 0;
        for y in (base.card.y - 1)..(base.card.y + 26) {
            for x in (base.card.x - 1)..(base.card.x + 26) {
                if cx <= x && x < cx + cw && cy <= y && y < cy + ch {
                    let p = at(x, y);
                    if p[0] != 0x50 && p[0].abs_diff(solid) > 2 {
                        aa += 1;
                    }
                }
            }
        }
        assert!(
            aa > 8,
            "corner arc should be anti-aliased, got {aa} blended px"
        );

        // The close glyph must actually rasterize. A wrong font family or
        // codepoint yields no swash image at all and would otherwise pass
        // silently as an empty button — count ink inside the normal-state
        // button, clear of its border.
        let b = shift(&base, 0);
        let fill = bgra(args.surface);
        let mut ink = 0;
        for y in (b.button.y + 4)..(b.button.y + b.button.h - 4) {
            for x in (b.button.x + 4)..(b.button.x + b.button.w - 4) {
                if at(x, y) != fill {
                    ink += 1;
                }
            }
        }
        assert!(
            ink > 20,
            "close glyph should rasterize, got {ink} ink px ({icon_source})"
        );

        // The target icon before the label must rasterize too, and sit left of
        // the label. The card body is a translucent fill over the stand-in
        // background, so any pixel differing from that composite is glyph ink.
        if !base.icon.is_empty() {
            let card_body = over_grey(args.surface_variant, card_alpha(&args));
            let mut icon_ink = 0;
            for y in base.icon.y..base.icon.bottom() {
                for x in base.icon.x..base.icon.right() {
                    if at(x, y) != card_body {
                        icon_ink += 1;
                    }
                }
            }
            assert!(
                icon_ink > 20,
                "target icon should rasterize, got {icon_ink} ink px ({icon_source})"
            );
            assert!(
                base.icon.right() <= base.label.x,
                "icon {}..{} must not overlap the label at {}",
                base.icon.x,
                base.icon.right(),
                base.label.x
            );

            // The whole point of centring on ink: the icon and the label must
            // share an optical line even though they come from different fonts.
            // Line-box centring is what used to break this.
            let icon_centre = base.icon.y + base.icon.h / 2;
            let label_centre = base.label.y + base.label.h / 2;
            assert!(
                (icon_centre - label_centre).abs() <= 1,
                "icon centre {icon_centre} must match label centre {label_centre} \
                 (icon {}..{}, label {}..{})",
                base.icon.y,
                base.icon.bottom(),
                base.label.y,
                base.label.bottom()
            );
        }

        // The text-glyph fallback must render too. Whenever the icon bundle is
        // present that branch is unreachable, so exercise it on a fresh renderer
        // rather than leaving it untested until someone runs without Noctalia.
        {
            let mut plain = TextRenderer::new();
            assert!(!plain.has_icons(), "a fresh renderer starts without icons");
            let mut buf = vec![0u8; w * h * 4];
            for p in buf.as_chunks_mut::<4>().0 {
                p.copy_from_slice(&[0x50, 0x50, 0x50, 0xff]);
            }
            // No icon font: `layout` must drop the icon and reserve no gap.
            let l = layout(w, h, ratio, &measure_glyphs(&mut plain, ratio, &args));
            assert!(l.icon.is_empty(), "no icon font must mean no icon box");
            let mut cv = Canvas::new(&mut buf, w, h);
            draw(&mut cv, &mut plain, &l, ratio, &args, BarInput::default());

            let mut fallback_ink = 0;
            for y in (l.button.y + 4)..(l.button.y + l.button.h - 4) {
                for x in (l.button.x + 4)..(l.button.x + l.button.w - 4) {
                    let o = (y as usize * w + x as usize) * 4;
                    if [buf[o], buf[o + 1], buf[o + 2], buf[o + 3]] != fill {
                        fallback_ink += 1;
                    }
                }
            }
            assert!(
                fallback_ink > 20,
                "fallback glyph {} should rasterize too, got {fallback_ink} ink px",
                icons::CLOSE_FALLBACK
            );
        }
    }

    /// The card fill alpha as the canvas takes it, from the 0..=1 option.
    fn card_alpha(args: &Args) -> u8 {
        (args.card_alpha * 255.0).round() as u8
    }

    /// Non-default options must actually reach the canvas, not merely parse.
    /// Threading a config through four modules is exactly where a field gets
    /// dropped, and a silently ignored `--primary` would look like a theme bug.
    #[test]
    fn options_reach_the_canvas() {
        let (w, h) = (720usize, 440usize);
        let mut px = vec![0u8; w * h * 4];
        for p in px.as_chunks_mut::<4>().0 {
            p.copy_from_slice(&[0x50, 0x50, 0x50, 0xff]);
        }

        let args = Args::parse_from([
            "screenshot-overlay",
            "--label",
            "Hi",
            "--primary",
            "#ff0000",
            "--surface-variant",
            "#0000ff",
            "--card-alpha",
            "1.0",
        ]);

        let ratio = 2.0f64;
        let mut text = TextRenderer::new();
        text.load_icon_font(icons::FONT);
        let base = layout(w, h, ratio, &measure_glyphs(&mut text, ratio, &args));
        {
            let mut cv = Canvas::new(&mut px, w, h);
            // Pressed, so the button carries `--primary`.
            draw(
                &mut cv,
                &mut text,
                &base,
                ratio,
                &args,
                BarInput {
                    hovered: true,
                    armed: true,
                },
            );
        }

        let at = |x: i32, y: i32| {
            let o = (y as usize * w + x as usize) * 4;
            [px[o], px[o + 1], px[o + 2], px[o + 3]]
        };

        // Pressed button fill, sampled clear of the glyph and the border.
        let pressed = at(base.button.x + base.button.w / 2, base.button.y + 5);
        assert_eq!(
            pressed,
            [0x00, 0x00, 0xff, 0xff],
            "--primary did not reach the button"
        );

        // Card fill at alpha 1, so it is exact. Sampled mid-card, clear of the
        // rounded corners and of the 1px (scaled) outline.
        let card = at(base.card.x + base.card.w / 2, base.card.y + 6);
        assert_eq!(
            card,
            [0xff, 0x00, 0x00, 0xff],
            "--surface-variant did not reach the card"
        );

        // The label really reaches the layout: a longer one must widen the card,
        // and dropping it altogether must narrow the card again. Neither end is a
        // default — there is no built-in label to compare against any more.
        let long = Args::parse_from(["screenshot-overlay", "--label", "A much longer label"]);
        let none = Args::parse_from(["screenshot-overlay"]);
        let short_w = base.card.w;
        let long_w = layout(w, h, ratio, &measure_glyphs(&mut text, ratio, &long))
            .card
            .w;
        let none_w = layout(w, h, ratio, &measure_glyphs(&mut text, ratio, &none))
            .card
            .w;
        assert!(
            short_w < long_w,
            "a longer --label did not widen the card: {short_w} vs {long_w}"
        );
        assert!(
            none_w < short_w,
            "dropping --label did not narrow the card: {none_w} vs {short_w}"
        );
    }

    /// `--corner-radius-scale` has to reach the canvas as well, for the same
    /// reason the colours do: it is the one metric Noctalia lets the user move
    /// (`shell.corner_radius_scale`), so a dropped one shows up as a bar whose
    /// corners disagree with the shell's own controls.
    ///
    /// Measured as *coverage*, not as one pixel: a bigger radius cuts more of the
    /// card's top-left corner away, so counting background pixels in a fixed box
    /// there goes up. A single sample would depend on sub-pixel AA.
    #[test]
    fn the_corner_radius_scale_reaches_the_canvas() {
        let (w, h) = (720usize, 440usize);
        let ratio = 2.0f64;

        // Background pixels in the card's top-left corner box. 20 px keeps the
        // box clear of the icon, which starts at `card.x + padding`.
        let corner_gap = |scale: &str| {
            let args = Args::parse_from([
                "screenshot-overlay",
                "--card-alpha",
                "1.0",
                "--corner-radius-scale",
                scale,
            ]);
            let mut px = vec![0u8; w * h * 4];
            for p in px.as_chunks_mut::<4>().0 {
                p.copy_from_slice(&[0x50, 0x50, 0x50, 0xff]);
            }

            let mut text = TextRenderer::new();
            text.load_icon_font(icons::FONT);
            let l = layout(w, h, ratio, &measure_glyphs(&mut text, ratio, &args));
            {
                let mut cv = Canvas::new(&mut px, w, h);
                draw(&mut cv, &mut text, &l, ratio, &args, BarInput::default());
            }

            let mut gap = 0;
            for y in l.card.y..(l.card.y + 20) {
                for x in l.card.x..(l.card.x + 20) {
                    let o = (y as usize * w + x as usize) * 4;
                    if [px[o], px[o + 1], px[o + 2], px[o + 3]] == [0x50, 0x50, 0x50, 0xff] {
                        gap += 1;
                    }
                }
            }
            // In the card's top padding, which the button and its glyph cannot
            // reach — with no label the button sits at the card's own centre.
            let o = ((l.card.y + 6) as usize * w + (l.card.x + l.card.w / 2) as usize) * 4;
            assert_eq!(
                [px[o], px[o + 1], px[o + 2], px[o + 3]],
                [0x23, 0x20, 0x1e, 0xff],
                "the card did not draw at all"
            );
            gap
        };

        // `shell.corner_radius_scale`'s range: 0 flattens the corners to the
        // 1px minimum radius `scaled` clamps to, 2 doubles them.
        let square = corner_gap("0");
        let normal = corner_gap("1");
        let round = corner_gap("2");
        assert_eq!(square, 0, "scale 0 must leave the corner square");
        assert!(
            normal < round,
            "a larger --corner-radius-scale must cut more of the corner away: \
             {normal} vs {round} px"
        );
    }

    /// `rgb` at alpha `a` composited over the grey stand-in background, as BGRA.
    fn over_grey(rgb: [u8; 3], a: u8) -> [u8; 4] {
        let f = |c: u8| ((c as u16 * a as u16 + 0x50u16 * (255 - a as u16)) / 255) as u8;
        [f(rgb[2]), f(rgb[1]), f(rgb[0]), 0xff]
    }

    /// The icon names the bar is asked to draw must actually rasterize out of the
    /// vendored font. A name that resolves to a codepoint with no glyph draws
    /// nothing at all, which would otherwise pass silently as an empty box.
    /// `--target-icon` is the caller's now, so these are the names the plugin's
    /// own target table uses — plus `x`, which is what `--close-icon` defaults to.
    ///
    /// **`x`, not Noctalia's `close`.** Noctalia addresses this glyph through a
    /// hand-curated alias table of its own (`glyph_registry.cpp`), and `close` is
    /// one of those aliases; the vendored `tabler.json` holds Tabler's canonical
    /// names, so `close` does not resolve here and never has. A caller that wants
    /// the close glyph asks for the Tabler name the alias points at.
    #[test]
    fn the_plugin_icons_have_ink() {
        let mut text = TextRenderer::new();
        assert!(
            text.load_icon_font(icons::FONT),
            "the embedded font must register"
        );

        for name in [
            "screenshot",
            "text-recognition",
            "photo-search",
            "camera",
            "camera-spark",
            "x",
        ] {
            let glyph = crate::icons::resolve(name)
                .unwrap_or_else(|e| panic!("{name} does not resolve: {e}"));
            let Some((w, h)) = text.ink_extent_icon(glyph, 20.0) else {
                panic!("{name} (U+{:04X}) has no ink", glyph as u32);
            };
            assert!(w > 0 && h > 0, "{name} ink is {w}x{h}");
            assert!(
                text.measure_icon(glyph, 20.0) > 0.0,
                "{name} has no advance"
            );
        }

        // The alias really is absent, so the comment above cannot rot.
        assert!(
            crate::icons::resolve("close").is_err(),
            "`close` is Noctalia's alias, not a name in the vendored Tabler table"
        );
    }
}
