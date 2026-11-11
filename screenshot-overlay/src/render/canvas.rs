//! A small drawing surface over a BGRA8888 buffer (wl_shm `Argb8888`).
//!
//! This is the only place that knows about strides, byte order and clipping.
//! Everything else draws in terms of [`Rect`] and RGB colours.

/// A rectangle in buffer pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    pub const fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Self { x, y, w, h }
    }

    pub const fn right(self) -> i32 {
        self.x + self.w
    }

    pub const fn bottom(self) -> i32 {
        self.y + self.h
    }

    pub const fn is_empty(self) -> bool {
        self.w <= 0 || self.h <= 0
    }

    pub const fn translate(self, dx: i32, dy: i32) -> Self {
        Self::new(self.x + dx, self.y + dy, self.w, self.h)
    }

    pub fn intersect(self, o: Rect) -> Self {
        // The origin is clamped too, not just the size. A rect that starts past
        // the right or bottom edge used to keep its out-of-bounds origin while
        // collapsing to zero size, and every caller that indexes `px` by that
        // origin (blend, copy) then panicked on an empty rect. An empty result
        // must also be a *locatable* one.
        let x0 = self.x.max(o.x).min(o.right());
        let y0 = self.y.max(o.y).min(o.bottom());
        let x1 = self.right().min(o.right());
        let y1 = self.bottom().min(o.bottom());
        Self::new(x0, y0, (x1 - x0).max(0), (y1 - y0).max(0))
    }

    /// Clip to the rect `(0, 0, w, h)`.
    pub fn clamp(self, w: i32, h: i32) -> Self {
        self.intersect(Self::new(0, 0, w, h))
    }

    /// Whether the point `(px, py)` falls inside, half-open on the far edges.
    pub fn contains(self, px: f64, py: f64) -> bool {
        !self.is_empty()
            && px >= self.x as f64
            && px < self.right() as f64
            && py >= self.y as f64
            && py < self.bottom() as f64
    }
}

/// src-over blend of an RGB colour with alpha `a` onto one BGRA pixel.
#[inline]
fn over(px: &mut [u8], rgb: [u8; 3], a: u16) {
    let inv = 255 - a;
    px[0] = ((px[0] as u16 * inv + rgb[2] as u16 * a) / 255) as u8;
    px[1] = ((px[1] as u16 * inv + rgb[1] as u16 * a) / 255) as u8;
    px[2] = ((px[2] as u16 * inv + rgb[0] as u16 * a) / 255) as u8;
}

/// Signed distance from `(px, py)` to a rounded rect of `w` x `h` anchored at
/// the origin, with a uniform corner `radius`. Negative inside.
///
/// This is the same SDF the native shell's rect shader uses
/// (`src/render/programs/rect_program.cpp`), evaluated on the CPU so the
/// corners get the same analytic anti-aliasing the shell's own controls have.
#[inline]
fn rounded_sdf(px: f32, py: f32, w: f32, h: f32, radius: f32) -> f32 {
    let r = radius.min(w * 0.5).min(h * 0.5).max(0.0);
    let qx = (px - w * 0.5).abs() - (w * 0.5 - r);
    let qy = (py - h * 0.5).abs() - (h * 0.5 - r);
    let outside = (qx.max(0.0) * qx.max(0.0) + qy.max(0.0) * qy.max(0.0)).sqrt();
    outside + qx.max(qy).min(0.0) - r
}

/// Coverage of the pixel centred on `(px, py)` by that rounded rect, in `0..=1`.
/// Roughly one pixel of falloff, matching the shader's `coverage_for`.
#[inline]
fn rounded_coverage(px: f32, py: f32, w: f32, h: f32, radius: f32) -> f32 {
    if w <= 0.0 || h <= 0.0 {
        return 0.0;
    }
    (0.5 - rounded_sdf(px, py, w, h, radius)).clamp(0.0, 1.0)
}

/// A mutable view of a BGRA8888 buffer.
///
/// A `Canvas` may be a sub-view ([`Canvas::sub`]) of a larger one; drawing
/// coordinates are always local to the view and clipped to it. `copy` reads
/// from a full-size source with the same stride.
pub struct Canvas<'a> {
    px: &'a mut [u8],
    stride: usize,
    w: usize,
    h: usize,
    /// Origin of this view inside the root canvas, in pixels.
    ox: i32,
    oy: i32,
}

impl<'a> Canvas<'a> {
    pub fn new(px: &'a mut [u8], w: usize, h: usize) -> Self {
        let stride = w * 4;
        // Trust the buffer over the requested height. The surface height and the
        // shm buffer can disagree by a row when the scale rounds up, and a canvas
        // one row taller than its buffer panics the moment anything draws on the
        // last row — the size label on a selection that reaches the bottom edge
        // is the reliable reproducer.
        let h = if stride == 0 {
            0
        } else {
            h.min(px.len() / stride)
        };
        debug_assert!(px.len() >= stride * h);
        Self {
            px,
            stride,
            w,
            h,
            ox: 0,
            oy: 0,
        }
    }

    pub fn w(&self) -> usize {
        self.w
    }

    pub fn h(&self) -> usize {
        self.h
    }

    pub fn bounds(&self) -> Rect {
        Rect::new(0, 0, self.w as i32, self.h as i32)
    }

    /// A clipped sub-view at `r`; its local origin is `(0, 0)`.
    pub fn sub(&mut self, r: Rect) -> Canvas<'_> {
        let r = r.clamp(self.w as i32, self.h as i32);
        let off = r.y as usize * self.stride + r.x as usize * 4;
        Canvas {
            px: &mut self.px[off..],
            stride: self.stride,
            w: r.w as usize,
            h: r.h as usize,
            ox: self.ox + r.x,
            oy: self.oy + r.y,
        }
    }

    /// Blend an RGB colour (alpha 0..=255) over `r`, clipped to the view.
    pub fn blend(&mut self, r: Rect, rgb: [u8; 3], a: u8) {
        let r = r.clamp(self.w as i32, self.h as i32);
        for y in 0..r.h as usize {
            let row = &mut self.px[(y + r.y as usize) * self.stride + r.x as usize * 4..]
                [..r.w as usize * 4];
            for px in row.as_chunks_mut::<4>().0 {
                over(px, rgb, a as u16);
            }
        }
    }

    /// Blend one pixel, clipped.
    pub fn blend_px(&mut self, x: i32, y: i32, rgb: [u8; 3], a: u8) {
        if x < 0 || y < 0 || x >= self.w as i32 || y >= self.h as i32 {
            return;
        }
        let i = y as usize * self.stride + x as usize * 4;
        over(&mut self.px[i..i + 4], rgb, a as u16);
    }

    /// src-over one premultiplied BGRA pixel (colour glyphs), clipped.
    pub fn blend_premul_px(&mut self, x: i32, y: i32, src: [u8; 4]) {
        if x < 0 || y < 0 || x >= self.w as i32 || y >= self.h as i32 {
            return;
        }
        let i = y as usize * self.stride + x as usize * 4;
        let inv = 255 - src[3] as u16;
        for (dst, s) in self.px[i..i + 3].iter_mut().zip(&src[..3]) {
            *dst = (*s as u16 + *dst as u16 * inv / 255).min(255) as u8;
        }
    }

    /// Write an opaque BGRA pixel, clipped.
    pub fn set_px(&mut self, x: i32, y: i32, bgra: [u8; 4]) {
        if x < 0 || y < 0 || x >= self.w as i32 || y >= self.h as i32 {
            return;
        }
        let i = y as usize * self.stride + x as usize * 4;
        self.px[i..i + 4].copy_from_slice(&bgra);
    }

    /// Fill `r` with an opaque BGRA colour, clipped.
    pub fn fill(&mut self, r: Rect, bgra: [u8; 4]) {
        let r = r.clamp(self.w as i32, self.h as i32);
        for y in 0..r.h as usize {
            let row = &mut self.px[(y + r.y as usize) * self.stride + r.x as usize * 4..]
                [..r.w as usize * 4];
            for px in row.as_chunks_mut::<4>().0 {
                px.copy_from_slice(&bgra);
            }
        }
    }

    /// Blend a border of thickness `t` along the inside edges of `r`.
    ///
    /// Every pixel is blended **once**. Drawing the four edges as four
    /// overlapping rects — which is what this used to do, the verticals running
    /// the full height — blends each corner square twice, and a double blend is
    /// only invisible when the colour is opaque. At `0xaa` over a dimmed backdrop
    /// it lands on `0.89·src + 0.11·dst` instead of `0.67·src + 0.33·dst`, which
    /// is exactly the bright dot the window highlight's border had at its
    /// corners.
    pub fn stroke(&mut self, r: Rect, rgb: [u8; 3], a: u8, t: i32) {
        let t = t.max(1);
        if r.is_empty() {
            return;
        }
        // Too thin to have an inside: the border is the whole rect.
        if r.h <= 2 * t || r.w <= 2 * t {
            self.blend(r, rgb, a);
            return;
        }
        // The horizontal spans take the full width, and the verticals between
        // them stop short of both, so no pixel is covered by two of the four.
        self.blend(Rect::new(r.x, r.y, r.w, t), rgb, a);
        self.blend(Rect::new(r.x, r.bottom() - t, r.w, t), rgb, a);
        let mid_y = r.y + t;
        let mid_h = r.h - 2 * t;
        self.blend(Rect::new(r.x, mid_y, t, mid_h), rgb, a);
        self.blend(Rect::new(r.right() - t, mid_y, t, mid_h), rgb, a);
    }

    /// Blend an RGB colour over a rounded rect, anti-aliased along the arcs.
    /// `radius == 0` degrades to a plain [`Canvas::blend`].
    pub fn fill_rounded(&mut self, r: Rect, rgb: [u8; 3], a: u8, radius: i32) {
        let (w, h) = (r.w as f32, r.h as f32);
        let rad = (radius.max(0) as f32).min(w * 0.5).min(h * 0.5);
        if r.is_empty() {
            return;
        }
        if rad <= 0.0 {
            self.blend(r, rgb, a);
            return;
        }
        // One pixel of margin, so partially covered edge pixels are included.
        let e = Rect::new(r.x - 1, r.y - 1, r.w + 2, r.h + 2).clamp(self.w as i32, self.h as i32);
        for y in e.y..e.bottom() {
            for x in e.x..e.right() {
                let px = x as f32 + 0.5 - r.x as f32;
                let py = y as f32 + 0.5 - r.y as f32;
                let cov = rounded_coverage(px, py, w, h, rad);
                if cov > 0.0 {
                    self.blend_px(x, y, rgb, (a as f32 * cov).round() as u8);
                }
            }
        }
    }

    /// Blend a rounded border of thickness `t` along the inside edges of `r`,
    /// anti-aliased on both the outer and inner arcs. `radius == 0` degrades to
    /// a plain [`Canvas::stroke`].
    pub fn stroke_rounded(&mut self, r: Rect, rgb: [u8; 3], a: u8, t: i32, radius: i32) {
        let (w, h) = (r.w as f32, r.h as f32);
        let rad = (radius.max(0) as f32).min(w * 0.5).min(h * 0.5);
        if r.is_empty() {
            return;
        }
        let t = t.max(1);
        if rad <= 0.0 {
            self.stroke(r, rgb, a, t);
            return;
        }
        let tf = t as f32;
        let e = Rect::new(r.x - 1, r.y - 1, r.w + 2, r.h + 2).clamp(self.w as i32, self.h as i32);
        for y in e.y..e.bottom() {
            for x in e.x..e.right() {
                let px = x as f32 + 0.5 - r.x as f32;
                let py = y as f32 + 0.5 - r.y as f32;
                // Ring: inside the outer rect, outside the rect inset by `t`.
                let outer = rounded_coverage(px, py, w, h, rad);
                if outer <= 0.0 {
                    continue;
                }
                let inner = rounded_coverage(
                    px - tf,
                    py - tf,
                    w - 2.0 * tf,
                    h - 2.0 * tf,
                    (rad - tf).max(0.0),
                );
                let cov = outer * (1.0 - inner);
                if cov > 0.0 {
                    self.blend_px(x, y, rgb, (a as f32 * cov).round() as u8);
                }
            }
        }
    }

    /// Copy a region from a full-size source image that shares this stride.
    pub fn copy(&mut self, r: Rect, src: &[u8]) {
        let r = r.clamp(self.w as i32, self.h as i32);
        for y in 0..r.h as usize {
            let dst = (y + r.y as usize) * self.stride + r.x as usize * 4;
            let sy = self.oy as usize + r.y as usize + y;
            let sx = self.ox as usize + r.x as usize;
            let src_off = sy * self.stride + sx * 4;
            self.px[dst..dst + r.w as usize * 4]
                .copy_from_slice(&src[src_off..src_off + r.w as usize * 4]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const W: usize = 24;
    const H: usize = 16;

    fn pixel(px: &[u8], x: i32, y: i32) -> [u8; 4] {
        let o = (y as usize * W + x as usize) * 4;
        px[o..o + 4].try_into().unwrap()
    }

    /// A translucent stroke must look the same at the corners as along the edges.
    /// Blending the four edges as four overlapping rects gave each corner square
    /// two blends, which at `0xaa` over a grey lands noticeably lighter — the
    /// window highlight's border, in the actual frame.
    #[test]
    fn an_out_of_bounds_rect_collapses_inside_the_canvas() {
        // The origin has to move in too: a zero-width rect whose x is past the
        // edge used to keep that x and panic the row slice in `blend`.
        let (w, h) = (8usize, 4usize);
        let mut px = vec![0u8; w * h * 4];
        let mut cv = Canvas::new(&mut px, w, h);
        let off = Rect::new(w as i32 + 1, 1, 5, 2).clamp(w as i32, h as i32);
        assert_eq!((off.x, off.w), (w as i32, 0));
        cv.blend(Rect::new(w as i32 + 1, 1, 5, 2), [255, 0, 0], 0xff);
        cv.blend(Rect::new(1, h as i32 + 1, 2, 5), [0, 255, 0], 0xff);
    }

    #[test]
    fn height_beyond_the_buffer_is_clamped() {
        // Declaration one row taller than the buffer: the missing row must be
        // ignored rather than indexed. This is the touchpad-to-the-corner panic.
        let (w, h) = (4usize, 2usize);
        let mut px = vec![0u8; w * h * 4];
        let mut cv = Canvas::new(&mut px, w, h + 1);
        assert_eq!(cv.h(), h);
        cv.blend(Rect::new(0, h as i32 - 1, w as i32, 2), [255, 0, 0], 0xff);
        cv.blend_px(w as i32 - 1, h as i32 - 1, [0, 255, 0], 0xff);
    }

    #[test]
    fn a_translucent_stroke_is_even_all_the_way_round() {
        let r = Rect::new(4, 3, 16, 10);
        let mut px = vec![0x40u8; W * H * 4];
        {
            let mut cv = Canvas::new(&mut px, W, H);
            cv.stroke(r, [0xff, 0xff, 0xff], 0xaa, 2);
        }

        let top = pixel(&px, r.x + r.w / 2, r.y);
        let left = pixel(&px, r.x, r.y + r.h / 2);
        assert_eq!(left, top, "the left edge must match the top edge");

        for (x, y) in [
            (r.x, r.y),
            (r.right() - 1, r.y),
            (r.x, r.bottom() - 1),
            (r.right() - 1, r.bottom() - 1),
        ] {
            assert_eq!(pixel(&px, x, y), top, "corner ({x}, {y}) is blended twice");
        }

        let inside = pixel(&px, r.x + r.w / 2, r.y + r.h / 2);
        assert_ne!(inside, top, "the stroke must not fill the interior");
    }

    /// A rect with no room for an inside is all border — and still blended once,
    /// not once per edge.
    #[test]
    fn a_stroke_thicker_than_its_rect_is_drawn_once() {
        let r = Rect::new(2, 2, 3, 3);

        let mut once = vec![0u8; W * H * 4];
        {
            let mut cv = Canvas::new(&mut once, W, H);
            cv.blend(r, [0xff, 0xff, 0xff], 0x80);
        }

        let mut stroked = vec![0u8; W * H * 4];
        {
            let mut cv = Canvas::new(&mut stroked, W, H);
            cv.stroke(r, [0xff, 0xff, 0xff], 0x80, 4);
        }

        for y in r.y..r.bottom() {
            for x in r.x..r.right() {
                assert_eq!(
                    pixel(&stroked, x, y),
                    pixel(&once, x, y),
                    "({x}, {y}) was blended more than once"
                );
            }
        }
    }
}
