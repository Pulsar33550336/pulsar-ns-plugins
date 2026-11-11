//! One overlay per output: composes presentation, damage tracking and input.
//!
//! The pieces it composes live beside it: `selection` (the drag and the window
//! hit test), `damage` (what to repaint), `bottombar` (the bar drawn into the
//! surface) and `frame` (the composition itself). They are one concept — this
//! surface — which is why they are a directory rather than five files at the
//! crate root.
//!
//! All four are `pub` because `benches/render.rs` drives them: a bench is a
//! separate crate and can only reach public items, and the point of that bench
//! is to time the *real* frame path — `frame::paint` over rects from the real
//! `damage::Damage` — rather than a copy of it that can drift. `frame` is what
//! makes that possible: it holds no Wayland state, so a frame draws into a plain
//! `Vec` with no surface and no compositor.

pub mod bottombar;
pub mod damage;
pub mod frame;
pub mod selection;

use log::debug;
use wayland_client::{protocol::wl_output, QueueHandle};

use std::sync::Arc;

use self::damage::Damage;
use self::frame::{paint, Frame};
use self::selection::Selection;
use crate::app::App;
use crate::cli::Args;
use crate::compositors::Window;
use crate::render::{buffer_dims, Base, Canvas, TextRenderer};
use crate::wayland::Presenter;

pub struct Overlay {
    pub output: wl_output::WlOutput,
    pub name: String,
    pub presenter: Presenter,
    pub width: u32,
    pub height: u32,
    pub scale: i32,
    /// Frozen screenshot of this output, at physical resolution.
    pub base: Option<Base>,
    /// Whether the one-shot capture was attempted (success or failure).
    pub capture_attempted: bool,
    pub selection: Selection,
    pub damage: Damage,
    /// Pointer is over the bottom bar's close button.
    pub bar_hover: bool,
    /// A press landed on the close button, so the drag must not start.
    pub bar_press: bool,
    /// Bottom bar geometry, rebuilt when the surface geometry changes.
    pub bar: bottombar::BarLayout,
    /// Command line: colours, label and icons. Shared rather than copied per
    /// overlay, and outlives every overlay.
    args: Arc<Args>,
    logged_size: Option<(u32, u32, i32)>,
}

impl Overlay {
    pub fn new(
        output: wl_output::WlOutput,
        name: String,
        presenter: Presenter,
        width: u32,
        height: u32,
        scale: i32,
        args: Arc<Args>,
    ) -> Self {
        Self {
            output,
            name,
            presenter,
            width,
            height,
            scale,
            base: None,
            capture_attempted: false,
            selection: Selection::new(),
            damage: Damage::new(),
            bar_hover: false,
            bar_press: false,
            bar: bottombar::BarLayout::empty(),
            args,
            logged_size: None,
        }
    }

    /// Request a redraw; the next event-loop pass or frame callback paints it.
    /// `full` forces a whole-buffer repaint (needed when the selection changes).
    pub fn mark_dirty(&mut self, full: bool) {
        self.damage.request(full);
    }

    /// Install windows fetched from the compositor backend, clipped to the
    /// output.
    pub fn set_windows(&mut self, wins: Vec<Window>) {
        self.selection
            .set_windows(wins, self.width as i32, self.height as i32);
    }

    /// Index of the topmost window containing `pos` (logical surface coords).
    pub fn window_at(&self, pos: (f64, f64)) -> Option<usize> {
        self.selection.window_at(pos)
    }

    /// Whether `pos` (logical surface coords) is on the bar's close button.
    pub fn over_close_button(&self, pos: (f64, f64)) -> bool {
        self.bar.button_logical.contains(pos.0, pos.1)
    }

    /// Whether `pos` is anywhere on the bar. The bar owns the pointer there, so
    /// the window beneath must not light up.
    pub fn over_bar(&self, pos: (f64, f64)) -> bool {
        self.bar.card_logical.contains(pos.0, pos.1)
    }

    /// Paint at most once per frame, if anything changed and a buffer is free.
    pub fn try_draw(&mut self, qh: &QueueHandle<App>, text: &mut TextRenderer) {
        // Do not map the surface before the frozen frame exists, or the capture
        // would contain our own overlay.
        if self.base.is_none() && !self.capture_attempted {
            return;
        }
        if !self.damage.needs_draw() || self.presenter.frame_pending {
            return;
        }

        let logical_w = self.width.max(1);
        let logical_h = self.height.max(1);
        let (w, h, buf_scale, call_scale) = buffer_dims(logical_w, logical_h, self.scale);
        let (wus, hus) = (w as usize, h as usize);
        let ratio = (w / logical_w).max(1) as f64;
        let unit = (ratio as i32).max(1);
        let border = ((2.0 * ratio).round() as i32).max(1);

        if self.logged_size != Some((w, h, buf_scale)) {
            self.logged_size = Some((w, h, buf_scale));
            debug!(
                "[draw] mode={} {} buffer={}x{} buffer_scale={} call_scale={}",
                std::env::var("SSR_MODE").unwrap_or_default(),
                self.name,
                w,
                h,
                buf_scale,
                call_scale
            );
            // The bar's geometry depends only on the buffer size, the scale and
            // the icon font, so measure and lay it out here rather than every
            // frame: measuring shapes text through the whole font stack.
            self.bar = bottombar::layout(
                wus,
                hus,
                ratio,
                &bottombar::measure_glyphs(text, ratio, &self.args),
            );
        }

        // A buffer the compositor is not holding, or retry on the release event.
        let Some(i) = self.presenter.acquire() else {
            return;
        };

        // Everything one frame draws, and nothing about the protocol. `paint`
        // owns the composition and its z-order; this function owns only the
        // buffer, the damage rects and the commit.
        let frame = Frame {
            w: wus,
            h: hus,
            ratio,
            unit,
            border,
            base: self.base.as_ref(),
            state: &self.selection,
            args: &self.args,
            bar: &self.bar,
            bar_input: bottombar::BarInput {
                hovered: self.bar_hover,
                armed: self.bar_press,
            },
        };

        let cur = frame.cross();

        let Some(mut rects) = self.damage.plan(i, cur, w as i32, h as i32, unit) else {
            return; // nothing changed
        };

        // The bar is drawn on every frame, so its area must be repainted in
        // whichever buffer we are about to present — otherwise the other buffer
        // of the double buffer would show a stale or empty bar. Its geometry was
        // refreshed above, when the buffer size last changed.
        rects.push(self.bar.card);

        if let Some(px) = self.presenter.canvas(i) {
            let mut cv = Canvas::new(px, wus, hus);
            paint(&mut cv, text, &rects, &frame);
        } else {
            return;
        }

        self.presenter
            .present(qh, i, &rects, call_scale.then_some(buf_scale));
        self.damage.presented(i, cur);
    }
}
