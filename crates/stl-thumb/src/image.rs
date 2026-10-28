//! Image thumbnails: fit inside the thumbnail square, pad transparent, encode lossless WebP.
use std::io::Cursor;

use ::image::codecs::webp::WebPEncoder;
use ::image::imageops::{self, FilterType};
use ::image::{ImageReader, Limits, RgbaImage};

use crate::THUMB_SIZE;

/// Decode `bytes` (any supported format) into a padded `THUMB_SIZE` square.
pub fn image_thumb(bytes: &[u8]) -> Result<RgbaImage, String> {
    let mut reader = ImageReader::new(Cursor::new(bytes)).with_guessed_format().map_err(|e| e.to_string())?;
    let mut limits = Limits::default();
    limits.max_alloc = Some(512 * 1024 * 1024);
    reader.limits(limits);
    let img = reader.decode().map_err(|e| e.to_string())?.into_rgba8();
    Ok(pad(&fit(&img)))
}

/// Scale to fit inside the square, keeping aspect (small images are scaled up too).
fn fit(img: &RgbaImage) -> RgbaImage {
    let (w, h) = img.dimensions();
    let scale = THUMB_SIZE as f64 / w.max(h).max(1) as f64;
    let (nw, nh) = (((w as f64 * scale).round() as u32).max(1), ((h as f64 * scale).round() as u32).max(1));
    imageops::resize(img, nw.min(THUMB_SIZE), nh.min(THUMB_SIZE), FilterType::Lanczos3)
}

/// Centre on a transparent square.
pub(crate) fn pad(img: &RgbaImage) -> RgbaImage {
    let mut out = RgbaImage::new(THUMB_SIZE, THUMB_SIZE);
    let (w, h) = img.dimensions();
    imageops::overlay(&mut out, img, ((THUMB_SIZE - w) / 2) as i64, ((THUMB_SIZE - h) / 2) as i64);
    out
}

pub fn encode_webp(img: &RgbaImage) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    WebPEncoder::new_lossless(&mut out).encode(img, img.width(), img.height(), ::image::ExtendedColorType::Rgba8).map_err(|e| e.to_string())?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ::image::{DynamicImage, GenericImageView, ImageFormat, Rgba, RgbaImage};

    #[test]
    fn wide_image_is_fitted_and_padded() {
        let src = RgbaImage::from_pixel(400, 100, Rgba([255, 0, 0, 255]));
        let mut png = Vec::new();
        DynamicImage::ImageRgba8(src).write_to(&mut std::io::Cursor::new(&mut png), ImageFormat::Png).unwrap();
        let t = image_thumb(&png).unwrap();
        assert_eq!(t.dimensions(), (128, 128));
        assert_eq!(t.get_pixel(64, 64)[3], 255); // centre is image
        assert_eq!(t.get_pixel(64, 2)[3], 0); // top band is padding
        let webp = encode_webp(&t).unwrap();
        assert_eq!(&webp[..4], b"RIFF");
        assert_eq!(::image::load_from_memory(&webp).unwrap().dimensions(), (128, 128));
    }

    #[test]
    fn small_image_is_not_upscaled_past_the_square() {
        let src = RgbaImage::from_pixel(10, 20, Rgba([0, 255, 0, 255]));
        let mut png = Vec::new();
        DynamicImage::ImageRgba8(src).write_to(&mut std::io::Cursor::new(&mut png), ImageFormat::Png).unwrap();
        let t = image_thumb(&png).unwrap();
        assert_eq!(t.dimensions(), (128, 128));
        assert_eq!(t.get_pixel(64, 64)[3], 255);
    }

    #[test]
    fn garbage_is_an_error() {
        assert!(image_thumb(b"not an image").is_err());
    }
}
