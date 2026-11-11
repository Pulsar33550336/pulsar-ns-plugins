//! Rendering: framebuffer canvas, frame composition and text rasterization.

pub mod canvas;
pub mod compose;
pub mod text;

pub use canvas::{Canvas, Rect};
pub use compose::{bands, buffer_dims, compose, Base, Dim, RenderCtx, WinOverlay};
pub use text::TextRenderer;
