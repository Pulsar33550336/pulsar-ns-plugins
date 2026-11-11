//! Frame composition: frozen base image + window highlights + selection +
//! crosshair, drawn through a [`Canvas`].
//!
//! Every colour here comes from [`RenderCtx`], which the overlay fills in from
//! the command line. The values are plain RGB — resolving Noctalia colour roles
//! against the theme is the plugin's job, not this crate's (see `config.rs`).

use image::RgbaImage;

use super::canvas::{Canvas, Rect};

/// How the frozen frame is darkened outside the selection. Baked into [`Base`]
/// at capture time because the dimmed copy is precomputed, not blended per frame.
#[derive(Clone, Copy, Debug)]
pub struct Dim {
    pub rgb: [u8; 3],
    pub alpha: f32,
}

/// The two thin bands occupied by a crosshair centred at `p` (buffer pixels).
pub fn bands(p: (i32, i32), w: i32, h: i32, t: i32) -> [Rect; 2] {
    let (px, py) = p;
    [Rect::new(px, 0, t, h), Rect::new(0, py, w, t)]
}

/// Buffer geometry for the current `SSR_MODE`: (width, height, buffer_scale,
/// call set_buffer_scale). The default is a physical-resolution buffer tagged
/// with the output scale, so the frozen screenshot maps 1:1 and stays crisp.
pub fn buffer_dims(logical_w: u32, logical_h: u32, scale: i32) -> (u32, u32, i32, bool) {
    let s = scale.max(1) as u32;
    let lw = logical_w.max(1);
    let lh = logical_h.max(1);
    match std::env::var("SSR_MODE").unwrap_or_default().as_str() {
        "logical_scale" => (lw, lh, scale.max(1), true),
        "logical_noscale" => (lw, lh, 1, false),
        "phys_noscale" => (lw * s, lh * s, 1, false),
        _ => (lw * s, lh * s, scale.max(1), true),
    }
}

/// src-over `rgb` at `alpha` onto one BGRA pixel.
fn dim_pixel(bgra: [u8; 4], rgb: [u8; 3], alpha: f32) -> [u8; 4] {
    let a = (alpha.clamp(0.0, 1.0) * 255.0).round() as u16;
    let inv = 255 - a;
    // BGRA in, BGRA out: rgb is RGBA-ordered, the buffer is not.
    let mix = |src: u8, i: usize| ((src as u16 * inv + rgb[i] as u16 * a) / 255) as u8;
    [mix(bgra[0], 2), mix(bgra[1], 1), mix(bgra[2], 0), bgra[3]]
}

/// Precomputed full-screen BGRA images: the frozen frame and a dimmed copy, so a
/// full repaint is mostly memcpy instead of per-pixel blending.
pub struct Base {
    raw: Vec<u8>,
    dim: Vec<u8>,
    w: usize,
    h: usize,
}

impl Base {
    /// Build from an RGBA screenshot.
    pub fn from_rgba(img: &RgbaImage, dim: Dim) -> Self {
        let (w, h) = (img.width() as usize, img.height() as usize);
        let mut raw = vec![0u8; w * h * 4];
        for (i, px) in img.as_raw().as_chunks::<4>().0.iter().enumerate() {
            raw[i * 4..i * 4 + 4].copy_from_slice(&[px[2], px[1], px[0], px[3]]);
        }
        Self::from_raw(raw, w, h, dim)
    }

    /// Build from a BGRA buffer; the dimmed copy is derived.
    pub fn from_raw(raw: Vec<u8>, w: usize, h: usize, dim: Dim) -> Self {
        let dimmed = raw
            .as_chunks::<4>()
            .0
            .iter()
            .flat_map(|p| dim_pixel([p[0], p[1], p[2], p[3]], dim.rgb, dim.alpha))
            .collect();
        Self {
            raw,
            dim: dimmed,
            w,
            h,
        }
    }

    pub fn w(&self) -> usize {
        self.w
    }

    pub fn h(&self) -> usize {
        self.h
    }

    pub fn raw(&self) -> &[u8] {
        &self.raw
    }

    pub fn dim(&self) -> &[u8] {
        &self.dim
    }

    /// One pixel from the raw or dimmed image.
    pub fn pixel(&self, x: usize, y: usize, dimmed: bool) -> [u8; 4] {
        let src = if dimmed { &self.dim } else { &self.raw };
        let o = (y * self.w + x) * 4;
        [src[o], src[o + 1], src[o + 2], src[o + 3]]
    }
}

/// A window highlight to draw over the frozen frame.
pub struct WinOverlay {
    pub rect: Rect,
    pub hovered: bool,
}

/// Inputs for [`compose`].
pub struct RenderCtx<'a> {
    pub base: Option<&'a Base>,
    pub w: usize,
    pub h: usize,
    /// Buffer pixels per logical pixel (for border widths).
    pub unit: i32,
    pub thickness: i32,
    pub border: i32,
    pub cross: Option<(i32, i32)>,
    pub selection: Option<Rect>,
    /// Window highlights, drawn while not dragging.
    pub windows: &'a [WinOverlay],
    /// Selection border, crosshair and drag size label.
    pub selection_color: [u8; 3],
    /// Window highlight rectangles.
    pub window_color: [u8; 3],
}

/// Draw the frame, restricted to `damage` (buffer pixels).
pub fn compose(canvas: &mut Canvas, damage: Rect, ctx: &RenderCtx) {
    let damage = damage.clamp(ctx.w as i32, ctx.h as i32);
    if damage.is_empty() {
        return;
    }
    let mut c = canvas.sub(damage);
    let local = |r: Rect| r.translate(-damage.x, -damage.y);
    let full = c.bounds();

    // Base: dimmed everywhere, raw inside the selection.
    match ctx.base {
        Some(b) if b.w() == ctx.w && b.h() == ctx.h => {
            c.copy(full, b.dim());
            if let Some(sel) = ctx.selection {
                c.copy(local(sel), b.raw());
            }
        }
        Some(b) => copy_scaled(&mut c, b, damage, ctx),
        None => c.fill(full, [0, 0, 0, 0]),
    }

    // Window highlights (hidden while dragging; the caller passes an empty slice).
    for win in ctx.windows {
        let r = local(win.rect);
        if win.hovered {
            c.blend(r, ctx.window_color, 0x22);
            c.stroke(r, ctx.window_color, 0xaa, 3 * ctx.unit);
        } else {
            c.stroke(r, ctx.window_color, 0x55, ctx.unit);
        }
    }

    // Selection border.
    if let Some(sel) = ctx.selection {
        c.stroke(local(sel), ctx.selection_color, 0xff, ctx.border);
    }

    // Crosshair. The two bands cross in one `thickness`-sized square, and both
    // are drawn at `0x66`: blending that square twice would put a brighter dot
    // exactly where the user is aiming, so the horizontal band is split around
    // the vertical one and no pixel is covered by both.
    if let Some((px, py)) = ctx.cross {
        let (px, py) = (px - damage.x, py - damage.y);
        let (w, h) = (c.w() as i32, c.h() as i32);
        let t = ctx.thickness;
        c.blend(Rect::new(px, 0, t, h), ctx.selection_color, 0x66);
        c.blend(Rect::new(0, py, px, t), ctx.selection_color, 0x66);
        c.blend(
            Rect::new(px + t, py, w - (px + t), t),
            ctx.selection_color,
            0x66,
        );
    }
}

/// Nearest-neighbour base copy for non-1:1 buffer geometry (diagnostic modes).
fn copy_scaled(c: &mut Canvas, b: &Base, damage: Rect, ctx: &RenderCtx) {
    for y in 0..c.h() as i32 {
        for x in 0..c.w() as i32 {
            let px = damage.x + x;
            let py = damage.y + y;
            let in_sel = ctx
                .selection
                .is_some_and(|s| px >= s.x && px < s.right() && py >= s.y && py < s.bottom());
            let sx = (px.max(0) as usize * b.w() / ctx.w.max(1)).min(b.w() - 1);
            let sy = (py.max(0) as usize * b.h() / ctx.h.max(1)).min(b.h() - 1);
            c.set_px(x, y, b.pixel(sx, sy, !in_sel));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const W: usize = 48;
    const H: usize = 32;

    fn pixel(px: &[u8], x: i32, y: i32) -> [u8; 4] {
        let o = (y as usize * W + x as usize) * 4;
        px[o..o + 4].try_into().unwrap()
    }

    /// A uniform mid-grey frame, so the only thing that varies between two
    /// samples is what `compose` drew on top of them.
    fn backdrop() -> Base {
        Base::from_raw(
            vec![0x80u8; W * H * 4],
            W,
            H,
            Dim {
                rgb: [0, 0, 0],
                alpha: 0.5,
            },
        )
    }

    fn frame(base: &Base, windows: &[WinOverlay], cross: Option<(i32, i32)>) -> Vec<u8> {
        let ctx = RenderCtx {
            base: Some(base),
            w: W,
            h: H,
            unit: 2,
            thickness: 2,
            border: 2,
            cross,
            selection: None,
            windows,
            selection_color: [0xcc, 0xcc, 0xcc],
            window_color: [0xff, 0xff, 0xff],
        };
        let mut px = vec![0u8; W * H * 4];
        let mut cv = Canvas::new(&mut px, W, H);
        compose(&mut cv, Rect::new(0, 0, W as i32, H as i32), &ctx);
        px
    }

    /// The window highlight's border is translucent, so a corner that got two
    /// blends showed as a bright dot at each corner of the frame.
    #[test]
    fn a_window_highlight_has_no_brighter_corners() {
        let base = backdrop();
        let r = Rect::new(6, 5, 30, 20);

        for hovered in [false, true] {
            let px = frame(&base, &[WinOverlay { rect: r, hovered }], None);
            let edge = pixel(&px, r.x + r.w / 2, r.y);
            for (x, y) in [
                (r.x, r.y),
                (r.right() - 1, r.y),
                (r.x, r.bottom() - 1),
                (r.right() - 1, r.bottom() - 1),
            ] {
                assert_eq!(
                    pixel(&px, x, y),
                    edge,
                    "hovered={hovered}: corner ({x}, {y}) is brighter than the edge"
                );
            }
        }
    }

    /// The crosshair's two bands used to be drawn full-length, so the square
    /// where they cross took two `0x66` blends — a brighter dot at exactly the
    /// point the user is aiming with.
    #[test]
    fn the_crosshair_has_no_brighter_crossing() {
        let base = backdrop();
        let (cx, cy) = (20, 12);
        let px = frame(&base, &[], Some((cx, cy)));

        let on_vertical = pixel(&px, cx, 0);
        assert_eq!(
            pixel(&px, cx, cy),
            on_vertical,
            "the crossing is blended twice"
        );
        assert_eq!(
            pixel(&px, 0, cy),
            on_vertical,
            "the two bands must have the same weight"
        );
    }
}
