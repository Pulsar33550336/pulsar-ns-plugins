//! Pointer-driven selection state: region drag plus window hover.

use crate::compositors::Window;
use crate::render::Rect;

pub struct Selection {
    /// Pointer position in logical surface coordinates.
    pub pointer: Option<(f64, f64)>,
    /// Where a drag started, if one is in progress.
    pub drag_start: Option<(f64, f64)>,
    /// Windows on this output (logical, clipped), for hover/click.
    pub windows: Vec<Window>,
    /// Index of the hovered window, if any.
    pub hovered: Option<usize>,
}

impl Default for Selection {
    fn default() -> Self {
        Self::new()
    }
}

impl Selection {
    pub fn new() -> Self {
        Self {
            pointer: None,
            drag_start: None,
            windows: Vec::new(),
            hovered: None,
        }
    }

    pub fn dragging(&self) -> bool {
        self.drag_start.is_some()
    }

    /// Install windows fetched from the compositor backend, clipped to the
    /// output bounds `ow` x `oh`.
    pub fn set_windows(&mut self, wins: Vec<Window>, ow: i32, oh: i32) {
        self.windows = wins
            .into_iter()
            .filter_map(|mut w| {
                let x0 = w.x.max(0);
                let y0 = w.y.max(0);
                let x1 = (w.x + w.w).min(ow);
                let y1 = (w.y + w.h).min(oh);
                if x1 <= x0 || y1 <= y0 {
                    return None;
                }
                w.x = x0;
                w.y = y0;
                w.w = x1 - x0;
                w.h = y1 - y0;
                Some(w)
            })
            .collect();
        self.hovered = None;
    }

    /// Index of the topmost window containing `pos` (logical surface coords).
    pub fn window_at(&self, pos: (f64, f64)) -> Option<usize> {
        self.windows.iter().enumerate().rev().find_map(|(i, w)| {
            let inside = pos.0 >= w.x as f64
                && pos.0 < (w.x + w.w) as f64
                && pos.1 >= w.y as f64
                && pos.1 < (w.y + w.h) as f64;
            inside.then_some(i)
        })
    }

    pub fn hovered_window(&self) -> Option<&Window> {
        self.hovered.and_then(|i| self.windows.get(i))
    }

    /// Pointer position in buffer pixels.
    pub fn cursor_buf(&self, ratio: f64, w: i32, h: i32) -> Option<(i32, i32)> {
        self.pointer.map(|p| to_buf(p, ratio, w, h))
    }

    /// The drag selection as a buffer-pixel rect, if dragging.
    pub fn rect(&self, ratio: f64, w: i32, h: i32) -> Option<Rect> {
        let (a, b) = (self.drag_start?, self.pointer?);
        let (ax, ay) = to_buf(a, ratio, w, h);
        let (bx, by) = to_buf(b, ratio, w, h);
        Some(Rect::new(
            ax.min(bx),
            ay.min(by),
            (ax - bx).abs().max(1),
            (ay - by).abs().max(1),
        ))
    }
}

fn to_buf((px, py): (f64, f64), ratio: f64, w: i32, h: i32) -> (i32, i32) {
    (
        (px * ratio).round().clamp(0.0, (w - 1).max(0) as f64) as i32,
        (py * ratio).round().clamp(0.0, (h - 1).max(0) as f64) as i32,
    )
}
