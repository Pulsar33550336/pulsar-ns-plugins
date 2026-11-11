//! Redraw scheduling and damage tracking for a double-buffered surface.

use crate::render::{bands, Rect};

pub struct Damage {
    /// A redraw is wanted but not yet painted.
    needs_draw: bool,
    /// The next paint must redraw the whole buffer (selection changed).
    needs_full: bool,
    /// Nothing has been presented yet: the first commit shows the whole buffer.
    first: bool,
    /// Crosshair position (buffer pixels) last drawn into buffer i.
    last_px: [Option<(i32, i32)>; 2],
    /// Crosshair position currently on *screen* (the other buffer). Damage must
    /// be computed against this, not against the buffer being drawn, or the
    /// other buffer's crosshair leaves a ghost during fast motion.
    screen_pt: Option<(i32, i32)>,
}

impl Default for Damage {
    fn default() -> Self {
        Self::new()
    }
}

impl Damage {
    pub fn new() -> Self {
        Self {
            needs_draw: false,
            needs_full: false,
            first: true,
            last_px: [None, None],
            screen_pt: None,
        }
    }

    pub fn request(&mut self, full: bool) {
        self.needs_draw = true;
        self.needs_full |= full;
    }

    pub fn needs_draw(&self) -> bool {
        self.needs_draw
    }

    /// Plan the rectangles to repaint into buffer `i`. Returns `None` when
    /// nothing changed (and clears the pending flag).
    pub fn plan(
        &mut self,
        i: usize,
        cur: Option<(i32, i32)>,
        w: i32,
        h: i32,
        thickness: i32,
    ) -> Option<Vec<Rect>> {
        if self.first || self.needs_full {
            return Some(vec![Rect::new(0, 0, w, h)]);
        }
        if cur == self.last_px[i] && cur == self.screen_pt {
            self.needs_draw = false;
            return None;
        }
        let mut rects = Vec::new();
        if let Some(p) = self.last_px[i] {
            rects.extend(bands(p, w, h, thickness));
        }
        if let Some(p) = self.screen_pt {
            rects.extend(bands(p, w, h, thickness));
        }
        if let Some(p) = cur {
            rects.extend(bands(p, w, h, thickness));
        }
        Some(rects)
    }

    /// Record that buffer `i` with crosshair `cur` has been presented.
    pub fn presented(&mut self, i: usize, cur: Option<(i32, i32)>) {
        self.first = false;
        self.needs_draw = false;
        self.needs_full = false;
        self.last_px[i] = cur;
        self.screen_pt = cur;
    }
}
