//! Minimal port of the Quickshell screen-shot-and-record overlay to plain Rust.
//!
//! Quickshell's `Instantiator { model: Quickshell.screens; delegate: ... }` maps
//! directly onto wlr-layer-shell:
//!
//!   enumerate wl_output  ->  one fullscreen overlay layer surface per output
//!   wl_output hotplug    ->  create / destroy the matching surface
//!   per-surface pointer  ->  independent selection state per screen
//!
//! Run inside a wlroots-based compositor (niri/sway/hyprland/river/...):
//!   nix-shell -p pkg-config wayland wayland-protocols libxkbcommon --run "cargo build --release"
//!   ./target/release/screenshot-overlay
//!
//! Left-drag once to capture a region (written as a PNG, then exits); Esc
//! cancels.
//!
//! **This program does not act on a selection.** It captures, writes the image
//! to the path the caller chose, and reports which mouse button finished it.
//! Whether that means the clipboard, an editor, OCR or a recording is the
//! plugin's decision, read out of the report — so nothing here shells out, and
//! `--mode` / `--editor` do not exist.
//!
//! Stream contract, because a launcher consumes this program's output:
//!
//! - **stdout** carries exactly one JSON object — how the plugin learns the
//!   button, the path, and the rectangle it will hand to `wf-recorder`:
//!   `{"result":"delivered"|"cancelled"|"failed","button":"left"|"right",`
//!   `"output":"…","monitor":"…","region":{"x":…,"y":…,"w":…,"h":…}}`.
//!   A field that does not apply is absent, never null. `region` is in
//!   output-local **logical** pixels. Nothing else is ever written there.
//! - **stderr** is the log channel, on by default (`RUST_LOG=off` silences it),
//!   and carries clap's usage text. Diagnostics therefore never land in the
//!   stream the plugin parses.
//! - The image is always a **file**: the caller's `-o`, or the default under
//!   `~/Pictures/Screenshots`. There is no stream form.
//! - **exit 0** means the report is on stdout, `cancelled` included; **exit 1**
//!   means it carries an `error`; **exit 2** is a usage error, where stdout has
//!   no JSON at all; **exit 3** means another overlay already held the
//!   single-instance lock and this run did nothing (`result: "busy"`).
//!
//! The rest of the program is the library beside this file — see `lib.rs` for
//! why it is split in two.

use std::process::ExitCode;

use clap::Parser;

use screenshot_overlay::report::{self, Outcome};
use screenshot_overlay::{app, cli, lock};

/// `3` for "an overlay is already up". Distinct from `1` because nothing went
/// wrong: the answer to "may I start?" was no.
///
/// `2` for a usage error needs no constant: every argument is checked by a clap
/// value parser, so clap prints the usage text and exits 2 itself.
const EXIT_BUSY: u8 = 3;

fn main() -> ExitCode {
    init_logger();

    // Bad arguments never reach here: clap exits 2 with the usage text on stderr.
    let args = cli::Args::parse();

    // One overlay at a time, before anything touches Wayland. `_lock` must stay
    // alive for the whole run: dropping it releases the lock while the overlay is
    // still on screen, which would let a second one stack on top.
    let _lock = match lock::acquire() {
        Ok(lock::Availability::Held(lock)) => lock,
        Ok(lock::Availability::Busy) => {
            log::info!("[lock] another overlay is already running");
            report::emit(Outcome::Busy);
            return ExitCode::from(EXIT_BUSY);
        }
        Err(e) => {
            // The lock is the only thing standing between the user and two
            // stacked overlays, so a lock file we cannot reach is worth refusing
            // over rather than pressing on without the guarantee.
            log::error!("[lock] {e}");
            report::emit(Outcome::Failed {
                button: None,
                error: &e,
            });
            return ExitCode::FAILURE;
        }
    };

    match app::run(args) {
        Ok(()) => ExitCode::SUCCESS,
        // `app::run` already put the reason on stdout as the JSON report.
        // Printing here as well would add a second, non-JSON line to the stream
        // the caller parses.
        Err(_) => ExitCode::FAILURE,
    }
}

/// Logging setup. Code logs through the `log` facade; the backend is
/// `env_logger`, configured by `RUST_LOG`.
///
/// **On by default, on stderr.** stderr is the log channel and stdout is the data
/// channel: the one JSON report object (`report::emit`) goes to stdout, so log
/// lines cannot land in the middle of what the plugin parses. `env_logger` writes
/// to stderr, which is what keeps that true.
///
/// ```sh
/// RUST_LOG=off ./target/release/screenshot-overlay   # silence the logs
/// RUST_LOG=debug ./target/release/screenshot-overlay # and go verbose
/// ```
///
/// It lives here rather than in a `logger.rs` because it is eleven lines and one
/// call site: a module for it would be a file you have to open to learn nothing.
fn init_logger() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp_millis()
        .init();
}
