//! Parsing of ImageKit-style transform strings and cache-key derivation.

use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::{
    config::Config,
    domain::limits::{MAX_TRANSFORM_DIMENSION, MAX_WEBP_TRANSFORM_DIMENSION},
};

/// Validation failures produced while parsing transform instructions.
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct TransformError(String);

impl TransformError {
    fn invalid(value: impl Into<String>) -> Self {
        Self(value.into())
    }
}

/// The output encoding requested for a transform.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum OutputFormat {
    Jpeg,
    Png,
    WebP,
}

impl OutputFormat {
    /// Parses a format token (`jpeg`, `jpg`, `png`, `webp`).
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.to_ascii_lowercase().as_str() {
            "jpeg" | "jpg" => Some(Self::Jpeg),
            "png" => Some(Self::Png),
            "webp" => Some(Self::WebP),
            _ => None,
        }
    }

    /// Returns the HTTP content type matching this format.
    pub fn content_type(self) -> &'static str {
        match self {
            Self::Jpeg => "image/jpeg",
            Self::Png => "image/png",
            Self::WebP => "image/webp",
        }
    }
}

/// A parsed and defaulted set of transform instructions.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Transformations {
    #[serde(rename = "w", skip_serializing_if = "Option::is_none")]
    pub width: Option<f64>,
    #[serde(rename = "h", skip_serializing_if = "Option::is_none")]
    pub height: Option<f64>,
    #[serde(rename = "q")]
    pub quality: u8,
    #[serde(rename = "f")]
    pub format: OutputFormat,
}

/// A source image path and its validated transformations.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedRequest {
    /// S3 key of the source image.
    pub image_path: String,
    pub transformations: Transformations,
}

impl ParsedRequest {
    /// Parses a request path and optional `tr` query value.
    pub fn parse(
        path: &str,
        tr_query: Option<&str>,
        config: &Config,
    ) -> Result<Self, TransformError> {
        let segments: Vec<&str> = path.split('/').collect();
        let is_path_transform = segments
            .get(1)
            .is_some_and(|first| first.starts_with("tr:"));
        let image_path = image_path(&segments, is_path_transform);

        let mut transformations = Transformations {
            width: None,
            height: None,
            quality: config.default_quality,
            format: OutputFormat::parse(&config.default_format)
                .ok_or_else(|| TransformError::invalid("invalid DEFAULT_FORMAT"))?,
        };

        if is_path_transform {
            apply_transforms(segments[1].trim_start_matches("tr:"), &mut transformations)?;
        }
        if let Some(query) = tr_query {
            apply_transforms(query, &mut transformations)?;
        }

        validate_webp_dimensions(&transformations)?;

        Ok(Self {
            image_path,
            transformations,
        })
    }

    /// Derives the S3 key of the cached, transformed image.
    pub fn cache_path_key(&self) -> String {
        let canonical =
            serde_json::to_string(&self.transformations).unwrap_or_else(|_| String::from("{}"));
        let input = format!("{}-{canonical}", self.image_path);
        let digest = Sha256::digest(input.as_bytes());
        format!("cached/{}", hex::encode(digest))
    }
}

fn image_path(segments: &[&str], is_path_transform: bool) -> String {
    let start = if is_path_transform { 2 } else { 1 };
    segments
        .iter()
        .skip(start)
        .copied()
        .collect::<Vec<_>>()
        .join("/")
}

fn apply_transforms(raw: &str, out: &mut Transformations) -> Result<(), TransformError> {
    for pair in raw.split(',').filter(|pair| !pair.is_empty()) {
        let (key, value) = pair
            .split_once('-')
            .ok_or_else(|| TransformError::invalid(pair))?;

        match key {
            "w" => {
                if let Some(dimension) = parse_dimension(value, pair)? {
                    out.width = Some(dimension);
                }
            }
            "h" => {
                if let Some(dimension) = parse_dimension(value, pair)? {
                    out.height = Some(dimension);
                }
            }
            "q" => {
                out.quality = value
                    .parse::<u8>()
                    .ok()
                    .filter(|quality| (1..=100).contains(quality))
                    .ok_or_else(|| TransformError::invalid(pair))?;
            }
            "f" => {
                out.format =
                    OutputFormat::parse(value).ok_or_else(|| TransformError::invalid(pair))?;
            }
            _ => {}
        }
    }
    Ok(())
}

fn parse_dimension(value: &str, pair: &str) -> Result<Option<f64>, TransformError> {
    let dimension = value
        .parse::<f64>()
        .ok()
        .filter(|value| *value > 0.0)
        .ok_or_else(|| TransformError::invalid(pair))?;

    if dimension > MAX_TRANSFORM_DIMENSION {
        return Ok(None);
    }

    Ok(Some(dimension))
}

fn validate_webp_dimensions(transformations: &Transformations) -> Result<(), TransformError> {
    if transformations.format != OutputFormat::WebP {
        return Ok(());
    }

    for (label, dimension) in [("w", transformations.width), ("h", transformations.height)] {
        if let Some(value) = dimension
            && value >= 1.0
            && value > f64::from(MAX_WEBP_TRANSFORM_DIMENSION)
        {
            return Err(TransformError::invalid(format!(
                "{label} exceeds WebP max of {MAX_WEBP_TRANSFORM_DIMENSION}px"
            )));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};

    use super::*;
    use crate::config::S3Config;

    fn test_config() -> Config {
        Config {
            address: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 3000),
            s3: S3Config {
                endpoint: String::new(),
                region: String::new(),
                access_key: String::new(),
                secret_key: String::new(),
                bucket: String::new(),
            },
            redis_url: String::new(),
            default_quality: 80,
            default_format: "webp".to_owned(),
            cache_delete_cron: String::new(),
            cached_time: 604_800,
        }
    }

    #[test]
    fn parse_should_read_query_transforms() {
        let parsed = ParsedRequest::parse(
            "/folder/image.png",
            Some("w-606,h-450,f-jpeg"),
            &test_config(),
        )
        .expect("valid transform");
        assert_eq!(parsed.image_path, "folder/image.png");
        assert_eq!(parsed.transformations.width, Some(606.0));
        assert_eq!(parsed.transformations.height, Some(450.0));
        assert_eq!(parsed.transformations.format, OutputFormat::Jpeg);
    }

    #[test]
    fn parse_should_read_path_transforms() {
        let parsed = ParsedRequest::parse("/tr:w-300,h-300/folder/image.png", None, &test_config())
            .expect("valid transform");
        assert_eq!(parsed.image_path, "folder/image.png");
        assert_eq!(parsed.transformations.width, Some(300.0));
    }

    #[test]
    fn parse_should_default_quality_and_format() {
        let parsed =
            ParsedRequest::parse("/image.png", None, &test_config()).expect("valid transform");
        assert_eq!(parsed.transformations.quality, 80);
        assert_eq!(parsed.transformations.format, OutputFormat::WebP);
        assert_eq!(parsed.transformations.width, None);
    }

    #[test]
    fn parse_should_accept_fractional_dimension() {
        let parsed =
            ParsedRequest::parse("/image.png", Some("w-0.5"), &test_config()).expect("valid");
        assert_eq!(parsed.transformations.width, Some(0.5));
    }

    #[test]
    fn parse_should_let_query_override_path() {
        let parsed = ParsedRequest::parse("/tr:w-100/image.png", Some("w-200"), &test_config())
            .expect("valid");
        assert_eq!(parsed.transformations.width, Some(200.0));
    }

    #[test]
    fn parse_should_reject_invalid_pair() {
        let result = ParsedRequest::parse("/image.png", Some("w-abc"), &test_config());
        assert!(result.is_err());
    }

    #[test]
    fn cache_key_should_be_deterministic() {
        let first =
            ParsedRequest::parse("/image.png", Some("w-100,f-png"), &test_config()).unwrap();
        let second =
            ParsedRequest::parse("/image.png", Some("w-100,f-png"), &test_config()).unwrap();
        assert_eq!(first.cache_path_key(), second.cache_path_key());
        assert!(first.cache_path_key().starts_with("cached/"));
    }

    #[test]
    fn cache_key_should_differ_for_different_transforms() {
        let first = ParsedRequest::parse("/image.png", Some("w-100"), &test_config()).unwrap();
        let second = ParsedRequest::parse("/image.png", Some("w-200"), &test_config()).unwrap();
        assert_ne!(first.cache_path_key(), second.cache_path_key());
    }

    #[test]
    fn parse_should_ignore_dimensions_above_imagekit_max() {
        let parsed =
            ParsedRequest::parse("/image.png", Some("w-65536,h-100,f-jpeg"), &test_config())
                .expect("valid");
        assert_eq!(parsed.transformations.width, None);
        assert_eq!(parsed.transformations.height, Some(100.0));
    }

    #[test]
    fn parse_should_accept_dimension_at_imagekit_max() {
        let parsed = ParsedRequest::parse("/image.png", Some("w-65535,f-jpeg"), &test_config())
            .expect("valid");
        assert_eq!(parsed.transformations.width, Some(65_535.0));
    }

    #[test]
    fn parse_should_reject_webp_dimensions_above_imagekit_max() {
        let result = ParsedRequest::parse("/image.png", Some("w-16384,f-webp"), &test_config());
        assert!(result.is_err());
    }

    #[test]
    fn parse_should_accept_webp_dimension_at_imagekit_max() {
        let parsed = ParsedRequest::parse("/image.png", Some("w-16383,f-webp"), &test_config())
            .expect("valid");
        assert_eq!(parsed.transformations.width, Some(16_383.0));
    }
}
