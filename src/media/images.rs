//! Photos: read their size, decode them for display, encode camera snapshots.
//!
//! Phone cameras often save a photo sideways plus an EXIF tag saying
//! "rotate me 90°". `image` reports that tag as an `Orientation`, and we
//! apply it, so a portrait photo shows (and is measured) upright.

use std::io::Cursor;

use eframe::egui::ColorImage;
use image::metadata::Orientation;
use image::{DynamicImage, ImageDecoder, ImageReader};

/// Width and height as the photo should be *shown* (after EXIF rotation).
pub fn upright_size(bytes: &[u8]) -> Result<(u32, u32), String> {
    let mut decoder = decoder(bytes)?;
    let orientation = decoder.orientation().unwrap_or(Orientation::NoTransforms);
    let (w, h) = decoder.dimensions();
    Ok(if swaps_sides(orientation) { (h, w) } else { (w, h) })
}

/// Decodes, rotates upright, and shrinks so the longer side is at most
/// `max_side` pixels (GPU textures have size limits, and a 12-megapixel photo
/// would waste memory on screen).
pub fn decode_for_display(bytes: &[u8], max_side: u32) -> Result<ColorImage, String> {
    let mut decoder = decoder(bytes)?;
    let orientation = decoder.orientation().unwrap_or(Orientation::NoTransforms);
    let mut image = DynamicImage::from_decoder(decoder).map_err(|e| format!("The image could not be decoded: {e}"))?;
    image.apply_orientation(orientation);
    if image.width().max(image.height()) > max_side {
        image = image.thumbnail(max_side, max_side);
    }
    let rgba = image.to_rgba8();
    let size = [rgba.width() as usize, rgba.height() as usize];
    Ok(ColorImage::from_rgba_unmultiplied(size, rgba.as_raw()))
}

/// JPEG for a camera snapshot (RGBA in, alpha dropped). Quality 85 keeps
/// board text sharp at about a tenth of the PNG size. Only the browser has
/// a camera (ADR-013); it is unit-tested on every target.
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
pub fn encode_jpeg(rgba: &[u8], width: u32, height: u32) -> Result<Vec<u8>, String> {
    const QUALITY: u8 = 85;
    let image = image::RgbaImage::from_raw(width, height, rgba.to_vec())
        .ok_or_else(|| "the snapshot has the wrong size".to_string())?;
    let rgb = DynamicImage::ImageRgba8(image).to_rgb8();
    let mut out = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, QUALITY)
        .encode_image(&rgb)
        .map_err(|e| format!("Encoding the photo failed: {e}"))?;
    Ok(out)
}

fn decoder(bytes: &[u8]) -> Result<impl ImageDecoder + use<'_>, String> {
    ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| format!("The image could not be read: {e}"))?
        .into_decoder()
        .map_err(|e| format!("The image could not be read: {e}"))
}

fn swaps_sides(orientation: Orientation) -> bool {
    matches!(
        orientation,
        Orientation::Rotate90 | Orientation::Rotate270 | Orientation::Rotate90FlipH | Orientation::Rotate270FlipH
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(width: u32, height: u32) -> Vec<u8> {
        let image = image::RgbaImage::from_pixel(width, height, image::Rgba([200, 30, 30, 255]));
        let mut out = Cursor::new(Vec::new());
        DynamicImage::ImageRgba8(image).write_to(&mut out, image::ImageFormat::Png).unwrap();
        out.into_inner()
    }

    #[test]
    fn size_is_read_without_decoding_the_pixels() {
        assert_eq!(upright_size(&png(40, 30)).unwrap(), (40, 30));
    }

    #[test]
    fn big_images_are_shrunk_for_display_keeping_their_shape() {
        let shown = decode_for_display(&png(400, 100), 200).unwrap();
        assert_eq!(shown.size, [200, 50]);
    }

    #[test]
    fn small_images_are_shown_at_their_own_size() {
        assert_eq!(decode_for_display(&png(40, 30), 200).unwrap().size, [40, 30]);
    }

    #[test]
    fn camera_snapshots_become_jpegs_of_the_same_size() {
        let rgba = vec![128u8; 16 * 8 * 4];
        let jpeg = encode_jpeg(&rgba, 16, 8).unwrap();
        assert!(jpeg.starts_with(b"\xFF\xD8\xFF"));
        assert_eq!(upright_size(&jpeg).unwrap(), (16, 8));
    }

    #[test]
    fn garbage_is_an_error_not_a_panic() {
        assert!(upright_size(b"definitely not an image").is_err());
        assert!(encode_jpeg(&[0; 3], 16, 8).is_err());
    }
}
