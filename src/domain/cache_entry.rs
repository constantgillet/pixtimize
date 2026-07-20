//! Metadata associated with a transformed image in the cache.

use serde::{Deserialize, Serialize};

/// Metadata for a cached transform stored in Redis.
///
/// Older deployments stored only the S3 key as a plain string; [`Self::parse`]
/// accepts both representations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CacheEntry {
    /// S3 object key of the transformed image.
    pub s3_key: String,
    /// Byte length of the transformed object, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    /// Content type of the transformed object, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
}

impl CacheEntry {
    /// Builds a fully populated entry for a freshly written transform.
    pub fn new(s3_key: impl Into<String>, size: u64, content_type: impl Into<String>) -> Self {
        Self {
            s3_key: s3_key.into(),
            size: Some(size),
            content_type: Some(content_type.into()),
        }
    }

    /// Parses Redis metadata or a legacy plain S3 key.
    pub fn parse(raw: &str) -> Self {
        if let Ok(entry) = serde_json::from_str::<Self>(raw)
            && !entry.s3_key.is_empty()
        {
            return entry;
        }

        Self {
            s3_key: raw.to_owned(),
            size: None,
            content_type: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_should_read_json_entry() {
        let entry =
            CacheEntry::parse(r#"{"s3_key":"cached/abc","size":1234,"content_type":"image/webp"}"#);
        assert_eq!(entry.s3_key, "cached/abc");
        assert_eq!(entry.size, Some(1234));
        assert_eq!(entry.content_type.as_deref(), Some("image/webp"));
    }

    #[test]
    fn parse_should_accept_legacy_plain_s3_key() {
        let entry = CacheEntry::parse("cached/abc");
        assert_eq!(entry.s3_key, "cached/abc");
        assert_eq!(entry.size, None);
        assert_eq!(entry.content_type, None);
    }

    #[test]
    fn new_should_populate_all_fields() {
        let entry = CacheEntry::new("cached/abc", 42, "image/jpeg");
        assert_eq!(entry.s3_key, "cached/abc");
        assert_eq!(entry.size, Some(42));
        assert_eq!(entry.content_type.as_deref(), Some("image/jpeg"));
    }
}
