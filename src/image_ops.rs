//! Image decoding, resizing, and re-encoding via libvips.
//!
//! The whole pipeline (decode, resize, encode) runs through libvips, which is
//! demand-driven and uses far less peak memory than decoding a full bitmap in
//! Rust. Resizing uses `vips_resize` with a Lanczos3 kernel (matching sharp's
//! default); a cover fit with both dimensions is produced by center-cropping the
//! source to the target aspect ratio and then scaling to the exact size.
//!
//! No ICC color management is applied, mirroring the previous pure-Rust
//! pipeline. libvips must be initialized once per process before any of these
//! functions are called; see [`crate::state::AppState::build`], which holds the
//! [`libvips_rs::VipsApp`] for the lifetime of the server.

use libvips_rs::VipsImage;
use libvips_rs::error::Error as VipsError;
use libvips_rs::ops::{self, ResizeOptions};

use crate::{
    error::AppError,
    limits::{MAX_IMAGE_MEGAPIXELS, MAX_WEBP_TRANSFORM_DIMENSION},
    transform::{OutputFormat, Transformations},
};

/// Decodes `source`, applies `transforms`, and returns the encoded bytes.
pub fn process(source: &[u8], transforms: &Transformations) -> Result<Vec<u8>, AppError> {
    let img =
        VipsImage::new_from_buffer(source, "").map_err(|err| vips_err("decode", &err))?;

    let (ow, oh) = (img.get_width(), img.get_height());
    if ow <= 0 || oh <= 0 {
        return Err(AppError::ImageProcessing(
            "image has invalid dimensions".to_owned(),
        ));
    }

    let megapixels = u64::from(ow.unsigned_abs()) * u64::from(oh.unsigned_abs());
    if megapixels > MAX_IMAGE_MEGAPIXELS {
        return Err(AppError::PayloadTooLarge(format!(
            "image exceeds max of {} megapixels",
            MAX_IMAGE_MEGAPIXELS / 1_000_000
        )));
    }

    let resized = resize(&img, transforms, ow as u32, oh as u32)?;
    let target = resized.as_ref().unwrap_or(&img);

    if transforms.format == OutputFormat::WebP {
        let (w, h) = (target.get_width(), target.get_height());
        if w > MAX_WEBP_TRANSFORM_DIMENSION as i32 || h > MAX_WEBP_TRANSFORM_DIMENSION as i32 {
            return Err(AppError::InvalidTransform(format!(
                "WebP output exceeds max of {MAX_WEBP_TRANSFORM_DIMENSION}px"
            )));
        }
    }

    encode(target, transforms)
}

/// Applies the requested resize, returning `None` when no resize was requested
/// (so the caller can encode the source image without an extra copy).
fn resize(
    img: &VipsImage,
    transforms: &Transformations,
    ow: u32,
    oh: u32,
) -> Result<Option<VipsImage>, AppError> {
    let resized = match (transforms.width, transforms.height) {
        // Both dimensions: cover fit with a centered crop (sharp's default).
        // Crop the source to the target aspect ratio, then scale to exact size.
        (Some(w), Some(h)) => {
            let tw = resolve(w, ow);
            let th = resolve(h, oh);
            let (left, top, cw, ch) = cover_crop(ow, oh, tw, th);
            let cropped =
                ops::extract_area(img, left, top, cw, ch).map_err(|err| vips_err("crop", &err))?;
            scale_to(&cropped, f64::from(tw) / f64::from(cw), f64::from(th) / f64::from(ch))?
        }
        // Only width: scale proportionally to the target width.
        (Some(w), None) => {
            let tw = resolve(w, ow);
            let scale = f64::from(tw) / f64::from(ow);
            scale_to(img, scale, scale)?
        }
        // Only height: scale proportionally to the target height.
        (None, Some(h)) => {
            let th = resolve(h, oh);
            let scale = f64::from(th) / f64::from(oh);
            scale_to(img, scale, scale)?
        }
        (None, None) => return Ok(None),
    };

    Ok(Some(resized))
}

/// Scales `img` by independent horizontal/vertical factors with Lanczos3.
fn scale_to(img: &VipsImage, hscale: f64, vscale: f64) -> Result<VipsImage, AppError> {
    let options = ResizeOptions {
        vscale,
        ..Default::default()
    };
    ops::resize_with_opts(img, hscale, &options).map_err(|err| vips_err("resize", &err))
}

/// Computes a centered source crop box whose aspect ratio matches `tw:th`, so a
/// subsequent scale to `(tw, th)` produces a cover fit with no distortion.
///
/// Returns `(left, top, width, height)` in source pixels.
fn cover_crop(ow: u32, oh: u32, tw: u32, th: u32) -> (i32, i32, i32, i32) {
    let (ow_f, oh_f) = (f64::from(ow), f64::from(oh));
    let src_aspect = ow_f / oh_f;
    let dst_aspect = f64::from(tw) / f64::from(th);

    let (cw, ch) = if src_aspect > dst_aspect {
        // Source is wider than the target: trim the sides.
        ((oh_f * dst_aspect).round(), oh_f)
    } else {
        // Source is taller than the target: trim top and bottom.
        (ow_f, (ow_f / dst_aspect).round())
    };

    let cw = (cw as i32).clamp(1, ow as i32);
    let ch = (ch as i32).clamp(1, oh as i32);
    let left = ((ow as i32 - cw) / 2).max(0);
    let top = ((oh as i32 - ch) / 2).max(0);
    (left, top, cw, ch)
}

/// Resolves a transform dimension against a source dimension: a value in
/// `(0, 1)` is a fraction, anything `>= 1` is an absolute pixel count.
fn resolve(dim: f64, source: u32) -> u32 {
    let pixels = if dim > 0.0 && dim < 1.0 {
        f64::from(source) * dim
    } else {
        dim
    };
    pixels.round().max(1.0) as u32
}

fn encode(img: &VipsImage, transforms: &Transformations) -> Result<Vec<u8>, AppError> {
    // Encode via libvips' suffix-options API (`.webp[Q=80]`) instead of the
    // typed `*_with_opts` helpers. The 8.18 bindings pass options (e.g. `exact`,
    // `smart_deblock`) that older runtime libvips builds reject; the suffix
    // string only sets what we ask, so it works across libvips versions.
    let quality = transforms.quality;
    match transforms.format {
        OutputFormat::Jpeg => {
            // JPEG has no alpha channel; flatten transparency onto a background.
            let flattened;
            let target = if img.image_hasalpha() {
                flattened = ops::flatten(img).map_err(|err| vips_err("flatten", &err))?;
                &flattened
            } else {
                img
            };
            target
                .image_write_to_buffer(&format!(".jpg[Q={quality}]"))
                .map_err(|err| vips_err("encode jpeg", &err))
        }
        OutputFormat::Png => img
            .image_write_to_buffer(".png")
            .map_err(|err| vips_err("encode png", &err)),
        OutputFormat::WebP => img
            .image_write_to_buffer(&format!(".webp[Q={quality}]"))
            .map_err(|err| vips_err("encode webp", &err)),
    }
}

/// Wraps a libvips error, appending its thread-local detail buffer when present.
fn vips_err(stage: &str, err: &VipsError) -> AppError {
    let detail = current_vips_error();
    if detail.is_empty() {
        AppError::ImageProcessing(format!("vips {stage}: {err}"))
    } else {
        AppError::ImageProcessing(format!("vips {stage}: {err} ({detail})"))
    }
}

/// Reads and clears libvips' thread-local error buffer for richer messages.
fn current_vips_error() -> String {
    // SAFETY: reading the process-global vips error buffer; libvips is already
    // initialized for the lifetime of the server. The returned pointer is owned
    // by libvips and valid until the next vips call on this thread.
    unsafe {
        let ptr = libvips_rs::bindings::vips_error_buffer();
        if ptr.is_null() {
            return String::new();
        }
        let message = std::ffi::CStr::from_ptr(ptr)
            .to_string_lossy()
            .trim()
            .to_owned();
        libvips_rs::bindings::vips_error_clear();
        message
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Once;

    static INIT: Once = Once::new();

    /// Initializes libvips once for the whole test binary. The app handle is
    /// intentionally leaked so `vips_shutdown` never runs mid-suite.
    fn init_vips() {
        INIT.call_once(|| {
            let app = libvips_rs::VipsApp::new("pixtimize-test", false)
                .expect("failed to init libvips for tests");
            std::mem::forget(app);
        });
    }

    fn transforms(width: Option<f64>, height: Option<f64>, format: OutputFormat) -> Transformations {
        Transformations {
            width,
            height,
            quality: 80,
            format,
        }
    }

    /// Encodes a solid test image of the given size as PNG bytes.
    fn png_source(w: i32, h: i32) -> Vec<u8> {
        init_vips();
        let img = ops::black(w, h).expect("black image");
        ops::pngsave_buffer(&img).expect("encode source png")
    }

    #[test]
    fn process_should_cover_fit_both_dimensions() {
        let source = png_source(200, 100);
        let out = process(&source, &transforms(Some(50.0), Some(50.0), OutputFormat::Png))
            .expect("process ok");
        let decoded = VipsImage::new_from_buffer(&out, "").expect("decode out");
        assert_eq!((decoded.get_width(), decoded.get_height()), (50, 50));
    }

    #[test]
    fn process_should_scale_proportionally_for_single_width() {
        let source = png_source(200, 100);
        let out = process(&source, &transforms(Some(100.0), None, OutputFormat::Png))
            .expect("process ok");
        let decoded = VipsImage::new_from_buffer(&out, "").expect("decode out");
        assert_eq!((decoded.get_width(), decoded.get_height()), (100, 50));
    }

    #[test]
    fn process_should_scale_proportionally_for_single_height() {
        let source = png_source(200, 100);
        let out = process(&source, &transforms(None, Some(50.0), OutputFormat::Png))
            .expect("process ok");
        let decoded = VipsImage::new_from_buffer(&out, "").expect("decode out");
        assert_eq!((decoded.get_width(), decoded.get_height()), (100, 50));
    }

    #[test]
    fn process_should_treat_fraction_as_percentage() {
        let source = png_source(200, 100);
        let out = process(&source, &transforms(Some(0.5), None, OutputFormat::Png))
            .expect("process ok");
        let decoded = VipsImage::new_from_buffer(&out, "").expect("decode out");
        assert_eq!((decoded.get_width(), decoded.get_height()), (100, 50));
    }

    #[test]
    fn process_should_pass_through_without_dimensions() {
        let source = png_source(64, 48);
        let out = process(&source, &transforms(None, None, OutputFormat::Png))
            .expect("process ok");
        let decoded = VipsImage::new_from_buffer(&out, "").expect("decode out");
        assert_eq!((decoded.get_width(), decoded.get_height()), (64, 48));
    }

    #[test]
    fn process_should_encode_webp() {
        let source = png_source(120, 80);
        let out = process(&source, &transforms(Some(60.0), None, OutputFormat::WebP))
            .expect("process ok");
        // WebP files begin with a RIFF container and a "WEBP" fourcc.
        assert_eq!(&out[0..4], b"RIFF");
        assert_eq!(&out[8..12], b"WEBP");
        let decoded = VipsImage::new_from_buffer(&out, "").expect("decode out");
        assert_eq!((decoded.get_width(), decoded.get_height()), (60, 40));
    }

    #[test]
    fn process_should_encode_jpeg() {
        let source = png_source(120, 80);
        let out = process(&source, &transforms(Some(60.0), None, OutputFormat::Jpeg))
            .expect("process ok");
        // JPEG files begin with the SOI marker 0xFFD8.
        assert_eq!(&out[0..2], &[0xFF, 0xD8]);
        let decoded = VipsImage::new_from_buffer(&out, "").expect("decode out");
        assert_eq!((decoded.get_width(), decoded.get_height()), (60, 40));
    }

    #[test]
    fn cover_crop_should_trim_sides_of_wider_source() {
        let (left, top, width, height) = cover_crop(200, 100, 50, 50);
        assert_eq!((width, height), (100, 100));
        assert_eq!((left, top), (50, 0));
    }

    #[test]
    fn cover_crop_should_trim_top_and_bottom_of_taller_source() {
        let (left, top, width, height) = cover_crop(100, 200, 50, 50);
        assert_eq!((width, height), (100, 100));
        assert_eq!((left, top), (0, 50));
    }
}
