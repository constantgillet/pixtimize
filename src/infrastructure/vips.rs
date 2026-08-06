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
    ///
    /// Downscales use libvips `thumbnail_buffer`, which applies shrink-on-load for
    /// JPEG/WebP/HEIF so large masters are not fully decoded before resizing.
    pub fn process(source: &[u8], transformations: &Transformations) -> Result<Vec<u8>, AppError> {
        // Header-only access: dimensions do not force a full pixel decode.
        let header =
            VipsImage::new_from_buffer(source, "").map_err(|error| vips_error("decode", &error))?;
        let (original_width, original_height) = (header.get_width(), header.get_height());
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

        let image = match (transformations.width, transformations.height) {
            (None, None) => header,
            (width, height) => {
                drop(header);
                thumbnail(
                    source,
                    width,
                    height,
                    original_width as u32,
                    original_height as u32,
                )?
            }
        };

        if transformations.format == OutputFormat::WebP {
            let (width, height) = (image.get_width(), image.get_height());
            if width > MAX_WEBP_TRANSFORM_DIMENSION as i32
                || height > MAX_WEBP_TRANSFORM_DIMENSION as i32
            {
                return Err(AppError::InvalidTransform(format!(
                    "WebP output exceeds max of {MAX_WEBP_TRANSFORM_DIMENSION}px"
                )));
            }
        }

        encode(&image, transformations)
    }
}

fn thumbnail(
    source: &[u8],
    width: Option<f64>,
    height: Option<f64>,
    original_width: u32,
    original_height: u32,
) -> Result<VipsImage, AppError> {
    match (width, height) {
        (Some(width), Some(height)) => {
            let target_width = resolve(width, original_width);
            let target_height = resolve(height, original_height);
            cover_thumbnail(
                source,
                original_width,
                original_height,
                target_width,
                target_height,
            )
        }
        (Some(width), None) => {
            let target_width = resolve(width, original_width);
            ops::thumbnail_buffer(source, target_width as i32)
                .map_err(|error| vips_error("thumbnail", &error))
        }
        (None, Some(height)) => {
            let target_height = resolve(height, original_height);
            let target_width = ((u64::from(original_width) * u64::from(target_height))
                / u64::from(original_height))
            .max(1) as u32;
            ops::thumbnail_buffer(source, target_width as i32)
                .map_err(|error| vips_error("thumbnail", &error))
        }
        (None, None) => unreachable!("caller only invokes thumbnail when a dimension is set"),
    }
}

/// Cover-fit via shrink-on-load: scale so the image fills the box, then center-crop.
fn cover_thumbnail(
    source: &[u8],
    original_width: u32,
    original_height: u32,
    target_width: u32,
    target_height: u32,
) -> Result<VipsImage, AppError> {
    let scale = (f64::from(target_width) / f64::from(original_width))
        .max(f64::from(target_height) / f64::from(original_height));
    let thumb_width = (f64::from(original_width) * scale)
        .round()
        .max(1.0)
        .max(f64::from(target_width)) as i32;

    let loaded = ops::thumbnail_buffer(source, thumb_width)
        .map_err(|error| vips_error("thumbnail", &error))?;
    let loaded_width = loaded.get_width();
    let loaded_height = loaded.get_height();
    let crop_width = (target_width as i32).min(loaded_width).max(1);
    let crop_height = (target_height as i32).min(loaded_height).max(1);
    let left = ((loaded_width - crop_width) / 2).max(0);
    let top = ((loaded_height - crop_height) / 2).max(0);

    let cropped = ops::extract_area(&loaded, left, top, crop_width, crop_height)
        .map_err(|error| vips_error("crop", &error))?;

    if crop_width == target_width as i32 && crop_height == target_height as i32 {
        return Ok(cropped);
    }

    scale_to(
        &cropped,
        f64::from(target_width) / f64::from(crop_width),
        f64::from(target_height) / f64::from(crop_height),
    )
}

fn scale_to(image: &VipsImage, horizontal: f64, vertical: f64) -> Result<VipsImage, AppError> {
    let options = ResizeOptions {
        vscale: vertical,
        ..Default::default()
    };
    ops::resize_with_opts(image, horizontal, &options).map_err(|error| vips_error("resize", &error))
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

    fn jpeg_source(width: i32, height: i32) -> Vec<u8> {
        init_vips();
        let image = ops::black(width, height).expect("create test image");
        image
            .image_write_to_buffer(".jpg[Q=85]")
            .expect("encode jpeg test image")
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
    fn process_should_downscale_large_jpeg() {
        let source = jpeg_source(4000, 3000);
        let output = VipsProcessor::process(
            &source,
            &transformations(Some(200.0), None, OutputFormat::Jpeg),
        )
        .expect("process large jpeg");
        let decoded = VipsImage::new_from_buffer(&output, "").expect("decode output");
        assert_eq!((decoded.get_width(), decoded.get_height()), (200, 150));
    }
}
