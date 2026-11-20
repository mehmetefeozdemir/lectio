//! A plain RGBA picture shared by the slide, photo and ink code, so the
//! desktop app (egui) and the mobile apps (UniFFI) use the same drawing.

/// Pixels as premultiplied RGBA, row by row.
#[derive(Debug, Clone, PartialEq)]
pub struct Rgba {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

impl Rgba {
    /// From straight (not premultiplied) RGBA, as image decoders give it.
    pub fn from_straight(width: u32, height: u32, straight: &[u8]) -> Self {
        let pixels = straight
            .as_chunks::<4>()
            .0
            .iter()
            .flat_map(|p| {
                let a = u16::from(p[3]);
                let mul = |c: u8| ((u16::from(c) * a + 127) / 255) as u8;
                [mul(p[0]), mul(p[1]), mul(p[2]), p[3]]
            })
            .collect();
        Self { width, height, pixels }
    }

    pub fn to_pixmap(&self) -> Result<tiny_skia::Pixmap, String> {
        let size = tiny_skia::IntSize::from_wh(self.width, self.height).ok_or("the picture is empty")?;
        tiny_skia::Pixmap::from_vec(self.pixels.clone(), size).ok_or_else(|| "the picture has the wrong size".to_owned())
    }

    pub fn to_png(&self) -> Result<Vec<u8>, String> {
        self.to_pixmap()?.encode_png().map_err(|e| format!("Saving the picture failed: {e}"))
    }
}
