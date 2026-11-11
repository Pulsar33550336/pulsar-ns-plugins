//! Wayland app state and the event loop.
//!
//! The protocol handlers themselves are in `handlers.rs`; this file is what `App`
//! *is* (`App`, `Pending`, `AlreadyReported`), the two things it does on its own
//! (`add_overlay`, `capture_frozen`), and the loop that drives it (`run`).
//!
//! `mod handlers;` is declared here so the impls land in the same module tree —
//! an `impl` for a type may live in a child module, and a child module can reach
//! its parent's private fields.

mod handlers;

use std::error::Error;

use libwayshot::WayshotConnection;
use log::{debug, error, info, warn};
use smithay_client_toolkit::{
    compositor::CompositorState,
    output::OutputState,
    registry::RegistryState,
    seat::{pointer::cursor_shape::CursorShapeManager, SeatState},
    shell::{
        wlr_layer::{Anchor, KeyboardInteractivity, Layer, LayerShell},
        WaylandSurface,
    },
    shm::{slot::SlotPool, Shm},
};
use wayland_client::{
    globals::registry_queue_init,
    protocol::{wl_keyboard, wl_output, wl_pointer, wl_shm},
    Connection, QueueHandle,
};

// `wp_cursor_shape_v1` lives in wayland-protocols' staging set, which sctk
// already depends on and re-exports — so asking for a crosshair costs no extra
// dependency and no cursor theme on disk.
use smithay_client_toolkit::reexports::protocols::wp::cursor_shape::v1::client::wp_cursor_shape_device_v1::{
    Shape, WpCursorShapeDeviceV1,
};

use std::sync::Arc;

use crate::capture;
use crate::cli::{Args, Cursor};
use crate::compositors::{self, WindowProvider};
use crate::overlay::Overlay;
use crate::render::{buffer_dims, Dim, TextRenderer};
use crate::report::{self, Button, Outcome};
use crate::wayland::Presenter;

/// The shape to ask for, as `--cursor` spells it.
///
/// Kept here rather than in `cli.rs` so that module stays free of Wayland
/// types — it describes the command line, not the protocol. `Cursor` exists at
/// all because `Shape` is foreign: `impl ValueEnum for Shape` has no local type
/// to hang off, so the flag needs an enum of its own, and this is what turns one
/// into the other. (`From<Cursor> for Shape` *is* allowed — the local type sits
/// in the trait's own parameter list — so there is no reason for a bespoke
/// conversion function.)
impl From<Cursor> for Shape {
    fn from(wanted: Cursor) -> Self {
        match wanted {
            Cursor::Crosshair => Shape::Crosshair,
            Cursor::Default => Shape::Default,
        }
    }
}

pub struct App {
    registry_state: RegistryState,
    output_state: OutputState,
    seat_state: SeatState,
    compositor: CompositorState,
    layer_shell: LayerShell,
    shm: Shm,
    overlays: Vec<Overlay>,
    pointer: Option<wl_pointer::WlPointer>,
    keyboard: Option<wl_keyboard::WlKeyboard>,
    /// `wp_cursor_shape_manager_v1`, when the compositor offers it, and the device
    /// it handed us for `pointer`. Together they let the overlay ask for a
    /// crosshair without shipping a cursor theme — see `cursor_shape_of` and the
    /// entry handling in `pointer_frame`.
    cursor_shape_manager: Option<CursorShapeManager>,
    cursor_shape: Option<WpCursorShapeDeviceV1>,
    /// Lazily-created libwayshot connection used for the one-shot capture.
    wayshot: Option<WayshotConnection>,
    /// Window-selection backend for the running compositor, if supported.
    provider: Option<Box<dyn WindowProvider>>,
    /// Text rasterizer (cosmic-text + swash) shared by the overlays.
    text: TextRenderer,
    /// Command line, shared with every overlay.
    args: Arc<Args>,
    /// Where a finished crop goes, resolved once from `-o`.
    dest: capture::Destination,
    /// A confirmed selection waiting to be delivered.
    ///
    /// The crop is encoded as soon as the pointer is released, but the delivery
    /// (file, clipboard, or editor) waits until the event loop has exited and the
    /// overlay surfaces are gone — otherwise an editor would open underneath the
    /// overlay it was launched from.
    pending: Option<Pending>,
    /// Set when a selection could not be delivered — with the button that made
    /// it, since a selection *did* happen and the report says so — so the run can
    /// exit non-zero instead of reporting success for a selection that saved
    /// nothing.
    fail: Option<(Button, String)>,
    exit: bool,
}

/// Linux input button code for a right click, as `wl_pointer` reports it (0x111;
/// left is 0x110). Only the right button is special-cased: QML's rule is
/// `mouseButton === Qt.RightButton ? "edit" : "copy"`, so everything else — left,
/// middle, a side button — is an ordinary selection.
const BTN_RIGHT: u32 = 0x111;

/// A confirmed selection, already encoded.
struct Pending {
    /// The encoded crop, or `None` for a `--region-only` run, where the caller
    /// asked for the rectangle and not the pixels.
    png: Option<Vec<u8>>,
    /// Output connector name, for the default filename and the report.
    name: String,
    /// Which button released. This is the binary's one piece of policy: whether
    /// that means "clipboard", "editor" or something else is the plugin's call,
    /// and it learns the answer from the report.
    button: report::Button,
    /// The selection, in output-local **logical** pixels. The crop above is
    /// physical; this is the rectangle a compositor tool takes as a geometry,
    /// which is what the plugin needs to start a recording of the same region.
    region: report::Region,
}

impl App {
    /// The Rust stand-in for one `Instantiator` delegate instantiation.
    fn add_overlay(&mut self, qh: &QueueHandle<Self>, output: wl_output::WlOutput) {
        if self.overlays.iter().any(|o| o.output == output) {
            return;
        }

        let info = self.output_state.info(&output);
        let name = info
            .as_ref()
            .and_then(|i| i.name.clone())
            .unwrap_or_else(|| "unknown-output".to_string());
        let scale = info.as_ref().map_or(1, |i| i.scale_factor);
        let (lw, lh) = info
            .as_ref()
            .and_then(|i| i.logical_size)
            .unwrap_or((1920, 1080));

        let logical_w = lw.max(1) as u32;
        let logical_h = lh.max(1) as u32;
        let (bw, bh, _bs, _cs) = buffer_dims(logical_w, logical_h, scale);
        let slot_len = (bw as usize) * (bh as usize) * 4;
        // Room for the two slots that are reused for the overlay's lifetime, so
        // the pool never grows during redraws.
        let mut pool = SlotPool::new(slot_len * 2 + 4096, &self.shm).expect("slot pool");
        let stride = (bw * 4) as i32;
        let s0 = pool.new_slot(slot_len).expect("slot 0");
        let s1 = pool.new_slot(slot_len).expect("slot 1");
        let b0 = pool
            .create_buffer_in(&s0, bw as i32, bh as i32, stride, wl_shm::Format::Argb8888)
            .expect("buffer 0");
        let b1 = pool
            .create_buffer_in(&s1, bw as i32, bh as i32, stride, wl_shm::Format::Argb8888)
            .expect("buffer 1");

        let surface = self.compositor.create_surface(qh);
        let layer = self.layer_shell.create_layer_surface(
            qh,
            surface,
            Layer::Overlay,
            Some("screenshot-overlay"),
            Some(&output),
        );
        // exact mirrors of ScreenShot.qml's PanelWindow settings
        layer.set_anchor(Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT);
        layer.set_size(0, 0); // fullscreen
        layer.set_exclusive_zone(-1); // ExclusionMode.Ignore
        layer.set_keyboard_interactivity(KeyboardInteractivity::OnDemand);
        layer.commit();

        info!("[overlay] created output={name:?} scale={scale} logical={lw}x{lh}");

        let presenter = Presenter::new(layer, pool, [s0, s1], [b0, b1]);
        self.overlays.push(Overlay::new(
            output,
            name,
            presenter,
            logical_w,
            logical_h,
            scale,
            Arc::clone(&self.args),
        ));
    }

    /// Capture this overlay's output once. Called before the surface is mapped,
    /// so the frozen frame does not contain our own overlay.
    fn capture_frozen(&mut self, idx: usize) {
        self.overlays[idx].capture_attempted = true;
        let name = self.overlays[idx].name.clone();

        if self.wayshot.is_none() {
            match WayshotConnection::new() {
                Ok(w) => self.wayshot = Some(w),
                Err(e) => {
                    error!("[capture] wayshot init failed: {e}");
                    self.overlays[idx].mark_dirty(true);
                    return;
                }
            }
        }

        let dim = Dim {
            rgb: self.args.dim_color,
            alpha: self.args.dim_alpha,
        };
        match capture::grab(self.wayshot.as_ref().unwrap(), &name, dim) {
            Ok(base) => {
                info!("[capture] {name} frozen {}x{}", base.w(), base.h());
                self.overlays[idx].base = Some(base);
            }
            Err(e) => error!("[capture] {e}"),
        }

        // Fetch on-screen windows for hover/click selection. `--no-window-detection`
        // skips the fetch outright rather than filtering later: with no window
        // list nothing can light up, and the compositor query (a `niri msg`
        // subprocess) is not run for a feature the caller turned off.
        let wins = if self.args.window_detection() {
            self.provider
                .as_ref()
                .and_then(|p| match p.windows_on(&name) {
                    Ok(w) => Some(w),
                    Err(e) => {
                        warn!("[windows] {}: {e}", p.name());
                        None
                    }
                })
        } else {
            None
        };
        if let Some(w) = wins {
            debug!("[windows] {} window(s) on {name}", w.len());
            self.overlays[idx].set_windows(w);
        }

        self.overlays[idx].mark_dirty(true);
    }
}

/// A failure whose reason is already on stdout as the JSON report, so `main`
/// must not write a second, non-JSON line next to it.
#[derive(Debug)]
pub struct AlreadyReported;

impl std::fmt::Display for AlreadyReported {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("the selection was not delivered")
    }
}

impl std::error::Error for AlreadyReported {}

/// Connect, create one overlay per output and run the event loop.
///
/// Every failure out of here reaches the caller as one JSON object on stdout:
/// either the delivery's own report, which knows the path and the button, or —
/// for a failure before any selection — one carrying just the reason.
pub fn run(args: Args) -> Result<(), Box<dyn Error>> {
    match run_inner(args) {
        Ok(()) => Ok(()),
        Err(e) if e.downcast_ref::<AlreadyReported>().is_some() => Err(e),
        Err(e) => {
            let reason = e.to_string();
            report::emit(Outcome::Failed {
                button: None,
                error: &reason,
            });
            Err(e)
        }
    }
}

fn run_inner(args: Args) -> Result<(), Box<dyn Error>> {
    let conn = Connection::connect_to_env()?;
    let (globals, mut event_queue) = registry_queue_init::<App>(&conn)?;
    let qh = event_queue.handle();

    let compositor = CompositorState::bind(&globals, &qh)?;
    let layer_shell = LayerShell::bind(&globals, &qh)
        .map_err(|e| format!("zwlr_layer_shell_v1 unavailable: {e}"))?;
    let shm = Shm::bind(&globals, &qh)?;
    let registry_state = RegistryState::new(&globals);
    let seat_state = SeatState::new(&globals, &qh);
    let output_state = OutputState::new(&globals, &qh);

    // Icons come from Noctalia's own font, compiled into the binary — see
    // `icons.rs`. Nothing to locate at runtime, so this cannot fail for want of
    // an installed shell.
    let mut text = TextRenderer::new();
    if text.load_icon_font(crate::icons::FONT) {
        info!(
            "[icons] {} ({} KiB embedded)",
            crate::icons::FAMILY,
            crate::icons::FONT.len() / 1024
        );
    }

    // Resolve `-o` once: the overlay loop cannot touch `self` while it is
    // borrowing the overlays, so the destination has to be picked up front.
    let dest = capture::Destination::from_arg(args.output.as_deref());

    // A compositor without `wp_cursor_shape_v1` is not a failure: the pointer just
    // keeps whatever it was showing. Bound before the first roundtrip so that the
    // pointer capability, which arrives with it, can build its device.
    let cursor_shape_manager = match CursorShapeManager::bind(&globals, &qh) {
        Ok(manager) => Some(manager),
        Err(e) => {
            info!("[cursor] no wp_cursor_shape_v1 ({e}); leaving the pointer alone");
            None
        }
    };

    let mut app = App {
        registry_state,
        output_state,
        seat_state,
        compositor,
        layer_shell,
        shm,
        overlays: Vec::new(),
        pointer: None,
        keyboard: None,
        cursor_shape_manager,
        cursor_shape: None,
        wayshot: None,
        provider: compositors::detect(),
        text,
        args: Arc::new(args),
        dest,
        pending: None,
        fail: None,
        exit: false,
    };

    // Let output info (name/logical size/scale) arrive, then materialise one
    // surface per output. new_output may already have done it; add_overlay dedupes.
    event_queue.roundtrip(&mut app)?;

    let outputs: Vec<_> = app.output_state.outputs().collect();
    info!("[main] wl_output globals: {}", outputs.len());
    for output in outputs {
        app.add_overlay(&qh, output);
    }

    info!(
        "[main] {} overlay surface(s) alive. Drag once to capture (then it exits); Esc cancels.",
        app.overlays.len()
    );

    while !app.exit {
        event_queue.blocking_dispatch(&mut app)?;
        // Capture the frozen frame once a surface is configured (before it is
        // mapped), then paint. Frame callbacks and buffer releases pace drawing.
        for i in 0..app.overlays.len() {
            if app.overlays[i].presenter.configured && !app.overlays[i].capture_attempted {
                app.capture_frozen(i);
            }
        }
        let App { overlays, text, .. } = &mut app;
        for ov in overlays.iter_mut() {
            ov.try_draw(&qh, text);
        }
    }

    // Tear the surfaces down *before* delivering. An editor launched over a
    // still-mapped overlay would open underneath it, and a clipboard owner would
    // briefly have the overlay in view. Dropping the presenters destroys the
    // layer surfaces; the roundtrip lets the compositor act on that.
    app.overlays.clear();
    event_queue.roundtrip(&mut app)?;

    // Save after the loop, on a clean desktop, and report what happened —
    // including the button, which is the only decision this binary makes.
    match app.pending.take() {
        Some(Pending {
            png: Some(png),
            name,
            button,
            region,
        }) => match capture::write_png(&png, &name, &app.dest) {
            Ok(path) => {
                info!("[save] {}", path.display());
                report::emit(Outcome::Delivered {
                    button,
                    output: Some(&path),
                    region,
                    monitor: &name,
                });
            }
            Err(e) => {
                error!("[save] {e}");
                // No path and no region: nothing was written, so a region would
                // describe a selection the caller has no image for.
                report::emit(Outcome::Failed {
                    button: Some(button),
                    error: &e,
                });
                app.fail = Some((button, e));
            }
        },
        // `--region-only`: the rectangle *is* the deliverable, so there is no file
        // to write and nothing that can fail to be written.
        Some(Pending {
            png: None,
            name,
            button,
            region,
        }) => report::emit(Outcome::Delivered {
            button,
            output: None,
            region,
            monitor: &name,
        }),
        // Nothing to write: either the user backed out (Esc, or the bar's close
        // button) — reported so the plugin can tell "nothing happened" from
        // "still running" — or the crop itself was refused back in
        // `pointer_frame`, which parked the reason on `fail` and left no image.
        // The second case still owes the plugin a line: `runStream` delivers no
        // exit status, so exiting quietly would look like a click that did
        // nothing at all.
        None => match &app.fail {
            // A selection that never became a PNG: same `failed` shape as a write
            // error, because the caller's question — did a selection happen? —
            // has the same answer.
            Some((button, e)) => report::emit(Outcome::Failed {
                button: Some(*button),
                error: e,
            }),
            None => report::emit(Outcome::Cancelled),
        },
    }

    // A run that failed must not look like a success, and `main` must not add a
    // second line to the stream the plugin parses: the report above is where the
    // reason went.
    if app.fail.is_some() {
        return Err(AlreadyReported.into());
    }

    info!("[main] done");
    Ok(())
}
