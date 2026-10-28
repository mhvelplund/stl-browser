//! Image thumbnails: fit inside the thumbnail square (keeping aspect) and encode as JPEG.
use std::io::Cursor;

use ::image::codecs::jpeg::JpegEncoder;
use ::image::imageops::{self, FilterType};
use ::image::{ImageReader, Limits, Rgb, RgbImage, RgbaImage};

use crate::THUMB_SIZE;

/// JPEG quality of the cached thumbnails.
pub const JPEG_QUALITY: u8 = 80;

/// What transparent pixels (mesh render backgrounds, transparent PNGs) become: JPEG has no
/// alpha. The mockup's STL tile colour.
pub const BACKGROUND: [u8; 3] = [0x2A, 0x2E, 0x34];

/// Decode `bytes` (any supported format) and fit it inside the `THUMB_SIZE` square.
pub fn image_thumb(bytes: &[u8]) -> Result<RgbaImage, String> {
    let mut reader = ImageReader::new(Cursor::new(bytes)).with_guessed_format().map_err(|e| e.to_string())?;
    let mut limits = Limits::default();
    limits.max_alloc = Some(512 * 1024 * 1024);
    reader.limits(limits);
    let img = reader.decode().map_err(|e| e.to_string())?.into_rgba8();
    Ok(fit(&img))
}

/// Scale to fit inside the square, keeping aspect (small images are scaled up too).
fn fit(img: &RgbaImage) -> RgbaImage {
    let (w, h) = img.dimensions();
    let scale = THUMB_SIZE as f64 / w.max(h).max(1) as f64;
    let (nw, nh) = (((w as f64 * scale).round() as u32).max(1), ((h as f64 * scale).round() as u32).max(1));
    imageops::resize(img, nw.min(THUMB_SIZE), nh.min(THUMB_SIZE), FilterType::Lanczos3)
}

/// Flatten onto `BACKGROUND` and encode as JPEG at `JPEG_QUALITY`.
pub fn encode_jpeg(img: &RgbaImage) -> Result<Vec<u8>, String> {
    let rgb = RgbImage::from_fn(img.width(), img.height(), |x, y| {
        let p = img.get_pixel(x, y).0;
        let a = p[3] as u32;
        Rgb(std::array::from_fn(|c| ((p[c] as u32 * a + BACKGROUND[c] as u32 * (255 - a) + 127) / 255) as u8))
    });
    let mut out = Vec::new();
    JpegEncoder::new_with_quality(&mut out, JPEG_QUALITY).encode_image(&rgb).map_err(|e| e.to_string())?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ::image::{DynamicImage, GenericImageView, ImageFormat, Rgba, RgbaImage};

    #[test]
    fn wide_image_is_fitted_not_padded() {
        let src = RgbaImage::from_pixel(400, 100, Rgba([255, 0, 0, 255]));
        let mut png = Vec::new();
        DynamicImage::ImageRgba8(src).write_to(&mut std::io::Cursor::new(&mut png), ImageFormat::Png).unwrap();
        let t = image_thumb(&png).unwrap();
        assert_eq!(t.dimensions(), (256, 64)); // JPEG has no transparency to pad with
        let jpg = encode_jpeg(&t).unwrap();
        assert_eq!(&jpg[..3], &[0xFF, 0xD8, 0xFF]);
        let back = ::image::load_from_memory(&jpg).unwrap();
        assert_eq!(back.dimensions(), (256, 64));
        let p = back.to_rgb8().get_pixel(128, 32).0;
        assert!(p[0] > 230 && p[1] < 30 && p[2] < 30, "{p:?}");
    }

    #[test]
    fn transparency_is_flattened_onto_the_tile_colour() {
        let jpg = encode_jpeg(&RgbaImage::from_pixel(16, 16, Rgba([255, 255, 255, 0]))).unwrap();
        let p = ::image::load_from_memory(&jpg).unwrap().to_rgb8().get_pixel(8, 8).0;
        for (got, want) in p.iter().zip(BACKGROUND) {
            assert!(got.abs_diff(want) <= 3, "{p:?}");
        }
    }

    #[test]
    fn quality_is_80() {
        // a noisy image: quality 80 must be clearly smaller than quality 100
        let src = RgbaImage::from_fn(256, 256, |x, y| Rgba([(x * 7 ^ y * 13) as u8, (x * y) as u8, (x + y * 3) as u8, 255]));
        let q80 = encode_jpeg(&src).unwrap();
        let mut q100 = Vec::new();
        ::image::codecs::jpeg::JpegEncoder::new_with_quality(&mut q100, 100)
            .encode_image(&DynamicImage::ImageRgba8(src).to_rgb8())
            .unwrap();
        assert!(q80.len() * 2 < q100.len(), "{} vs {}", q80.len(), q100.len());
    }

    #[test]
    fn small_image_is_not_upscaled_past_the_square() {
        let src = RgbaImage::from_pixel(10, 20, Rgba([0, 255, 0, 255]));
        let mut png = Vec::new();
        DynamicImage::ImageRgba8(src).write_to(&mut std::io::Cursor::new(&mut png), ImageFormat::Png).unwrap();
        let t = image_thumb(&png).unwrap();
        assert_eq!(t.dimensions(), (128, 256));
    }

    #[test]
    fn garbage_is_an_error() {
        assert!(image_thumb(b"not an image").is_err());
    }
}
