//! Minimal text rasterizer on top of cosmic-text + swash.
//!
//! Body text uses cosmic-text's own `FontSystem` (system fonts via fontdb). UI
//! icons come from Noctalia's icon font, compiled into the binary — see
//! [`TextRenderer::load_icon_font`] and `crate::icons`.

use std::sync::Arc;

use cosmic_text::{
    Attrs, Buffer as TextBuffer, Family, FontSystem, Metrics, Shaping, SwashCache, SwashContent,
};

use super::canvas::Canvas;

pub struct TextRenderer {
    font_system: FontSystem,
    swash: SwashCache,
    /// The Noctalia icon font is loaded, so `icons::*` codepoints resolve.
    icons: bool,
}

impl Default for TextRenderer {
    fn default() -> Self {
        Self::new()
    }
}

impl TextRenderer {
    pub fn new() -> Self {
        Self {
            font_system: FontSystem::new(),
            swash: SwashCache::new(),
            icons: false,
        }
    }

    /// Register Noctalia's icon font from `data`, so its Private Use codepoints
    /// resolve. Returns whether fontdb accepted it.
    ///
    /// Wraps the caller's `&'static [u8]` in an `Arc` rather than going through
    /// `Database::load_font_data`, which takes a `Vec` and would copy the whole
    /// font onto the heap — 2.3 MiB that would then live twice.
    pub fn load_icon_font(&mut self, data: &'static [u8]) -> bool {
        let source = cosmic_text::fontdb::Source::Binary(Arc::new(data));
        let ids = self.font_system.db_mut().load_font_source(source);
        self.icons = !ids.is_empty();
        if !self.icons {
            log::warn!("[icons] the embedded icon font could not be registered");
        }
        self.icons
    }

    /// Whether an icon font is loaded, i.e. whether `draw_icon` will produce a
    /// real icon rather than a missing glyph.
    pub fn has_icons(&self) -> bool {
        self.icons
    }

    fn layout(&mut self, text: &str, size: f32, family: Option<&str>) -> TextBuffer {
        let mut buffer = TextBuffer::new(&mut self.font_system, Metrics::new(size, size * 1.25));
        buffer.set_size(Some(100000.0), Some(size * 1.25));
        let attrs = match family {
            Some(name) => Attrs::new().family(Family::Name(name)),
            None => Attrs::new(),
        };
        buffer.set_text(text, &attrs, Shaping::Advanced, None);
        buffer.shape_until_scroll(&mut self.font_system, false);
        buffer
    }

    pub fn measure(&mut self, text: &str, size: f32) -> f32 {
        self.measure_family(text, size, None)
    }

    /// Width of one icon from Noctalia's icon font.
    pub fn measure_icon(&mut self, icon: char, size: f32) -> f32 {
        self.measure_family(&icon.to_string(), size, Some(crate::icons::FAMILY))
    }

    fn measure_family(&mut self, text: &str, size: f32, family: Option<&str>) -> f32 {
        let buffer = self.layout(text, size, family);
        buffer
            .layout_runs()
            .map(|r| r.line_w)
            .fold(0.0_f32, f32::max)
    }

    /// The extent of the pixels `text` would actually paint, relative to the
    /// draw origin: `(top, height)`. This is ink, not the font's line box.
    ///
    /// Two glyphs from different fonts can share a line-box height while their
    /// ink sits at different heights, because ascent and descent differ. Anything
    /// that has to line them up optically must centre on this, not on the box.
    pub fn ink_extent(&mut self, text: &str, size: f32) -> Option<(i32, i32)> {
        self.ink_extent_family(text, size, None)
    }

    /// Ink extent of one icon from Noctalia's icon font.
    pub fn ink_extent_icon(&mut self, icon: char, size: f32) -> Option<(i32, i32)> {
        self.ink_extent_family(&icon.to_string(), size, Some(crate::icons::FAMILY))
    }

    fn ink_extent_family(
        &mut self,
        text: &str,
        size: f32,
        family: Option<&str>,
    ) -> Option<(i32, i32)> {
        let buffer = self.layout(text, size, family);
        let mut top = i32::MAX;
        let mut bottom = i32::MIN;
        for run in buffer.layout_runs() {
            let line_y = run.line_y.round() as i32;
            for glyph in run.glyphs.iter() {
                let physical = glyph.physical((0.0, 0.0), 1.0);
                let Some(image) = self
                    .swash
                    .get_image(&mut self.font_system, physical.cache_key)
                else {
                    continue;
                };
                if image.data.is_empty() {
                    continue;
                }
                // Same arithmetic `raster` uses, with the draw origin at y = 0.
                let gy = line_y + physical.y - image.placement.top;
                top = top.min(gy);
                bottom = bottom.max(gy + image.placement.height as i32);
            }
        }
        (bottom > top).then_some((top, bottom - top))
    }

    /// Draw `text` in an explicit colour, run origin at (x, y).
    pub fn draw_colored(
        &mut self,
        canvas: &mut Canvas,
        text: &str,
        x: i32,
        y: i32,
        size: f32,
        color: [u8; 3],
    ) {
        let buffer = self.layout(text, size, None);
        self.raster(canvas, &buffer, x, y, color);
    }

    /// Draw one icon from Noctalia's icon font. Check [`Self::has_icons`] first;
    /// without the font this yields the text font's missing-glyph box.
    pub fn draw_icon(
        &mut self,
        canvas: &mut Canvas,
        icon: char,
        x: i32,
        y: i32,
        size: f32,
        color: [u8; 3],
    ) {
        let buffer = self.layout(&icon.to_string(), size, Some(crate::icons::FAMILY));
        self.raster(canvas, &buffer, x, y, color);
    }

    /// Blend a shaped buffer's glyphs, run origin at (x, y).
    fn raster(&mut self, canvas: &mut Canvas, buffer: &TextBuffer, x: i32, y: i32, color: [u8; 3]) {
        for run in buffer.layout_runs() {
            let line_y = run.line_y.round() as i32;
            for glyph in run.glyphs.iter() {
                let physical = glyph.physical((0.0, 0.0), 1.0);
                let image = match self
                    .swash
                    .get_image(&mut self.font_system, physical.cache_key)
                {
                    Some(img) => img,
                    None => continue,
                };
                if image.data.is_empty() {
                    continue;
                }
                let gx = x + physical.x + image.placement.left;
                let gy = y + line_y + physical.y - image.placement.top;
                let (gw, gh) = (image.placement.width as i32, image.placement.height as i32);
                match image.content {
                    SwashContent::Mask => {
                        for row in 0..gh {
                            for col in 0..gw {
                                let a = image.data[(row * gw + col) as usize];
                                if a != 0 {
                                    canvas.blend_px(gx + col, gy + row, color, a);
                                }
                            }
                        }
                    }
                    SwashContent::Color => {
                        for row in 0..gh {
                            for col in 0..gw {
                                let si = ((row * gw + col) * 4) as usize;
                                if image.data[si + 3] != 0 {
                                    canvas.blend_premul_px(
                                        gx + col,
                                        gy + row,
                                        [
                                            image.data[si],
                                            image.data[si + 1],
                                            image.data[si + 2],
                                            image.data[si + 3],
                                        ],
                                    );
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}
