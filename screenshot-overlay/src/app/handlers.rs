//! Every `smithay-client-toolkit` protocol handler `App` implements.
//!
//! These are the callbacks the compositor drives, as opposed to `App`'s own state
//! and the event loop in `mod.rs`. They were two thirds of a 900-line file, which
//! is why they moved — not because they are unimportant: `pointer_frame` is where
//! a drag turns into the crop the plugin is handed, and it is the longest thing
//! in the crate.
//!
//! An `impl` block for `App` in a child module is ordinary Rust, and it can touch
//! its private fields because privacy is visible to descendants. So this is a
//! move, not an API.

use log::{debug, error, info};
use smithay_client_toolkit::{
    compositor::CompositorHandler,
    output::{OutputHandler, OutputState},
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    seat::{
        keyboard::{KeyEvent, KeyboardHandler, Keysym, Modifiers, RawModifiers},
        pointer::{PointerEvent, PointerEventKind, PointerHandler},
        Capability, SeatHandler, SeatState,
    },
    shell::wlr_layer::{LayerShellHandler, LayerSurface, LayerSurfaceConfigure},
    shm::{Shm, ShmHandler},
};
use wayland_client::{
    protocol::{wl_keyboard, wl_output, wl_pointer, wl_seat, wl_surface},
    Connection, QueueHandle,
};

use super::{App, Pending, BTN_RIGHT};
use crate::capture;
use crate::report::{Button, Region};

impl CompositorHandler for App {
    fn scale_factor_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _new_factor: i32,
    ) {
    }

    fn transform_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _new_transform: wl_output::Transform,
    ) {
    }

    fn frame(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        surface: &wl_surface::WlSurface,
        _time: u32,
    ) {
        // The previous frame has been presented; allow the next one.
        for ov in self.overlays.iter_mut() {
            if ov.presenter.is_surface(surface) {
                ov.presenter.frame_done();
            }
        }
    }

    fn surface_enter(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _output: &wl_output::WlOutput,
    ) {
    }

    fn surface_leave(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _output: &wl_output::WlOutput,
    ) {
    }
}

impl OutputHandler for App {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }

    fn new_output(
        &mut self,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
        output: wl_output::WlOutput,
    ) {
        // Hotplug: a new monitor appeared -> new delegate instance.
        self.add_overlay(qh, output);
    }

    fn update_output(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _output: wl_output::WlOutput,
    ) {
    }

    fn output_destroyed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        output: wl_output::WlOutput,
    ) {
        // Hotplug: monitor removed -> drop its delegate instance.
        self.overlays.retain(|o| o.output != output);
        debug!(
            "[overlay] output destroyed, {} remaining",
            self.overlays.len()
        );
    }
}

impl LayerShellHandler for App {
    fn closed(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _layer: &LayerSurface) {
        self.exit = true;
    }

    fn configure(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        layer: &LayerSurface,
        configure: LayerSurfaceConfigure,
        _serial: u32,
    ) {
        let (nw, nh) = configure.new_size;
        for ov in self.overlays.iter_mut() {
            if ov.presenter.surface() != layer {
                continue;
            }
            if nw > 0 && nh > 0 {
                ov.width = nw;
                ov.height = nh;
            }
            ov.presenter.configured = true;
            ov.mark_dirty(false);
            debug!(
                "[overlay] {} configured {}x{}",
                ov.name, ov.width, ov.height
            );
        }
    }
}

impl SeatHandler for App {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seat_state
    }

    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}

    fn new_capability(
        &mut self,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
        seat: wl_seat::WlSeat,
        capability: Capability,
    ) {
        if capability == Capability::Keyboard && self.keyboard.is_none() {
            self.keyboard = Some(
                self.seat_state
                    .get_keyboard(qh, &seat, None)
                    .expect("create keyboard"),
            );
        }
        if capability == Capability::Pointer && self.pointer.is_none() {
            let pointer = self
                .seat_state
                .get_pointer(qh, &seat)
                .expect("create pointer");
            // The shape device is per-pointer, so it can only be made once the
            // pointer exists — which is why the manager is bound early and kept.
            if let Some(manager) = &self.cursor_shape_manager {
                self.cursor_shape = Some(manager.get_shape_device(&pointer, qh));
            }
            self.pointer = Some(pointer);
        }
    }

    fn remove_capability(
        &mut self,
        _conn: &Connection,
        _: &QueueHandle<Self>,
        _: wl_seat::WlSeat,
        capability: Capability,
    ) {
        if capability == Capability::Keyboard {
            if let Some(k) = self.keyboard.take() {
                k.release();
            }
        }
        if capability == Capability::Pointer {
            if let Some(p) = self.pointer.take() {
                p.release();
            }
        }
    }

    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
}

impl KeyboardHandler for App {
    fn enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _surface: &wl_surface::WlSurface,
        _serial: u32,
        _raw: &[u32],
        _keysyms: &[Keysym],
    ) {
    }

    fn leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _surface: &wl_surface::WlSurface,
        _serial: u32,
    ) {
    }

    fn press_key(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _serial: u32,
        event: KeyEvent,
    ) {
        if event.keysym == Keysym::Escape {
            info!("[key] Escape -> quit");
            self.exit = true;
        }
    }

    fn repeat_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _serial: u32,
        _event: KeyEvent,
    ) {
    }

    fn release_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _serial: u32,
        _event: KeyEvent,
    ) {
    }

    fn update_modifiers(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _serial: u32,
        _modifiers: Modifiers,
        _raw_modifiers: RawModifiers,
        _layout: u32,
    ) {
    }
}

impl PointerHandler for App {
    fn pointer_frame(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _pointer: &wl_pointer::WlPointer,
        events: &[PointerEvent],
    ) {
        let mut close = false;
        // `self` is borrowed by the overlay iterator below, so a selection's
        // outcome is collected in locals and applied after the loop.
        let mut confirmed: Option<Pending> = None;
        let mut confirm_error: Option<(Button, String)> = None;
        // The cursor shape belongs to the pointer, not to a surface, so it is
        // handled once per event rather than inside the per-surface loop.
        let cursor = self.cursor_shape.clone();
        let shape = self.args.cursor.into();
        for event in events {
            // Only `Enter` carries a serial, which the protocol requires, and the
            // compositor drops the shape again when the pointer leaves the
            // surface — so it is re-asserted on every entry.
            if let PointerEventKind::Enter { serial, .. } = event.kind {
                if let Some(device) = cursor.as_ref() {
                    device.set_shape(serial, shape);
                }
            }
            for ov in self.overlays.iter_mut() {
                // Events carry the wl_surface they belong to: that is how each
                // delegate instance keeps its own state.
                if !ov.presenter.is_surface(&event.surface) {
                    continue;
                }

                match event.kind {
                    PointerEventKind::Enter { .. } | PointerEventKind::Motion { .. } => {
                        ov.selection.pointer = Some(event.position);

                        // The close button owns the pointer while hovered.
                        let over_button = ov.over_close_button(event.position);
                        if over_button != ov.bar_hover {
                            ov.bar_hover = over_button;
                            // The bar's own area is always damaged, so a partial
                            // repaint is enough to restyle the button.
                            ov.mark_dirty(false);
                        }

                        if ov.selection.dragging() {
                            ov.mark_dirty(true);
                            continue;
                        }

                        // No window lights up under the bar, but the crosshair
                        // must keep following the pointer everywhere: in the QML
                        // original the toolbar never stops it either, it only
                        // suppresses window detection. Fold the suppression into
                        // the hit test so the crosshair redraw stays on the
                        // common path and cannot be skipped by a branch.
                        let h = if ov.over_bar(event.position) {
                            None
                        } else {
                            ov.window_at(event.position)
                        };
                        if h != ov.selection.hovered {
                            // Highlight changed: repaint everything.
                            ov.selection.hovered = h;
                            if let Some(i) = h {
                                debug!(
                                    "[hover] {} — {}",
                                    ov.selection.windows[i].app_id, ov.selection.windows[i].title
                                );
                            }
                            ov.mark_dirty(true);
                        } else {
                            // Only the crosshair moved: partial repaint.
                            ov.mark_dirty(false);
                        }
                    }
                    PointerEventKind::Leave { .. } => {
                        ov.selection.pointer = None;
                        ov.selection.drag_start = None;
                        ov.selection.hovered = None;
                        ov.bar_hover = false;
                        ov.bar_press = false;
                        ov.mark_dirty(true);
                    }
                    PointerEventKind::Press { button, .. } => {
                        ov.selection.pointer = Some(event.position);

                        // A press on the close button is consumed by it, exactly
                        // as the QML NButton prevents the MouseArea beneath from
                        // starting a selection.
                        if ov.over_close_button(event.position) {
                            ov.bar_press = true;
                            ov.mark_dirty(false);
                            continue;
                        }

                        ov.selection.hovered = ov.window_at(event.position);
                        ov.selection.drag_start = Some(event.position);
                        debug!(
                            "[select] [{}] press 0x{button:x} @ ({:.0}, {:.0})",
                            ov.name, event.position.0, event.position.1
                        );
                        ov.mark_dirty(true);
                    }
                    PointerEventKind::Release { button, .. } => {
                        if ov.bar_press {
                            let hit = ov.over_close_button(event.position);
                            ov.bar_press = false;
                            ov.bar_hover = hit;
                            ov.mark_dirty(false);
                            if hit {
                                close = true; // close button pressed
                            }
                            continue;
                        }
                        // The one thing this binary decides: which button
                        // finished the selection. What that *means* is the
                        // plugin's business, and it reads it out of the report.
                        let button = if button == BTN_RIGHT {
                            Button::Right
                        } else {
                            Button::Left
                        };
                        if let Some((x1, y1)) = ov.selection.drag_start.take() {
                            let (x2, y2) = event.position;
                            let s = ov.scale.max(1) as f64;
                            let mut lx = x1.min(x2);
                            let mut ly = y1.min(y2);
                            let mut lw = (x2 - x1).abs();
                            let mut lh = (y2 - y1).abs();
                            // No meaningful drag -> use the hovered window, like
                            // QS' resolveFallbackRegion.
                            if lw.round() < 1.0 || lh.round() < 1.0 {
                                if let Some(w) = ov.selection.hovered_window() {
                                    lx = w.x as f64;
                                    ly = w.y as f64;
                                    lw = w.w as f64;
                                    lh = w.h as f64;
                                }
                            }
                            debug!(
                                "[select] [{}] logical {:.0}x{:.0} at ({:.0}, {:.0}) | physical {:.0}x{:.0} at ({:.0}, {:.0})",
                                ov.name, lw, lh, lx, ly, lw * s, lh * s, lx * s, ly * s,
                            );
                            let region = Region {
                                x: lx.round() as i32,
                                y: ly.round() as i32,
                                w: (lw.round() as i32).max(1),
                                h: (lh.round() as i32).max(1),
                            };
                            // `--region-only` callers want the rectangle, not the
                            // pixels, so the copy and the PNG encode — most of
                            // what a release costs — are skipped rather than
                            // computed and thrown away by the caller.
                            let mut png = None;
                            let mut deliver = self.args.region_only;
                            if !self.args.region_only {
                                if let Some(base) = &ov.base {
                                    let iw = base.w() as f64;
                                    let ih = base.h() as f64;
                                    let cx = (lx * s).round().clamp(0.0, iw - 1.0) as u32;
                                    let cy = (ly * s).round().clamp(0.0, ih - 1.0) as u32;
                                    let cw = (lw * s).round().min(iw - cx as f64) as u32;
                                    let chh = (lh * s).round().min(ih - cy as f64) as u32;
                                    if cw > 0 && chh > 0 {
                                        // Encode now, while the frozen frame is
                                        // still in hand; deliver once the overlay
                                        // is gone.
                                        match capture::png_bytes(
                                            base,
                                            cx,
                                            cy,
                                            cw,
                                            chh,
                                            self.args.png_compression,
                                        ) {
                                            Ok(bytes) => {
                                                png = Some(bytes);
                                                deliver = true;
                                            }
                                            Err(e) => {
                                                error!("[confirm] {e}");
                                                confirm_error = Some((button, e));
                                            }
                                        }
                                    }
                                }
                            }
                            if deliver {
                                confirmed = Some(Pending {
                                    png,
                                    name: ov.name.clone(),
                                    button,
                                    region,
                                })
                            }
                        }
                        ov.mark_dirty(true);
                        close = true; // one-shot: finish after the first selection
                    }
                    PointerEventKind::Axis { .. } => {}
                }
            }
        }
        if close {
            self.exit = true;
        }
        if let Some((button, e)) = confirm_error {
            self.fail = Some((button, e));
        }
        if let Some(p) = confirmed {
            self.pending = Some(p);
        }
    }
}

impl ShmHandler for App {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

impl ProvidesRegistryState for App {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }

    registry_handlers![OutputState, SeatState];
}

smithay_client_toolkit::delegate_registry!(App);
smithay_client_toolkit::delegate_dispatch2!(App);
