//! Headless benchmark of the overlay's frame path: `cargo bench`.
//!
//! It drives the **real** frame path. `overlay::frame::paint` is the same
//! function `Overlay::try_draw` calls on a shm buffer, the damage rects come
//! from the real `overlay::damage::Damage`, and the `Args`, the bottom-bar
//! layout and the frozen frame are the real types. Nothing here re-implements
//! the composition or the damage planning, so a change to either is measured
//! rather than missed.
//!
//! That is possible because `Frame` holds no Wayland state, so a frame can be
//! drawn into a plain `Vec` with no compositor and no surface. It is also why
//! there is no `bar only` or `text only` benchmark: those were micro-measurements
//! of pieces, and the number that matters is what one frame costs.
//!
//! Three scenarios, chosen because they are the three things the overlay
//! actually does:
//!
//! - **drag, full repaint** — the selection changed, so the whole buffer is
//!   recomposed. This is the expensive one.
//! - **crosshair move, partial repaint** — the common case while the pointer
//!   moves. `Damage::plan` returns a handful of bands and only those are
//!   recomposed, which is what keeps a moving crosshair cheap.
//! - **hover, full repaint** — not dragging, so the window highlights are in
//!   play; a hover change forces a full repaint.
//!
//! Criterion owns the timing, the statistics and the machine-readable reports
//! under `target/criterion/`; `cargo bench -- --quick` trades sample count for
//! wall time, and `--sample-size` / `--measurement-time` are there when a real
//! number is wanted. Note that criterion writes to **stdout**: that does not
//! bend the rule that stdout is the report channel, because that rule binds the
//! overlay *binary*, whose stdout a launcher parses. A benchmark target is a
//! different program, and criterion owns its own output.

use std::hint::black_box;

use clap::Parser;
use criterion::{criterion_group, criterion_main, Criterion};

use screenshot_overlay::cli::Args;
use screenshot_overlay::compositors::Window;
use screenshot_overlay::icons;
use screenshot_overlay::overlay::bottombar::{self, BarInput, BarLayout};
use screenshot_overlay::overlay::damage::Damage;
use screenshot_overlay::overlay::frame::{paint, Frame};
use screenshot_overlay::overlay::selection::Selection;
use screenshot_overlay::render::{buffer_dims, Base, Canvas, Dim, TextRenderer};

/// The overlay's stock dimming, baked into the frozen frame exactly as the real
/// run bakes it (`--dim-color` defaults to the QML `#111111` at 0.533). Spelled
/// out rather than read from `Args` because `Dim` belongs to the captured frame,
/// not to the frame being drawn.
const DIM: Dim = Dim {
    rgb: [0x11, 0x11, 0x11],
    alpha: 0.533,
};

/// A 1536x960 logical output at scale 2, i.e. a 4K-class physical buffer — what
/// a HiDPI frame is really drawn at. Fed through the real `buffer_dims` so the
/// benchmark follows an `SSR_MODE` change instead of hardcoding one.
const LOGICAL: (u32, u32) = (1536, 960);
const SCALE: i32 = 2;

/// The buffer geometry, and the scalars `try_draw` derives from it.
struct Geom {
    w: usize,
    h: usize,
    ratio: f64,
    unit: i32,
    border: i32,
}

impl Geom {
    fn new() -> Self {
        let (w, h, _buf_scale, _call_scale) = buffer_dims(LOGICAL.0, LOGICAL.1, SCALE);
        let w = w as usize;
        let h = h as usize;
        let ratio = (w / LOGICAL.0 as usize).max(1) as f64;
        Self {
            w,
            h,
            ratio,
            unit: (ratio as i32).max(1),
            border: ((2.0 * ratio).round() as i32).max(1),
        }
    }
}

/// Everything the three scenarios share. Built once, before any timing, so the
/// cost of a 4K frozen frame and a font scan is not in the numbers.
struct Setup {
    geom: Geom,
    args: Args,
    bar: BarLayout,
    base: Base,
    text: TextRenderer,
    pixels: Vec<u8>,
}

impl Setup {
    fn new() -> Self {
        let geom = Geom::new();
        // Both presentation flags, because neither has a default: the plugin
        // passes a translated label and the target's glyph on every launch, so a
        // benchmark without them would measure a bar shape that never appears.
        let args = Args::parse_from([
            "screenshot-overlay",
            "--label",
            "Screenshot",
            "--target-icon",
            "screenshot",
        ]);
        let base = frozen_frame(geom.w, geom.h);

        let mut text = TextRenderer::new();
        // Register the embedded icon font exactly as `app::run` does, so the bar
        // is measured with its real glyph rather than a missing-glyph fallback.
        text.load_icon_font(icons::FONT);
        let bar = bottombar::layout(
            geom.w,
            geom.h,
            geom.ratio,
            &bottombar::measure_glyphs(&mut text, geom.ratio, &args),
        );

        let pixels = vec![0u8; geom.w * geom.h * 4];
        Self {
            geom,
            args,
            bar,
            base,
            text,
            pixels,
        }
    }
}

fn frame(c: &mut Criterion) {
    let mut setup = Setup::new();
    bench_drag_full(c, &mut setup);
    bench_crosshair_move(c, &mut setup);
    bench_hover_full(c, &mut setup);
}

/// A selection drag in progress: the size label is up, the window highlights are
/// suppressed, and every change forces a full recompose.
fn bench_drag_full(c: &mut Criterion, s: &mut Setup) {
    let Setup {
        geom,
        args,
        bar,
        base,
        text,
        pixels,
    } = s;
    let (geom, args, bar, base): (&Geom, &Args, &BarLayout, &Base) = (geom, args, bar, base);

    let state = drag_state();
    let mut damage = Damage::new();

    c.bench_function("drag: full repaint", |b| {
        b.iter(|| {
            let frame = frame_for(geom, base, &state, args, bar);
            let cross = frame.cross();

            damage.request(true);
            let mut rects = damage
                .plan(0, cross, geom.w as i32, geom.h as i32, geom.unit)
                .expect("a full repaint is always planned");
            rects.push(bar.card);
            {
                let mut cv = Canvas::new(pixels.as_mut_slice(), geom.w, geom.h);
                paint(&mut cv, text, &rects, &frame);
            }
            damage.presented(0, cross);
            // `compose` writes into a buffer nothing reads, so without an opaque
            // barrier the optimizer may delete the work being timed.
            black_box(&pixels[..]);
        });
    });
}

/// The crosshair moving with no drag: `Damage::plan` hands back the bands under
/// the old and new positions (in both buffers) and only those are recomposed.
fn bench_crosshair_move(c: &mut Criterion, s: &mut Setup) {
    let Setup {
        geom,
        args,
        bar,
        base,
        text,
        pixels,
    } = s;
    let (geom, args, bar, base): (&Geom, &Args, &BarLayout, &Base) = (geom, args, bar, base);

    let mut state = Selection::new();
    state.pointer = Some((600.5, 800.5));

    // Two adjacent positions, alternating, so every iteration is a real move
    // rather than the "nothing changed" early return.
    let (p0, p1) = ((601, 800), (602, 800));
    let mut damage = Damage::new();
    damage.presented(0, Some(p0)); // buffer 0 is already showing `p0`
    let mut k = 0u32;

    c.bench_function("crosshair move: partial repaint", |b| {
        b.iter(|| {
            let cur = if k.is_multiple_of(2) { p1 } else { p0 };
            k += 1;

            let frame = frame_for(geom, base, &state, args, bar);
            damage.request(false);
            let Some(mut rects) =
                damage.plan(0, Some(cur), geom.w as i32, geom.h as i32, geom.unit)
            else {
                return;
            };
            rects.push(bar.card);
            {
                let mut cv = Canvas::new(pixels.as_mut_slice(), geom.w, geom.h);
                paint(&mut cv, text, &rects, &frame);
            }
            damage.presented(0, Some(cur));
            black_box(&pixels[..]);
        });
    });
}

/// Not dragging, with windows on the output and one hovered: a hover change is a
/// full repaint, and the highlights are extra fill and border work per window.
fn bench_hover_full(c: &mut Criterion, s: &mut Setup) {
    let Setup {
        geom,
        args,
        bar,
        base,
        text,
        pixels,
    } = s;
    let (geom, args, bar, base): (&Geom, &Args, &BarLayout, &Base) = (geom, args, bar, base);

    let mut state = Selection::new();
    state.pointer = Some((500.0, 400.0));
    state.windows = windows();
    state.hovered = Some(state.windows.len() - 1);

    let mut damage = Damage::new();

    c.bench_function("hover: full repaint", |b| {
        b.iter(|| {
            let frame = frame_for(geom, base, &state, args, bar);
            let cross = frame.cross();

            damage.request(true);
            let mut rects = damage
                .plan(0, cross, geom.w as i32, geom.h as i32, geom.unit)
                .expect("a full repaint is always planned");
            rects.push(bar.card);
            {
                let mut cv = Canvas::new(pixels.as_mut_slice(), geom.w, geom.h);
                paint(&mut cv, text, &rects, &frame);
            }
            damage.presented(0, cross);
            black_box(&pixels[..]);
        });
    });
}

fn frame_for<'a>(
    geom: &Geom,
    base: &'a Base,
    state: &'a Selection,
    args: &'a Args,
    bar: &'a BarLayout,
) -> Frame<'a> {
    Frame {
        w: geom.w,
        h: geom.h,
        ratio: geom.ratio,
        unit: geom.unit,
        border: geom.border,
        base: Some(base),
        state,
        args,
        bar,
        bar_input: BarInput::default(),
    }
}

/// A drag from the top-left quadrant to the bottom-right one — the state in
/// which the size label is drawn.
fn drag_state() -> Selection {
    let mut state = Selection::new();
    state.drag_start = Some((200.0, 150.0));
    state.pointer = Some((1200.0, 850.0));
    state
}

/// A grid of tiled windows in logical coordinates, as the niri backend reports
/// them, filling the output: 4 columns by 3 rows.
fn windows() -> Vec<Window> {
    (0..12)
        .map(|i| {
            let (col, row) = (i % 4, i / 4);
            Window {
                x: 40 + col * 370,
                y: 60 + row * 300,
                w: 350,
                h: 280,
                title: "benchmark window".into(),
                app_id: "bench".into(),
                floating: false,
            }
        })
        .collect()
}

/// A frozen frame with non-compressible pixels, so the dim pass and the
/// selection copy are real memory traffic rather than a memset.
fn frozen_frame(w: usize, h: usize) -> Base {
    let mut raw = vec![0u8; w * h * 4];
    for (i, px) in raw.as_chunks_mut::<4>().0.iter_mut().enumerate() {
        let v = (i % 251) as u8;
        px.copy_from_slice(&[v, v, v, 255]);
    }
    Base::from_raw(raw, w, h, DIM)
}

criterion_group!(benches, frame);
criterion_main!(benches);
