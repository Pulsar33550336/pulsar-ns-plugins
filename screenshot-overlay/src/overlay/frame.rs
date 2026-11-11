//! One frame, drawn: the whole composition path, with no Wayland in it.
//!
//! This exists because the frame has two callers. `Overlay::try_draw` draws it
//! into an acquired shm buffer; `benches/render.rs` draws it into a plain `Vec`.
//! Before this module the benchmark re-implemented the composition by calling
//! `compose` and `bottombar::draw` in what it hoped was the right order — a copy
//! that could silently stop matching the real frame, and therefore a benchmark
//! that could quietly stop measuring anything.
//!
//! So the drawing order lives here and nowhere else: the frozen frame and every
//! overlay stroke, then the drag size label, then the bar on top. [`Frame`] is
//! the state a frame needs, and it holds no Wayland types, which is what makes
//! it constructible offscreen.

use crate::cli::Args;
use crate::render::{compose, Base, Canvas, Rect, RenderCtx, TextRenderer, WinOverlay};

use super::bottombar::{self, BarInput, BarLayout};
use super::selection::Selection;

/// Everything one frame draws.
pub struct Frame<'a> {
    /// Buffer size in pixels.
    pub w: usize,
    pub h: usize,
    /// Buffer pixels per logical pixel.
    pub ratio: f64,
    /// `ratio` as an integer, for one-pixel lines.
    pub unit: i32,
    /// Selection border width: `2 * ratio`, rounded.
    pub border: i32,
    /// The frozen screenshot of this output, at buffer resolution.
    pub base: Option<&'a Base>,
    /// Pointer and drag state for this surface.
    pub state: &'a Selection,
    pub args: &'a Args,
    /// Bottom bar geometry, measured once per buffer size by the caller.
    pub bar: &'a BarLayout,
    pub bar_input: BarInput,
}

impl Frame<'_> {
    /// Crosshair position in buffer pixels.
    ///
    /// Public because `Damage::plan` needs the same value the paint does, and
    /// deriving it twice is exactly how the two would come to disagree.
    pub fn cross(&self) -> Option<(i32, i32)> {
        self.state
            .cursor_buf(self.ratio, self.w as i32, self.h as i32)
    }

    /// The drag selection in buffer pixels.
    pub fn selection(&self) -> Option<Rect> {
        self.state.rect(self.ratio, self.w as i32, self.h as i32)
    }
}

/// Draw one frame into `cv`, restricted to the damage `rects` (buffer pixels).
pub fn paint(cv: &mut Canvas, text: &mut TextRenderer, rects: &[Rect], f: &Frame) {
    let cross = f.cross();
    let selection = f.selection();

    // Window highlights are hidden while dragging.
    let win_rects: Vec<WinOverlay> = if f.state.dragging() {
        Vec::new()
    } else {
        f.state
            .windows
            .iter()
            .enumerate()
            .map(|(idx, win)| WinOverlay {
                rect: Rect::new(
                    (win.x as f64 * f.ratio).round() as i32,
                    (win.y as f64 * f.ratio).round() as i32,
                    (win.w as f64 * f.ratio).round() as i32,
                    (win.h as f64 * f.ratio).round() as i32,
                ),
                hovered: f.state.hovered == Some(idx),
            })
            .collect()
    };

    let ctx = RenderCtx {
        base: f.base,
        w: f.w,
        h: f.h,
        unit: f.unit,
        thickness: f.unit,
        border: f.border,
        cross,
        selection,
        windows: &win_rects,
        selection_color: f.args.selection_color,
        window_color: f.args.window_color,
    };

    // QS-style size label at the selection's bottom-right, while dragging.
    let label = match (
        f.state.dragging(),
        f.state.drag_start,
        f.state.pointer,
        selection,
    ) {
        (true, Some(a), Some(b), Some(sel)) => {
            let lw = (b.0 - a.0).abs().round() as i32;
            let lh = (b.1 - a.1).abs().round() as i32;
            let s = format!("{lw} x {lh}");
            let size = 13.0 * f.ratio as f32;
            let tw = text.measure(&s, size) as i32;
            let pad = (8.0 * f.ratio) as i32;
            let mut tx = sel.right() - tw - pad;
            let mut ty = sel.bottom() + pad;
            if tx < 0 {
                tx = 0;
            }
            if ty + size as i32 > f.h as i32 {
                ty = sel.y - size as i32 - pad;
            }
            if ty < 0 {
                ty = sel.bottom() + pad;
            }
            Some((s, tx, ty, size))
        }
        _ => None,
    };

    for &rect in rects {
        compose(cv, rect, &ctx);
    }
    // The size label is part of the overlay content, so it goes down *before*
    // the bar — like the selection border and crosshair, which `compose` already
    // drew — and the bar paints over it where they overlap. QML orders them the
    // same way: the label is `z: 3` inside `contentRoot`, the toolbar is its
    // sibling at `z: 20`. Drawing the label last put the text on top of the card.
    if let Some((s, tx, ty, size)) = &label {
        text.draw_colored(cv, s, *tx, *ty, *size, f.args.selection_color);
    }
    bottombar::draw(cv, text, f.bar, f.ratio, f.args, f.bar_input);
}
