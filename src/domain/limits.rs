//! ImageKit-compatible processing limits.

/// Max source image size accepted for processing (ImageKit free plan).
pub const MAX_IMAGE_FILE_SIZE: usize = 20 * 1024 * 1024;

/// Max source image megapixels accepted for processing (ImageKit free plan).
pub const MAX_IMAGE_MEGAPIXELS: u64 = 25_000_000;

/// Max absolute `w` / `h` in a transform string. Larger values are ignored.
pub const MAX_TRANSFORM_DIMENSION: f64 = 65_535.0;

/// Max absolute `w` / `h` (and output size) allowed for WebP transforms.
pub const MAX_WEBP_TRANSFORM_DIMENSION: u32 = 16_383;
