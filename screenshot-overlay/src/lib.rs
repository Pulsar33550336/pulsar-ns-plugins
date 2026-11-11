//! The library half of `screenshot-overlay`.
//!
//! The binary is a thin shell over this: it parses the command line, takes the
//! single-instance lock and calls [`app::run`]. Everything else — the overlay
//! surface, the renderer, the compositor backends, the report — lives here.
//!
//! It is a library as well as a binary because `benches/` is a separate crate and
//! can only reach a **library** target. The headless render benchmark in
//! `benches/render.rs` needs exactly that, and it is worth the split: otherwise
//! the benchmark has to be smuggled into the shipped binary behind an env var
//! (`SSR_BENCH`), which is what this replaced.

pub mod app;
pub mod capture;
pub mod cli;
pub mod compositors;
pub mod icons;
pub mod lock;
pub mod overlay;
pub mod render;
pub mod report;
pub mod wayland;
