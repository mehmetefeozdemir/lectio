//! Photos: the decoding lives in `lectio_export::images` (shared with the
//! exports and the mobile apps, ADR-029); this wraps it for egui.

use eframe::egui::ColorImage;

pub use lectio_export::images::{encode_jpeg, upright_size};

/// Decodes, rotates upright (EXIF) and shrinks so the longer side is at most
/// `max_side` pixels, as an egui image.
pub fn decode_for_display(bytes: &[u8], max_side: u32) -> Result<ColorImage, String> {
    let picture = lectio_export::images::decode(bytes, max_side)?;
    Ok(ColorImage::from_rgba_premultiplied([picture.width as usize, picture.height as usize], &picture.pixels))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn photos_become_egui_images_of_their_shown_size() {
        let image = image::RgbaImage::from_pixel(400, 100, image::Rgba([200, 30, 30, 255]));
        let mut png = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(image).write_to(&mut png, image::ImageFormat::Png).unwrap();

        let shown = decode_for_display(&png.into_inner(), 200).unwrap();

        assert_eq!(shown.size, [200, 50]);
    }
}
