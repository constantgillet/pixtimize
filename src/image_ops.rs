//! Image decoding, resizing, and re-encoding.
//!
//! JPEG and PNG are produced with the `image` crate; WebP uses libwebp via the
//! `webp` crate so that a lossy quality can be applied.

use std::io::Cursor;

use image::{DynamicImage, ImageFormat, codecs::jpeg::JpegEncoder, imageops::FilterType};

use crate::{
    error::AppError,
    limits::{MAX_IMAGE_MEGAPIXELS, MAX_WEBP_TRANSFORM_DIMENSION},
    transform::{OutputFormat, Transformations},
};

/// The filter used for all resampling. Lanczos3 mirrors sharp's default.
const RESIZE_FILTER: FilterType = FilterType::Lanczos3;

/// Decodes `source`, applies `transforms`, and returns the encoded bytes.
pub fn process(source: &[u8], transforms: &Transformations) -> Result<Vec<u8>, AppError> {
    let img = image::load_from_memory(source)
        .map_err(|err| AppError::ImageProcessing(err.to_string()))?;

    let megapixels = u64::from(img.width()) * u64::from(img.height());
    if megapixels > MAX_IMAGE_MEGAPIXELS {
        return Err(AppError::PayloadTooLarge(format!(
            "image exceeds max of {} megapixels",
            MAX_IMAGE_MEGAPIXELS / 1_000_000
        )));
    }

    let img = resize(img, transforms);
    if transforms.format == OutputFormat::WebP
        && (img.width() > MAX_WEBP_TRANSFORM_DIMENSION || img.height() > MAX_WEBP_TRANSFORM_DIMENSION)
    {
        return Err(AppError::InvalidTransform(format!(
            "WebP output exceeds max of {MAX_WEBP_TRANSFORM_DIMENSION}px"
        )));
    }

    encode(&img, transforms)
}

fn resize(img: DynamicImage, transforms: &Transformations) -> DynamicImage {
    let (ow, oh) = (img.width(), img.height());
    if ow == 0 || oh == 0 {
        return img;
    }

    match (transforms.width, transforms.height) {
        // Both dimensions: cover fit with a centered crop (sharp's default).
        (Some(w), Some(h)) => {
            let tw = resolve(w, ow);
            let th = resolve(h, oh);
            img.resize_to_fill(tw, th, RESIZE_FILTER)
        }
        // Single dimension: scale proportionally.
        (Some(w), None) => {
            let tw = resolve(w, ow);
            let th = ((u64::from(oh) * u64::from(tw)) / u64::from(ow)).max(1) as u32;
            img.resize_exact(tw, th, RESIZE_FILTER)
        }
        (None, Some(h)) => {
            let th = resolve(h, oh);
            let tw = ((u64::from(ow) * u64::from(th)) / u64::from(oh)).max(1) as u32;
            img.resize_exact(tw, th, RESIZE_FILTER)
        }
        (None, None) => img,
    }
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

fn encode(img: &DynamicImage, transforms: &Transformations) -> Result<Vec<u8>, AppError> {
    match transforms.format {
        OutputFormat::Jpeg => encode_jpeg(img, transforms.quality),
        OutputFormat::Png => encode_png(img),
        OutputFormat::WebP => encode_webp(img, transforms.quality),
    }
}

fn encode_jpeg(img: &DynamicImage, quality: u8) -> Result<Vec<u8>, AppError> {
    // JPEG has no alpha channel; drop it before encoding.
    let rgb = DynamicImage::ImageRgb8(img.to_rgb8());
    let mut buffer = Vec::new();
    let mut encoder = JpegEncoder::new_with_quality(&mut buffer, quality);
    encoder
        .encode_image(&rgb)
        .map_err(|err| AppError::ImageProcessing(err.to_string()))?;
    Ok(buffer)
}

fn encode_png(img: &DynamicImage) -> Result<Vec<u8>, AppError> {
    let mut buffer = Cursor::new(Vec::new());
    img.write_to(&mut buffer, ImageFormat::Png)
        .map_err(|err| AppError::ImageProcessing(err.to_string()))?;
    Ok(buffer.into_inner())
}

fn encode_webp(img: &DynamicImage, quality: u8) -> Result<Vec<u8>, AppError> {
    let rgba = img.to_rgba8();
    let encoder = webp::Encoder::from_rgba(rgba.as_raw(), rgba.width(), rgba.height());
    let encoded = encoder.encode(f32::from(quality));
    Ok(encoded.to_vec())
}
