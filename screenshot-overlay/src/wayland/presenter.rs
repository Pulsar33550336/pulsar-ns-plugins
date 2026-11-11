//! Double-buffered shm presentation for one layer surface.
//!
//! Owns the layer surface plus two reusable shm slots/buffers. Reusing them is
//! deliberate: `SlotPool::create_buffer` allocates a fresh slot each call, and
//! under repeated redraws the pool grows until `wl_shm_pool.resize` overflows
//! `i32`, which the compositor rejects ("invalid wl_shm_pool size").

use smithay_client_toolkit::{
    compositor::FrameCallbackData,
    shell::{wlr_layer::LayerSurface, WaylandSurface},
    shm::slot::{Buffer, Slot, SlotPool},
};
use wayland_client::{protocol::wl_surface, QueueHandle};

use crate::app::App;
use crate::render::Rect;

pub struct Presenter {
    layer: LayerSurface,
    pool: SlotPool,
    /// Kept alive: each `Buffer` only holds a weak reference to its slot.
    _slots: [Slot; 2],
    buffers: [Buffer; 2],
    next: usize,
    /// A frame callback from the last commit is still outstanding.
    pub frame_pending: bool,
    /// The layer surface has been configured by the compositor.
    pub configured: bool,
}

impl Presenter {
    pub fn new(
        layer: LayerSurface,
        pool: SlotPool,
        slots: [Slot; 2],
        buffers: [Buffer; 2],
    ) -> Self {
        Self {
            layer,
            pool,
            _slots: slots,
            buffers,
            next: 0,
            frame_pending: false,
            configured: false,
        }
    }

    pub fn surface(&self) -> &LayerSurface {
        &self.layer
    }

    pub fn is_surface(&self, surface: &wl_surface::WlSurface) -> bool {
        self.layer.wl_surface() == surface
    }

    /// Index of a reusable buffer that the compositor is not holding, if any.
    pub fn acquire(&self) -> Option<usize> {
        let i = if self.buffers[self.next].slot().has_active_buffers() {
            1 - self.next
        } else {
            self.next
        };
        (!self.buffers[i].slot().has_active_buffers()).then_some(i)
    }

    /// The canvas of buffer `i`, if it is not in flight.
    pub fn canvas(&mut self, i: usize) -> Option<&mut [u8]> {
        self.buffers[i].canvas(&mut self.pool)
    }

    /// Damage, attach, request the next frame callback and commit buffer `i`.
    pub fn present(
        &mut self,
        qh: &QueueHandle<App>,
        i: usize,
        rects: &[Rect],
        buf_scale: Option<i32>,
    ) {
        let surface = self.layer.wl_surface();
        if let Some(bs) = buf_scale {
            surface.set_buffer_scale(bs);
        }
        for r in rects {
            surface.damage_buffer(r.x, r.y, r.w, r.h);
        }
        self.buffers[i].attach_to(surface).expect("attach buffer");
        // `wl_surface.frame` is double-buffered: it must be requested before
        // the commit, otherwise the callback is never delivered.
        surface.frame(qh, FrameCallbackData(surface.clone()));
        self.layer.commit();

        self.frame_pending = true;
        self.next = 1 - i;
    }

    /// The previous frame was presented; allow the next one.
    pub fn frame_done(&mut self) {
        self.frame_pending = false;
    }
}
