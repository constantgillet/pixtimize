//! libvips adapter for image decoding, resizing, and encoding.

use anyhow::Context;
use libvips_rs::{
    VipsApp, VipsImage,
    error::Error as VipsError,
    ops::{self, ResizeOptions},
};

use crate::{
    domain::{
        limits::{MAX_IMAGE_MEGAPIXELS, MAX_WEBP_TRANSFORM_DIMENSION},
        transform::{OutputFormat, Transformations},
    },
    error::AppError,
};

/// Owns the process-wide libvips runtime and exposes image processing.
pub struct VipsProcessor {
    _app: VipsApp,
}

impl VipsProcessor {
    /// Initializes libvips for the process lifetime.
    pub fn new() -> anyhow::Result<Self> {
        let app = VipsApp::new("pixtimize", false).context("failed to initialize libvips")?;
        // Requests generally produce unique images, so libvips' operation cache
        // retains memory without offering useful reuse.
        app.cache_set_max(0);
        app.cache_set_max_mem(0);
        Ok(Self { _app: app })
    }

    /// Decodes `source`, applies `transformations`, and returns encoded bytes.
    pub fn process(source: &[u8], transformations: &Transformations) -> Result<Vec<u8>, AppError> {
        let image =
            VipsImage::new_from_buffer(source, "").map_err(|error| vips_error("decode", &error))?;
        let (original_width, original_height) = (image.get_width(), image.get_height());
        if original_width <= 0 || original_height <= 0 {
            return Err(AppError::ImageProcessing(
                "image has invalid dimensions".to_owned(),
            ));
        }

        let pixels =
            u64::from(original_width.unsigned_abs()) * u64::from(original_height.unsigned_abs());
        if pixels > MAX_IMAGE_MEGAPIXELS {
            return Err(AppError::PayloadTooLarge(format!(
                "image exceeds max of {} megapixels",
                MAX_IMAGE_MEGAPIXELS / 1_000_000
            )));
        }

        let resized = resize(
            &image,
            transformations,
            original_width as u32,
            original_height as u32,
        )?;
        let target = resized.as_ref().unwrap_or(&image);

        if transformations.format == OutputFormat::WebP {
            let (width, height) = (target.get_width(), target.get_height());
            if width > MAX_WEBP_TRANSFORM_DIMENSION as i32
                || height > MAX_WEBP_TRANSFORM_DIMENSION as i32
            {
                return Err(AppError::InvalidTransform(format!(
                    "WebP output exceeds max of {MAX_WEBP_TRANSFORM_DIMENSION}px"
                )));
            }
        }

        encode(target, transformations)
    }
}

fn resize(
    image: &VipsImage,
    transformations: &Transformations,
    original_width: u32,
    original_height: u32,
) -> Result<Option<VipsImage>, AppError> {
    let resized = match (transformations.width, transformations.height) {
        (Some(width), Some(height)) => {
            let target_width = resolve(width, original_width);
            let target_height = resolve(height, original_height);
            let (left, top, crop_width, crop_height) =
                cover_crop(original_width, original_height, target_width, target_height);
            let cropped = ops::extract_area(image, left, top, crop_width, crop_height)
                .map_err(|error| vips_error("crop", &error))?;
            scale_to(
                &cropped,
                f64::from(target_width) / f64::from(crop_width),
                f64::from(target_height) / f64::from(crop_height),
            )?
        }
        (Some(width), None) => {
            let target_width = resolve(width, original_width);
            let scale = f64::from(target_width) / f64::from(original_width);
            scale_to(image, scale, scale)?
        }
        (None, Some(height)) => {
            let target_height = resolve(height, original_height);
            let scale = f64::from(target_height) / f64::from(original_height);
            scale_to(image, scale, scale)?
        }
        (None, None) => return Ok(None),
    };

    Ok(Some(resized))
}

fn scale_to(image: &VipsImage, horizontal: f64, vertical: f64) -> Result<VipsImage, AppError> {
    let options = ResizeOptions {
        vscale: vertical,
        ..Default::default()
    };
    ops::resize_with_opts(image, horizontal, &options).map_err(|error| vips_error("resize", &error))
}

fn cover_crop(
    original_width: u32,
    original_height: u32,
    target_width: u32,
    target_height: u32,
) -> (i32, i32, i32, i32) {
    let original_width_f = f64::from(original_width);
    let original_height_f = f64::from(original_height);
    let source_aspect = original_width_f / original_height_f;
    let target_aspect = f64::from(target_width) / f64::from(target_height);

    let (crop_width, crop_height) = if source_aspect > target_aspect {
        (
            (original_height_f * target_aspect).round(),
            original_height_f,
        )
    } else {
        (original_width_f, (original_width_f / target_aspect).round())
    };
    let crop_width = (crop_width as i32).clamp(1, original_width as i32);
    let crop_height = (crop_height as i32).clamp(1, original_height as i32);
    let left = ((original_width as i32 - crop_width) / 2).max(0);
    let top = ((original_height as i32 - crop_height) / 2).max(0);
    (left, top, crop_width, crop_height)
}

fn resolve(dimension: f64, source: u32) -> u32 {
    let pixels = if dimension > 0.0 && dimension < 1.0 {
        f64::from(source) * dimension
    } else {
        dimension
    };
    pixels.round().max(1.0) as u32
}

fn encode(image: &VipsImage, transformations: &Transformations) -> Result<Vec<u8>, AppError> {
    let quality = transformations.quality;
    match transformations.format {
        OutputFormat::Jpeg => {
            let flattened;
            let target = if image.image_hasalpha() {
                flattened = ops::flatten(image).map_err(|error| vips_error("flatten", &error))?;
                &flattened
            } else {
                image
            };
            target
                .image_write_to_buffer(&format!(".jpg[Q={quality}]"))
                .map_err(|error| vips_error("encode jpeg", &error))
        }
        OutputFormat::Png => image
            .image_write_to_buffer(".png")
            .map_err(|error| vips_error("encode png", &error)),
        OutputFormat::WebP => image
            .image_write_to_buffer(&format!(".webp[Q={quality}]"))
            .map_err(|error| vips_error("encode webp", &error)),
    }
}

fn vips_error(stage: &str, error: &VipsError) -> AppError {
    let detail = current_vips_error();
    if detail.is_empty() {
        AppError::ImageProcessing(format!("vips {stage}: {error}"))
    } else {
        AppError::ImageProcessing(format!("vips {stage}: {error} ({detail})"))
    }
}

fn current_vips_error() -> String {
    // SAFETY: libvips is initialized for the process lifetime. The pointer is
    // owned by libvips and remains valid until the next call on this thread.
    unsafe {
        let pointer = libvips_rs::bindings::vips_error_buffer();
        if pointer.is_null() {
            return String::new();
        }
        let message = std::ffi::CStr::from_ptr(pointer)
            .to_string_lossy()
            .trim()
            .to_owned();
        libvips_rs::bindings::vips_error_clear();
        message
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Once;

    use super::*;

    static INIT: Once = Once::new();

    fn init_vips() {
        INIT.call_once(|| {
            let app =
                libvips_rs::VipsApp::new("pixtimize-test", false).expect("initialize libvips");
            std::mem::forget(app);
        });
    }

    fn transformations(
        width: Option<f64>,
        height: Option<f64>,
        format: OutputFormat,
    ) -> Transformations {
        Transformations {
            width,
            height,
            quality: 80,
            format,
        }
    }

    fn png_source(width: i32, height: i32) -> Vec<u8> {
        init_vips();
        let image = ops::black(width, height).expect("create test image");
        ops::pngsave_buffer(&image).expect("encode test image")
    }

    #[test]
    fn process_should_cover_fit_both_dimensions() {
        let source = png_source(200, 100);
        let output = VipsProcessor::process(
            &source,
            &transformations(Some(50.0), Some(50.0), OutputFormat::Png),
        )
        .expect("process image");
        let decoded = VipsImage::new_from_buffer(&output, "").expect("decode output");
        assert_eq!((decoded.get_width(), decoded.get_height()), (50, 50));
    }

    #[test]
    fn process_should_scale_proportionally_for_single_width() {
        let source = png_source(200, 100);
        let output = VipsProcessor::process(
            &source,
            &transformations(Some(100.0), None, OutputFormat::Png),
        )
        .expect("process image");
        let decoded = VipsImage::new_from_buffer(&output, "").expect("decode output");
        assert_eq!((decoded.get_width(), decoded.get_height()), (100, 50));
    }

    #[test]
    fn process_should_scale_proportionally_for_single_height() {
        let source = png_source(200, 100);
        let output = VipsProcessor::process(
            &source,
            &transformations(None, Some(50.0), OutputFormat::Png),
        )
        .expect("process image");
        let decoded = VipsImage::new_from_buffer(&output, "").expect("decode output");
        assert_eq!((decoded.get_width(), decoded.get_height()), (100, 50));
    }

    #[test]
    fn process_should_treat_fraction_as_percentage() {
        let source = png_source(200, 100);
        let output = VipsProcessor::process(
            &source,
            &transformations(Some(0.5), None, OutputFormat::Png),
        )
        .expect("process image");
        let decoded = VipsImage::new_from_buffer(&output, "").expect("decode output");
        assert_eq!((decoded.get_width(), decoded.get_height()), (100, 50));
    }

    #[test]
    fn process_should_pass_through_without_dimensions() {
        let source = png_source(64, 48);
        let output =
            VipsProcessor::process(&source, &transformations(None, None, OutputFormat::Png))
                .expect("process image");
        let decoded = VipsImage::new_from_buffer(&output, "").expect("decode output");
        assert_eq!((decoded.get_width(), decoded.get_height()), (64, 48));
    }

    #[test]
    fn process_should_encode_webp() {
        let source = png_source(120, 80);
        let output = VipsProcessor::process(
            &source,
            &transformations(Some(60.0), None, OutputFormat::WebP),
        )
        .expect("process image");
        assert_eq!(&output[0..4], b"RIFF");
        assert_eq!(&output[8..12], b"WEBP");
    }

    #[test]
    fn process_should_encode_jpeg() {
        let source = png_source(120, 80);
        let output = VipsProcessor::process(
            &source,
            &transformations(Some(60.0), None, OutputFormat::Jpeg),
        )
        .expect("process image");
        assert_eq!(&output[0..2], &[0xFF, 0xD8]);
    }

    #[test]
    fn cover_crop_should_trim_sides_of_wider_source() {
        assert_eq!(cover_crop(200, 100, 50, 50), (50, 0, 100, 100));
    }

    #[test]
    fn cover_crop_should_trim_top_and_bottom_of_taller_source() {
        assert_eq!(cover_crop(100, 200, 50, 50), (0, 50, 100, 100));
    }
}
