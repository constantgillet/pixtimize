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
        (Some(width), Some(height)) => cover_thumbnail(
            source,
            original_width,
            original_height,
            resolve(width, original_width),
            resolve(height, original_height),
        ),
        (Some(width), None) => {
            let target_width = resolve(width, original_width);
            let target_height = scaled_side(original_height, target_width, original_width);
            fit_thumbnail(source, target_width, target_height)
        }
        (None, Some(height)) => {
            let target_height = resolve(height, original_height);
            let target_width = scaled_side(original_width, target_height, original_height);
            fit_thumbnail(source, target_width, target_height)
        }
        (None, None) => unreachable!("caller only invokes thumbnail when a dimension is set"),
    }
}

/// libvips `thumbnail_buffer(width)` fits the image in a `width × width` square.
fn fit_thumbnail(
    source: &[u8],
    target_width: u32,
    target_height: u32,
) -> Result<VipsImage, AppError> {
    thumbnail_to_square(source, target_width.max(target_height))
}

/// Cover-fit via shrink-on-load: scale so the image fills the box, then center-crop.
///
/// `thumbnail_buffer` only accepts a square bounding box, so the edge is the
/// longer side of the cover-scaled image. Using the target width alone shrinks
/// portrait sources and the old fallback then stretched them to the box.
fn cover_thumbnail(
    source: &[u8],
    original_width: u32,
    original_height: u32,
    target_width: u32,
    target_height: u32,
) -> Result<VipsImage, AppError> {
    let scale = (f64::from(target_width) / f64::from(original_width))
        .max(f64::from(target_height) / f64::from(original_height));
    let cover_width = (f64::from(original_width) * scale)
        .ceil()
        .max(f64::from(target_width)) as u32;
    let cover_height = (f64::from(original_height) * scale)
        .ceil()
        .max(f64::from(target_height)) as u32;

    let loaded = thumbnail_to_square(source, cover_width.max(cover_height))?;
    let filled = fill_box(loaded, target_width, target_height)?;
    center_crop(&filled, target_width, target_height)
}

fn thumbnail_to_square(source: &[u8], edge: u32) -> Result<VipsImage, AppError> {
    ops::thumbnail_buffer(source, edge.max(1) as i32)
        .map_err(|error| vips_error("thumbnail", &error))
}

fn fill_box(
    image: VipsImage,
    target_width: u32,
    target_height: u32,
) -> Result<VipsImage, AppError> {
    let width = image.get_width();
    let height = image.get_height();
    if width >= target_width as i32 && height >= target_height as i32 {
        return Ok(image);
    }

    let scale = (f64::from(target_width) / f64::from(width))
        .max(f64::from(target_height) / f64::from(height));
    ops::resize(&image, scale).map_err(|error| vips_error("resize", &error))
}

fn center_crop(
    image: &VipsImage,
    target_width: u32,
    target_height: u32,
) -> Result<VipsImage, AppError> {
    let width = image.get_width();
    let height = image.get_height();
    let crop_width = (target_width as i32).min(width).max(1);
    let crop_height = (target_height as i32).min(height).max(1);
    let left = ((width - crop_width) / 2).max(0);
    let top = ((height - crop_height) / 2).max(0);

    let cropped = ops::extract_area(image, left, top, crop_width, crop_height)
        .map_err(|error| vips_error("crop", &error))?;

    if crop_width == target_width as i32 && crop_height == target_height as i32 {
        return Ok(cropped);
    }

    // Sub-pixel rounding can leave the crop 1px short of the requested box.
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

fn scaled_side(source: u32, target_other: u32, source_other: u32) -> u32 {
    ((u64::from(source) * u64::from(target_other)) / u64::from(source_other)).max(1) as u32
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

    fn rgb_band(width: i32, height: i32, rgb: [f64; 3]) -> VipsImage {
        let canvas = ops::black(width, height).expect("create canvas");
        VipsImage::new_from_image(&canvas, &rgb).expect("color band")
    }

    fn stacked_png(width: i32, bands: &[(i32, [f64; 3])]) -> Vec<u8> {
        init_vips();
        let image = bands
            .iter()
            .map(|&(height, rgb)| rgb_band(width, height, rgb))
            .reduce(|left, right| {
                ops::join(&left, &right, ops::Direction::Vertical).expect("stack bands")
            })
            .expect("at least one band");
        ops::pngsave_buffer(&image).expect("encode stacked png")
    }

    fn assert_green(pixel: &[f64]) {
        assert!(pixel[0] < 40.0, "red should be near 0, got {pixel:?}");
        assert!(pixel[1] > 200.0, "green should be near 255, got {pixel:?}");
        assert!(pixel[2] < 40.0, "blue should be near 0, got {pixel:?}");
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
    fn process_should_center_crop_portrait_without_stretching() {
        let red = [255.0, 0.0, 0.0];
        let green = [0.0, 255.0, 0.0];
        let blue = [0.0, 0.0, 255.0];
        // 100x200 portrait into 80x50 landscape: cover-scale is 0.8 (80x160),
        // then center-crop 80x50. A stretch-to-fit would mix the red/blue ends in.
        let source = stacked_png(100, &[(30, red), (140, green), (30, blue)]);
        let output = VipsProcessor::process(
            &source,
            &transformations(Some(80.0), Some(50.0), OutputFormat::Png),
        )
        .expect("process image");
        let decoded = VipsImage::new_from_buffer(&output, "").expect("decode output");
        assert_eq!((decoded.get_width(), decoded.get_height()), (80, 50));
        assert_green(&ops::getpoint(&decoded, 40, 2).expect("sample top"));
        assert_green(&ops::getpoint(&decoded, 40, 25).expect("sample center"));
        assert_green(&ops::getpoint(&decoded, 40, 47).expect("sample bottom"));
    }

    #[test]
    fn process_should_scale_portrait_proportionally_for_single_width() {
        let source = png_source(100, 200);
        let output = VipsProcessor::process(
            &source,
            &transformations(Some(50.0), None, OutputFormat::Png),
        )
        .expect("process image");
        let decoded = VipsImage::new_from_buffer(&output, "").expect("decode output");
        assert_eq!((decoded.get_width(), decoded.get_height()), (50, 100));
    }

    #[test]
    fn process_should_scale_portrait_proportionally_for_single_height() {
        let source = png_source(100, 200);
        let output = VipsProcessor::process(
            &source,
            &transformations(None, Some(50.0), OutputFormat::Png),
        )
        .expect("process image");
        let decoded = VipsImage::new_from_buffer(&output, "").expect("decode output");
        assert_eq!((decoded.get_width(), decoded.get_height()), (25, 50));
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
