//! Runtime configuration loaded from environment variables.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use crate::error::ConfigError;

const DEFAULT_PORT: u16 = 3000;
const DEFAULT_S3_ENDPOINT: &str = "https://ams3.digitaloceanspaces.com";
const DEFAULT_S3_REGION: &str = "ams3";
const DEFAULT_QUALITY: u8 = 80;
const DEFAULT_FORMAT: &str = "webp";
const DEFAULT_CACHE_DELETE_CRON: &str = "0 1 * * 1";
const DEFAULT_CACHED_TIME: u64 = 604_800;

/// Connection details for the backing S3-compatible object store.
#[derive(Debug, Clone)]
pub struct S3Config {
    pub endpoint: String,
    pub region: String,
    pub access_key: String,
    pub secret_key: String,
    pub bucket: String,
}

/// Fully resolved application configuration.
#[derive(Debug, Clone)]
pub struct Config {
    pub address: SocketAddr,
    pub s3: S3Config,
    pub redis_url: String,
    pub default_quality: u8,
    pub default_format: String,
    /// Cron expression driving the cache cleanup job.
    pub cache_delete_cron: String,
    /// `max-age` (in seconds) advertised on served images.
    pub cached_time: u64,
}

impl Config {
    /// Builds a [`Config`] from the process environment.
    ///
    /// Returns [`ConfigError`] when a required variable is missing or a
    /// provided value cannot be parsed.
    pub fn from_env() -> Result<Self, ConfigError> {
        let port = parse_optional("PORT")?.unwrap_or(DEFAULT_PORT);
        let address = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), port);

        let s3 = S3Config {
            endpoint: optional_var("S3_ENDPOINT").unwrap_or_else(|| DEFAULT_S3_ENDPOINT.to_owned()),
            region: optional_var("S3_REGION").unwrap_or_else(|| DEFAULT_S3_REGION.to_owned()),
            access_key: required_var("S3_ACCESS_KEY")?,
            secret_key: required_var("S3_SECRET_KEY")?,
            bucket: required_var("S3_BUCKET")?,
        };

        let default_quality = parse_optional::<u8>("DEFAULT_QUALITY")?
            .filter(|q| (1..=100).contains(q))
            .unwrap_or(DEFAULT_QUALITY);

        let default_format =
            optional_var("DEFAULT_FORMAT").unwrap_or_else(|| DEFAULT_FORMAT.to_owned());

        let cache_delete_cron =
            optional_var("CACHE_DELETE_CRON").unwrap_or_else(|| DEFAULT_CACHE_DELETE_CRON.to_owned());

        let cached_time = parse_optional("CACHED_TIME")?.unwrap_or(DEFAULT_CACHED_TIME);

        Ok(Self {
            address,
            s3,
            redis_url: required_var("REDIS_URL")?,
            default_quality,
            default_format,
            cache_delete_cron,
            cached_time,
        })
    }
}

fn required_var(key: &'static str) -> Result<String, ConfigError> {
    match std::env::var(key) {
        Ok(value) if !value.trim().is_empty() => Ok(value),
        _ => Err(ConfigError::MissingVar(key)),
    }
}

fn optional_var(key: &str) -> Option<String> {
    match std::env::var(key) {
        Ok(value) if !value.trim().is_empty() => Some(value),
        _ => None,
    }
}

fn parse_optional<T>(key: &'static str) -> Result<Option<T>, ConfigError>
where
    T: std::str::FromStr,
{
    match optional_var(key) {
        Some(raw) => raw
            .parse::<T>()
            .map(Some)
            .map_err(|_| ConfigError::InvalidValue { key, value: raw }),
        None => Ok(None),
    }
}
