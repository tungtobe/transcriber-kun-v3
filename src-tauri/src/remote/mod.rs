//! Verified remote content boundary for ads, recommended settings, and images.
//!
//! The wire envelope is `{ "version": 1, "payload": <JSON value>,
//! "signature": <base64> }`. The Ed25519 signature covers
//! `b"trans-kun.remote.v1\\0"` followed by `serde_json::to_vec(payload)`.
//! `serde_json::Value` sorts object keys in this crate configuration, so this
//! is the one canonical byte representation the publishing tool must sign.
//! The payload is parsed for consumers only after strict signature verification.

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;
use ed25519_dalek::{Signature, VerifyingKey};
use image::{ImageFormat, ImageReader, Limits};
use reqwest::header::{ACCEPT, CONTENT_LENGTH, CONTENT_TYPE, USER_AGENT};
use reqwest::Url;
use serde::de::DeserializeOwned;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::future::Future;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::core::error::{AppError, Code};

const ENVELOPE_VERSION: u64 = 1;
const ENVELOPE_DOMAIN: &[u8] = b"trans-kun.remote.v1\0";
const CACHE_MAGIC: &[u8; 9] = b"TRREMOTE1";
const CACHE_HEADER_BYTES: usize = CACHE_MAGIC.len() + std::mem::size_of::<i64>();
const CACHE_TTL_SECONDS: i64 = 24 * 60 * 60;
const MAX_DOCUMENT_BYTES: usize = 1024 * 1024;
const MAX_IMAGE_BYTES: usize = 100 * 1024;
const MAX_IMAGE_WIDTH: u32 = 2048;
const MAX_IMAGE_HEIGHT: u32 = 2048;
const MAX_IMAGE_PIXELS: u64 = 4_194_304;
const MAX_IMAGE_DECODE_BYTES: u64 = 16 * 1024 * 1024;
const MAX_REDIRECTS: usize = 3;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);

const BUILD_REMOTE_ORIGIN: Option<&str> = option_env!("TRANS_KUN_REMOTE_ORIGIN");
const BUILD_REMOTE_PUBLIC_KEY_B64: Option<&str> =
    option_env!("TRANS_KUN_REMOTE_ED25519_PUBLIC_KEY_B64");

pub type RemoteTransportFuture =
    Pin<Box<dyn Future<Output = Result<TransportResponse, RemoteFailure>> + Send + 'static>>;

/// Stable resource names. Callers cannot turn a server-provided string into an
/// arbitrary document path or HTTP method.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteResource {
    Ads,
    RecommendedSettings,
}

impl RemoteResource {
    fn path(self) -> &'static str {
        match self {
            Self::Ads => "ads.json",
            Self::RecommendedSettings => "recommended-settings.json",
        }
    }
}

/// The origin from which the bytes arrived. This is safe to expose in logs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteSource {
    Network,
    VerifiedCache,
    Embedded,
}

/// Fresh network responses and fresh cache entries both have `Fresh`
/// provenance. Stale cache is only returned for ads; settings downloads never
/// accept cache or embedded content as a successful result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteProvenance {
    Fresh,
    StaleCache,
    EmbeddedFallback,
}

/// Classified failures contain no URL, response body, OS path, or raw library
/// error. Callers can report the class without leaking remote or local data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteFailure {
    NotConfigured,
    InvalidConfiguration,
    InvalidUrl,
    DisallowedOrigin,
    Timeout,
    Network,
    HttpStatus,
    ResponseTooLarge,
    InvalidEnvelope,
    UnsupportedVersion,
    InvalidSignature,
    CacheRead,
    CacheWrite,
    InvalidImageMime,
    ImageDigestMismatch,
    InvalidImageData,
    ImageDimensionsExceeded,
    UnsafeOutboundUrl,
}

impl RemoteFailure {
    /// Convert to the app's existing classified/redacted error shape using
    /// fixed messages only. The gateway never passes raw reqwest or path errors.
    pub fn as_app_error(self) -> AppError {
        let code = match self {
            Self::NotConfigured | Self::Network | Self::HttpStatus => Code::Network,
            Self::Timeout => Code::Timeout,
            Self::InvalidConfiguration
            | Self::InvalidUrl
            | Self::DisallowedOrigin
            | Self::UnsafeOutboundUrl => Code::Blocked,
            Self::ResponseTooLarge
            | Self::InvalidEnvelope
            | Self::UnsupportedVersion
            | Self::InvalidSignature
            | Self::InvalidImageMime
            | Self::ImageDigestMismatch
            | Self::InvalidImageData
            | Self::ImageDimensionsExceeded => Code::Format,
            Self::CacheRead | Self::CacheWrite => Code::Storage,
        };
        AppError::new(code, self.to_string())
    }
}

impl std::fmt::Display for RemoteFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let message = match self {
            Self::NotConfigured => "remote content is not configured",
            Self::InvalidConfiguration => "remote content configuration is invalid",
            Self::InvalidUrl => "remote URL is invalid",
            Self::DisallowedOrigin => "remote URL is outside the configured origin",
            Self::Timeout => "remote request timed out",
            Self::Network => "remote request failed",
            Self::HttpStatus => "remote server returned an unsuccessful status",
            Self::ResponseTooLarge => "remote response exceeded its size limit",
            Self::InvalidEnvelope => "remote signed envelope is malformed",
            Self::UnsupportedVersion => "remote signed envelope version is unsupported",
            Self::InvalidSignature => "remote signature verification failed",
            Self::CacheRead => "verified remote cache could not be read",
            Self::CacheWrite => "verified remote cache could not be written",
            Self::InvalidImageMime => "remote image MIME type does not match its bytes",
            Self::ImageDigestMismatch => "remote image digest does not match the manifest",
            Self::InvalidImageData => "remote image could not be decoded safely",
            Self::ImageDimensionsExceeded => "remote image dimensions exceed the limit",
            Self::UnsafeOutboundUrl => "outbound URL is not allowed",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for RemoteFailure {}

/// A decoded signed document plus typed provenance and any recoverable error.
#[derive(Debug, Clone, PartialEq)]
pub struct RemoteDocument<T> {
    pub payload: T,
    /// SHA-256 of the exact canonical payload bytes covered by the verified
    /// envelope signature. Embedded fallback documents have no signed digest.
    pub signed_payload_digest: Option<[u8; 32]>,
    pub source: RemoteSource,
    pub provenance: RemoteProvenance,
    pub fetched_at_unix_seconds: Option<i64>,
    /// Set when ads were served from cache or embedded content after a failed
    /// request. The returned payload remains safe and usable.
    pub recovered_from: Option<RemoteFailure>,
    /// Set when a fresh, verified response could not be persisted locally.
    pub cache_warning: Option<RemoteFailure>,
    // Prevent callers from manufacturing a "verified manifest" from an
    // embedded fallback or arbitrary local value before asking for images.
    signature_verified: bool,
}

#[cfg(test)]
pub(crate) fn test_fresh_document<T: serde::Serialize>(payload: T) -> RemoteDocument<T> {
    let canonical_payload = serde_json::to_vec(&payload).expect("test payload serializes");
    let signed_payload_digest = Some(Sha256::digest(canonical_payload).into());
    RemoteDocument {
        payload,
        signed_payload_digest,
        source: RemoteSource::Network,
        provenance: RemoteProvenance::Fresh,
        fetched_at_unix_seconds: Some(1_800_000_000),
        recovered_from: None,
        cache_warning: None,
        signature_verified: true,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageMime {
    Png,
    Jpeg,
    Webp,
}

impl ImageMime {
    fn from_header(header: &str) -> Result<Self, RemoteFailure> {
        let mime = header
            .split(';')
            .next()
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase();
        match mime.as_str() {
            "image/png" => Ok(Self::Png),
            "image/jpeg" => Ok(Self::Jpeg),
            "image/webp" => Ok(Self::Webp),
            _ => Err(RemoteFailure::InvalidImageMime),
        }
    }

    fn image_format(self) -> ImageFormat {
        match self {
            Self::Png => ImageFormat::Png,
            Self::Jpeg => ImageFormat::Jpeg,
            Self::Webp => ImageFormat::WebP,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedImage {
    pub bytes: Vec<u8>,
    pub sha256: [u8; 32],
    pub mime: ImageMime,
    pub width: u32,
    pub height: u32,
    pub source: RemoteSource,
}

#[derive(Debug, Clone)]
pub struct TransportResponse {
    pub status: u16,
    pub content_type: Option<String>,
    pub body: Vec<u8>,
}

/// Transport seam used by deterministic tests. Implementations must perform
/// GET only and enforce `max_bytes` while reading the response stream.
pub trait RemoteTransport: Send + Sync + 'static {
    fn get(&self, url: Url, max_bytes: usize) -> RemoteTransportFuture;
}

pub trait RemoteClock: Send + Sync + 'static {
    fn now_unix_seconds(&self) -> i64;
}

struct SystemRemoteClock;

impl RemoteClock for SystemRemoteClock {
    fn now_unix_seconds(&self) -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_secs().min(i64::MAX as u64) as i64)
            .unwrap_or(0)
    }
}

#[derive(Clone)]
pub struct RemoteGateway {
    origin: Url,
    verifying_key: VerifyingKey,
    cache_dir: PathBuf,
    transport: Arc<dyn RemoteTransport>,
    clock: Arc<dyn RemoteClock>,
}

impl std::fmt::Debug for RemoteGateway {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .write_str("RemoteGateway { origin: [configured], key: [embedded], cache: [local] }")
    }
}

impl RemoteGateway {
    /// Build a gateway from compile-time settings and the app data directory.
    /// The public key and origin are compiled into the binary; a missing
    /// publication configuration is a typed error and never prevents boot.
    pub fn from_app_data_dir(app_data_dir: impl AsRef<Path>) -> Result<Self, RemoteFailure> {
        let origin = BUILD_REMOTE_ORIGIN.ok_or(RemoteFailure::NotConfigured)?;
        let key = BUILD_REMOTE_PUBLIC_KEY_B64.ok_or(RemoteFailure::NotConfigured)?;
        let origin = parse_origin(origin)?;
        let key_bytes = BASE64
            .decode(key)
            .map_err(|_| RemoteFailure::InvalidConfiguration)?;
        let key_bytes: [u8; 32] = key_bytes
            .try_into()
            .map_err(|_| RemoteFailure::InvalidConfiguration)?;
        let verifying_key = VerifyingKey::from_bytes(&key_bytes)
            .map_err(|_| RemoteFailure::InvalidConfiguration)?;
        let transport = Arc::new(ReqwestTransport::new(origin.clone())?);
        Ok(Self {
            origin,
            verifying_key,
            cache_dir: app_data_dir.as_ref().join("cache").join("remote"),
            transport,
            clock: Arc::new(SystemRemoteClock),
        })
    }

    /// Fetch ads, using a verified cache entry if available and the caller's
    /// embedded creative data as the final fallback. This function never
    /// writes settings or touches transcription/recording state.
    pub async fn fetch_ads<T>(&self, embedded_fallback: T) -> RemoteDocument<T>
    where
        T: DeserializeOwned,
    {
        self.fetch_ads_resource(RemoteResource::Ads, embedded_fallback)
            .await
    }

    /// Fetch a fresh, valid recommended-settings document. Cache and embedded
    /// fallback are intentionally not successful results for this operation.
    pub async fn fetch_recommended_settings<T>(&self) -> Result<RemoteDocument<T>, RemoteFailure>
    where
        T: DeserializeOwned,
    {
        self.fetch_fresh_document(RemoteResource::RecommendedSettings)
            .await
    }

    /// Fetch an image named by a signed manifest. The URL must remain on the
    /// configured HTTPS origin, and both digest and bounded decode checks pass
    /// before bytes are persisted or returned.
    pub async fn fetch_image<T>(
        &self,
        signed_manifest: &RemoteDocument<T>,
        image_pointer: &str,
    ) -> Result<VerifiedImage, RemoteFailure>
    where
        T: serde::Serialize,
    {
        if !signed_manifest.signature_verified {
            return Err(RemoteFailure::InvalidSignature);
        }
        let manifest_value = serde_json::to_value(&signed_manifest.payload)
            .map_err(|_| RemoteFailure::InvalidEnvelope)?;
        let entry = manifest_value
            .pointer(image_pointer)
            .cloned()
            .ok_or(RemoteFailure::InvalidEnvelope)?;
        let reference: RemoteImageReference =
            serde_json::from_value(entry).map_err(|_| RemoteFailure::InvalidEnvelope)?;
        self.fetch_image_reference(&reference).await
    }

    async fn fetch_image_reference(
        &self,
        reference: &RemoteImageReference,
    ) -> Result<VerifiedImage, RemoteFailure> {
        let url = self.validate_resource_url(&reference.url)?;
        let expected_digest = parse_sha256_hex(&reference.sha256)?;
        let content_type_hint = reference.mime_type.as_str();
        let cache_path = self.cache_path_for_url(&url);

        if let Ok(Some(cached)) = read_cached_body(&cache_path, MAX_IMAGE_BYTES) {
            let age = self
                .clock
                .now_unix_seconds()
                .saturating_sub(cached.fetched_at_unix_seconds);
            if (0..=CACHE_TTL_SECONDS).contains(&age) {
                if let Ok(mut image) = validate_image(
                    cached.body,
                    content_type_hint,
                    &expected_digest,
                    RemoteSource::VerifiedCache,
                ) {
                    image.source = RemoteSource::VerifiedCache;
                    return Ok(image);
                }
            }
        }

        let response = self.transport.get(url, MAX_IMAGE_BYTES).await?;
        if !(200..300).contains(&response.status) {
            return Err(RemoteFailure::HttpStatus);
        }
        if response.body.len() > MAX_IMAGE_BYTES {
            return Err(RemoteFailure::ResponseTooLarge);
        }
        let content_type = response
            .content_type
            .as_deref()
            .ok_or(RemoteFailure::InvalidImageMime)?;
        if ImageMime::from_header(content_type_hint)? != ImageMime::from_header(content_type)? {
            return Err(RemoteFailure::InvalidImageMime);
        }
        let mut image = validate_image(
            response.body,
            content_type,
            &expected_digest,
            RemoteSource::Network,
        )?;
        // Cache persistence is best-effort after content validation. A local
        // cache failure cannot invalidate otherwise verified content.
        if let Err(error) = write_cached_body(
            &self.cache_dir,
            &cache_path,
            self.clock.now_unix_seconds(),
            &image.bytes,
            MAX_IMAGE_BYTES,
        ) {
            tracing::warn!(failure = %error, "verified remote image was not cached");
        }
        image.source = RemoteSource::Network;
        Ok(image)
    }

    async fn fetch_ads_resource<T>(
        &self,
        resource: RemoteResource,
        embedded_fallback: T,
    ) -> RemoteDocument<T>
    where
        T: DeserializeOwned,
    {
        let url = match self.resource_url(resource) {
            Ok(url) => url,
            Err(error) => {
                return RemoteDocument {
                    payload: embedded_fallback,
                    signed_payload_digest: None,
                    source: RemoteSource::Embedded,
                    provenance: RemoteProvenance::EmbeddedFallback,
                    fetched_at_unix_seconds: None,
                    recovered_from: Some(error),
                    cache_warning: None,
                    signature_verified: false,
                };
            }
        };
        let cache_path = self.cache_path_for_url(&url);
        let now = self.clock.now_unix_seconds();
        let verified_cache = read_cached_body(&cache_path, MAX_DOCUMENT_BYTES)
            .ok()
            .flatten()
            .and_then(|entry| {
                verify_envelope_with_digest::<T>(&entry.body, &self.verifying_key)
                    .ok()
                    .map(|(payload, digest)| (entry, payload, digest))
            });

        let verified_cache = match verified_cache {
            Some((entry, payload, signed_payload_digest))
                if (0..=CACHE_TTL_SECONDS)
                    .contains(&now.saturating_sub(entry.fetched_at_unix_seconds)) =>
            {
                return RemoteDocument {
                    payload,
                    signed_payload_digest: Some(signed_payload_digest),
                    source: RemoteSource::VerifiedCache,
                    provenance: RemoteProvenance::Fresh,
                    fetched_at_unix_seconds: Some(entry.fetched_at_unix_seconds),
                    recovered_from: None,
                    cache_warning: None,
                    signature_verified: true,
                }
            }
            other => other,
        };

        match self.fetch_network_document::<T>(&url, &cache_path).await {
            Ok(document) => document,
            Err(error) => {
                if let Some((entry, payload, signed_payload_digest)) = verified_cache {
                    RemoteDocument {
                        payload,
                        signed_payload_digest: Some(signed_payload_digest),
                        source: RemoteSource::VerifiedCache,
                        provenance: RemoteProvenance::StaleCache,
                        fetched_at_unix_seconds: Some(entry.fetched_at_unix_seconds),
                        recovered_from: Some(error),
                        cache_warning: None,
                        signature_verified: true,
                    }
                } else {
                    RemoteDocument {
                        payload: embedded_fallback,
                        signed_payload_digest: None,
                        source: RemoteSource::Embedded,
                        provenance: RemoteProvenance::EmbeddedFallback,
                        fetched_at_unix_seconds: None,
                        recovered_from: Some(error),
                        cache_warning: None,
                        signature_verified: false,
                    }
                }
            }
        }
    }

    async fn fetch_fresh_document<T>(
        &self,
        resource: RemoteResource,
    ) -> Result<RemoteDocument<T>, RemoteFailure>
    where
        T: DeserializeOwned,
    {
        let url = self.resource_url(resource)?;
        let cache_path = self.cache_path_for_url(&url);
        self.fetch_network_document(&url, &cache_path).await
    }

    async fn fetch_network_document<T>(
        &self,
        url: &Url,
        cache_path: &Path,
    ) -> Result<RemoteDocument<T>, RemoteFailure>
    where
        T: DeserializeOwned,
    {
        let response = self.transport.get(url.clone(), MAX_DOCUMENT_BYTES).await?;
        if !(200..300).contains(&response.status) {
            return Err(RemoteFailure::HttpStatus);
        }
        if response.body.len() > MAX_DOCUMENT_BYTES {
            return Err(RemoteFailure::ResponseTooLarge);
        }
        let (payload, signed_payload_digest) =
            verify_envelope_with_digest(&response.body, &self.verifying_key)?;
        let fetched_at = self.clock.now_unix_seconds();
        let mut warning = None;
        if let Err(error) = write_cached_body(
            &self.cache_dir,
            cache_path,
            fetched_at,
            &response.body,
            MAX_DOCUMENT_BYTES,
        ) {
            warning = Some(error);
            tracing::warn!(failure = %error, "verified remote document was not cached");
        }
        Ok(RemoteDocument {
            payload,
            signed_payload_digest: Some(signed_payload_digest),
            source: RemoteSource::Network,
            provenance: RemoteProvenance::Fresh,
            fetched_at_unix_seconds: Some(fetched_at),
            recovered_from: None,
            cache_warning: warning,
            signature_verified: true,
        })
    }

    fn resource_url(&self, resource: RemoteResource) -> Result<Url, RemoteFailure> {
        let url = self
            .origin
            .join(resource.path())
            .map_err(|_| RemoteFailure::InvalidConfiguration)?;
        self.validate_resource_url(url.as_str())
    }

    fn validate_resource_url(&self, raw: &str) -> Result<Url, RemoteFailure> {
        let url = Url::parse(raw).map_err(|_| RemoteFailure::InvalidUrl)?;
        if url.scheme() != "https"
            || !url.username().is_empty()
            || url.password().is_some()
            || url.fragment().is_some()
        {
            return Err(RemoteFailure::DisallowedOrigin);
        }
        if !same_origin(&self.origin, &url) {
            return Err(RemoteFailure::DisallowedOrigin);
        }
        Ok(url)
    }

    fn cache_path_for_url(&self, url: &Url) -> PathBuf {
        self.cache_dir
            .join(hex(&Sha256::digest(url.as_str().as_bytes())))
    }
}

struct ReqwestTransport {
    client: reqwest::Client,
}

impl ReqwestTransport {
    fn new(origin: Url) -> Result<Self, RemoteFailure> {
        let policy = reqwest::redirect::Policy::custom(move |attempt| {
            if attempt.previous().len() >= MAX_REDIRECTS
                || attempt.url().scheme() != "https"
                || !same_origin(&origin, attempt.url())
            {
                attempt.stop()
            } else {
                attempt.follow()
            }
        });
        let mut headers = reqwest::header::HeaderMap::new();
        // A constant empty value prevents reqwest from adding a build-specific
        // User-Agent. No cookies, identity, Referer, or auth headers are sent.
        headers.insert(USER_AGENT, reqwest::header::HeaderValue::from_static(""));
        headers.insert(
            ACCEPT,
            reqwest::header::HeaderValue::from_static(
                "application/json, image/png, image/jpeg, image/webp",
            ),
        );
        let client = reqwest::Client::builder()
            .https_only(true)
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(REQUEST_TIMEOUT)
            .redirect(policy)
            .no_proxy()
            .default_headers(headers)
            .build()
            .map_err(|_| RemoteFailure::InvalidConfiguration)?;
        Ok(Self { client })
    }
}

impl RemoteTransport for ReqwestTransport {
    fn get(&self, url: Url, max_bytes: usize) -> RemoteTransportFuture {
        let client = self.client.clone();
        Box::pin(async move {
            let mut response = client.get(url).send().await.map_err(map_reqwest_failure)?;
            let status = response.status().as_u16();
            let content_type = response
                .headers()
                .get(CONTENT_TYPE)
                .and_then(|value| value.to_str().ok())
                .map(str::to_owned);
            if let Some(content_length) = response
                .headers()
                .get(CONTENT_LENGTH)
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.parse::<u64>().ok())
            {
                if content_length > max_bytes as u64 {
                    return Err(RemoteFailure::ResponseTooLarge);
                }
            }
            if !(200..300).contains(&status) {
                return Ok(TransportResponse {
                    status,
                    content_type,
                    body: Vec::new(),
                });
            }

            let mut body = Vec::new();
            while let Some(chunk) = response.chunk().await.map_err(map_reqwest_failure)? {
                if body.len().saturating_add(chunk.len()) > max_bytes {
                    return Err(RemoteFailure::ResponseTooLarge);
                }
                body.extend_from_slice(&chunk);
            }
            Ok(TransportResponse {
                status,
                content_type,
                body,
            })
        })
    }
}

fn map_reqwest_failure(error: reqwest::Error) -> RemoteFailure {
    if error.is_timeout() {
        RemoteFailure::Timeout
    } else {
        // Do not format or log the raw error: it may contain a URL or platform
        // details that are outside the remote module's redaction boundary.
        RemoteFailure::Network
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SignedEnvelope {
    version: u64,
    payload: serde_json::Value,
    signature: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct RemoteImageReference {
    url: String,
    sha256: String,
    mime_type: String,
}

fn verify_envelope_with_digest<T: DeserializeOwned>(
    bytes: &[u8],
    verifying_key: &VerifyingKey,
) -> Result<(T, [u8; 32]), RemoteFailure> {
    if bytes.len() > MAX_DOCUMENT_BYTES {
        return Err(RemoteFailure::ResponseTooLarge);
    }
    let envelope: SignedEnvelope =
        serde_json::from_slice(bytes).map_err(|_| RemoteFailure::InvalidEnvelope)?;
    if envelope.version != ENVELOPE_VERSION {
        return Err(RemoteFailure::UnsupportedVersion);
    }
    let signature_bytes = BASE64
        .decode(envelope.signature)
        .map_err(|_| RemoteFailure::InvalidEnvelope)?;
    let signature = Signature::try_from(signature_bytes.as_slice())
        .map_err(|_| RemoteFailure::InvalidEnvelope)?;
    let canonical_payload =
        serde_json::to_vec(&envelope.payload).map_err(|_| RemoteFailure::InvalidEnvelope)?;
    let mut signed_bytes = Vec::with_capacity(ENVELOPE_DOMAIN.len() + canonical_payload.len());
    signed_bytes.extend_from_slice(ENVELOPE_DOMAIN);
    signed_bytes.extend_from_slice(&canonical_payload);
    verifying_key
        .verify_strict(&signed_bytes, &signature)
        .map_err(|_| RemoteFailure::InvalidSignature)?;
    let signed_payload_digest = Sha256::digest(&canonical_payload).into();
    let payload =
        serde_json::from_value(envelope.payload).map_err(|_| RemoteFailure::InvalidEnvelope)?;
    Ok((payload, signed_payload_digest))
}

#[derive(Debug)]
struct CachedBody {
    fetched_at_unix_seconds: i64,
    body: Vec<u8>,
}

fn read_cached_body(
    path: &Path,
    max_body_bytes: usize,
) -> Result<Option<CachedBody>, RemoteFailure> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(RemoteFailure::CacheRead),
    };
    let max_file_bytes = CACHE_HEADER_BYTES.saturating_add(max_body_bytes);
    let metadata_len = file.metadata().map_err(|_| RemoteFailure::CacheRead)?.len();
    if metadata_len > max_file_bytes as u64 {
        return Err(RemoteFailure::ResponseTooLarge);
    }
    let mut bounded = file.take(max_file_bytes as u64 + 1);
    let mut bytes = Vec::new();
    bounded
        .read_to_end(&mut bytes)
        .map_err(|_| RemoteFailure::CacheRead)?;
    if bytes.len() > max_file_bytes {
        return Err(RemoteFailure::ResponseTooLarge);
    }
    if bytes.len() < CACHE_HEADER_BYTES || &bytes[..CACHE_MAGIC.len()] != CACHE_MAGIC {
        return Err(RemoteFailure::CacheRead);
    }
    let timestamp_start = CACHE_MAGIC.len();
    let timestamp_end = timestamp_start + std::mem::size_of::<i64>();
    let timestamp = i64::from_be_bytes(
        bytes[timestamp_start..timestamp_end]
            .try_into()
            .map_err(|_| RemoteFailure::CacheRead)?,
    );
    Ok(Some(CachedBody {
        fetched_at_unix_seconds: timestamp,
        body: bytes[timestamp_end..].to_vec(),
    }))
}

fn write_cached_body(
    cache_dir: &Path,
    destination: &Path,
    fetched_at_unix_seconds: i64,
    body: &[u8],
    max_body_bytes: usize,
) -> Result<(), RemoteFailure> {
    if body.len() > max_body_bytes {
        return Err(RemoteFailure::ResponseTooLarge);
    }
    fs::create_dir_all(cache_dir).map_err(|_| RemoteFailure::CacheWrite)?;
    let file_name = destination
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or(RemoteFailure::CacheWrite)?;
    let temporary = cache_dir.join(format!(".{file_name}.{}.tmp", uuid::Uuid::now_v7()));
    let write_result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|_| RemoteFailure::CacheWrite)?;
        file.write_all(CACHE_MAGIC)
            .and_then(|_| file.write_all(&fetched_at_unix_seconds.to_be_bytes()))
            .and_then(|_| file.write_all(body))
            .and_then(|_| file.sync_all())
            .map_err(|_| RemoteFailure::CacheWrite)?;
        #[cfg(windows)]
        if destination.exists() {
            fs::remove_file(destination).map_err(|_| RemoteFailure::CacheWrite)?;
        }
        fs::rename(&temporary, destination).map_err(|_| RemoteFailure::CacheWrite)?;
        Ok(())
    })();
    if write_result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    write_result
}

fn validate_image(
    bytes: Vec<u8>,
    content_type: &str,
    expected_digest: &[u8; 32],
    source: RemoteSource,
) -> Result<VerifiedImage, RemoteFailure> {
    if bytes.len() > MAX_IMAGE_BYTES {
        return Err(RemoteFailure::ResponseTooLarge);
    }
    let mime = ImageMime::from_header(content_type)?;
    let guessed_format =
        image::guess_format(&bytes).map_err(|_| RemoteFailure::InvalidImageData)?;
    if guessed_format != mime.image_format() {
        return Err(RemoteFailure::InvalidImageMime);
    }
    let actual_digest: [u8; 32] = Sha256::digest(&bytes).into();
    if &actual_digest != expected_digest {
        return Err(RemoteFailure::ImageDigestMismatch);
    }

    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_IMAGE_WIDTH);
    limits.max_image_height = Some(MAX_IMAGE_HEIGHT);
    limits.max_alloc = Some(MAX_IMAGE_DECODE_BYTES);
    let dimensions_reader =
        ImageReader::with_format(std::io::Cursor::new(bytes.as_slice()), mime.image_format());
    let (width, height) = dimensions_reader
        .into_dimensions()
        .map_err(|_| RemoteFailure::InvalidImageData)?;
    if width == 0
        || height == 0
        || width > MAX_IMAGE_WIDTH
        || height > MAX_IMAGE_HEIGHT
        || u64::from(width).saturating_mul(u64::from(height)) > MAX_IMAGE_PIXELS
    {
        return Err(RemoteFailure::ImageDimensionsExceeded);
    }

    let mut reader =
        ImageReader::with_format(std::io::Cursor::new(bytes.as_slice()), mime.image_format());
    reader.limits(limits);
    let decoded = reader
        .decode()
        .map_err(|_| RemoteFailure::InvalidImageData)?;
    if decoded.width() != width || decoded.height() != height {
        return Err(RemoteFailure::ImageDimensionsExceeded);
    }
    Ok(VerifiedImage {
        bytes,
        sha256: actual_digest,
        mime,
        width,
        height,
        source,
    })
}

/// Validate a creative click target. Only credential-free HTTPS URLs with a
/// host are accepted; custom, file, javascript, and plain HTTP schemes fail.
pub fn validate_click_url(raw: &str) -> Result<Url, RemoteFailure> {
    let url = parse_outbound_url(raw)?;
    if url.scheme() != "https" || url.host_str().is_none() {
        return Err(RemoteFailure::UnsafeOutboundUrl);
    }
    Ok(url)
}

/// Validate a report target and append the creative ID using URL form
/// encoding. HTTPS and mailto are the only permitted schemes.
pub fn build_report_url(raw: &str, creative_id: &str) -> Result<Url, RemoteFailure> {
    if creative_id.is_empty()
        || creative_id.len() > 256
        || creative_id.chars().any(char::is_control)
    {
        return Err(RemoteFailure::UnsafeOutboundUrl);
    }
    let mut url = parse_outbound_url(raw)?;
    match url.scheme() {
        "https" if url.host_str().is_some() => {}
        "mailto" if url.host_str().is_none() && url.path().contains('@') => {}
        _ => return Err(RemoteFailure::UnsafeOutboundUrl),
    }
    url.query_pairs_mut()
        .append_pair("creative_id", creative_id);
    Ok(url)
}

fn parse_outbound_url(raw: &str) -> Result<Url, RemoteFailure> {
    if raw.chars().any(char::is_control) {
        return Err(RemoteFailure::UnsafeOutboundUrl);
    }
    let url = Url::parse(raw).map_err(|_| RemoteFailure::UnsafeOutboundUrl)?;
    if !url.username().is_empty() || url.password().is_some() {
        return Err(RemoteFailure::UnsafeOutboundUrl);
    }
    Ok(url)
}

fn parse_origin(raw: &str) -> Result<Url, RemoteFailure> {
    let origin = Url::parse(raw).map_err(|_| RemoteFailure::InvalidConfiguration)?;
    if origin.scheme() != "https"
        || origin.host_str().is_none()
        || !origin.username().is_empty()
        || origin.password().is_some()
        || origin.path() != "/"
        || origin.query().is_some()
        || origin.fragment().is_some()
    {
        return Err(RemoteFailure::InvalidConfiguration);
    }
    Ok(origin)
}

fn same_origin(left: &Url, right: &Url) -> bool {
    left.scheme() == "https"
        && right.scheme() == "https"
        && left.host_str() == right.host_str()
        && left.port_or_known_default() == right.port_or_known_default()
}

fn parse_sha256_hex(raw: &str) -> Result<[u8; 32], RemoteFailure> {
    if raw.len() != 64 || !raw.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(RemoteFailure::InvalidEnvelope);
    }
    let mut digest = [0u8; 32];
    for (index, byte) in digest.iter_mut().enumerate() {
        let start = index * 2;
        *byte = u8::from_str_radix(&raw[start..start + 2], 16)
            .map_err(|_| RemoteFailure::InvalidEnvelope)?;
    }
    Ok(digest)
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(DIGITS[(byte >> 4) as usize] as char);
        encoded.push(DIGITS[(byte & 0x0f) as usize] as char);
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};
    use std::collections::VecDeque;
    use std::sync::atomic::{AtomicI64, AtomicUsize, Ordering};
    use std::sync::Mutex;

    #[derive(Clone)]
    struct FakeClock(Arc<AtomicI64>);

    impl FakeClock {
        fn new(now: i64) -> Self {
            Self(Arc::new(AtomicI64::new(now)))
        }

        fn set(&self, now: i64) {
            self.0.store(now, Ordering::SeqCst);
        }
    }

    impl RemoteClock for FakeClock {
        fn now_unix_seconds(&self) -> i64 {
            self.0.load(Ordering::SeqCst)
        }
    }

    #[derive(Clone)]
    struct FakeTransport {
        responses: Arc<Mutex<VecDeque<Result<TransportResponse, RemoteFailure>>>>,
        calls: Arc<AtomicUsize>,
    }

    impl FakeTransport {
        fn new(
            responses: impl IntoIterator<Item = Result<TransportResponse, RemoteFailure>>,
        ) -> Self {
            Self {
                responses: Arc::new(Mutex::new(responses.into_iter().collect())),
                calls: Arc::new(AtomicUsize::new(0)),
            }
        }

        fn calls(&self) -> usize {
            self.calls.load(Ordering::SeqCst)
        }
    }

    impl RemoteTransport for FakeTransport {
        fn get(&self, _url: Url, _max_bytes: usize) -> RemoteTransportFuture {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let response = self
                .responses
                .lock()
                .expect("fake response lock")
                .pop_front()
                .unwrap_or(Err(RemoteFailure::Network));
            Box::pin(async move { response })
        }
    }

    fn test_gateway(
        cache_dir: &Path,
        signing_key: &SigningKey,
        transport: FakeTransport,
        clock: FakeClock,
    ) -> RemoteGateway {
        RemoteGateway {
            origin: Url::parse("https://remote.example/").expect("test origin"),
            verifying_key: signing_key.verifying_key(),
            cache_dir: cache_dir.to_path_buf(),
            transport: Arc::new(transport),
            clock: Arc::new(clock),
        }
    }

    fn response(body: Vec<u8>) -> Result<TransportResponse, RemoteFailure> {
        Ok(TransportResponse {
            status: 200,
            content_type: Some("application/json".to_owned()),
            body,
        })
    }

    fn sign_payload(payload: serde_json::Value, signing_key: &SigningKey) -> Vec<u8> {
        let canonical = serde_json::to_vec(&payload).expect("canonical payload");
        let mut message = ENVELOPE_DOMAIN.to_vec();
        message.extend_from_slice(&canonical);
        let signature = signing_key.sign(&message);
        serde_json::to_vec(&serde_json::json!({
            "version": ENVELOPE_VERSION,
            "payload": payload,
            "signature": BASE64.encode(signature.to_bytes()),
        }))
        .expect("signed envelope")
    }

    fn signing_key() -> SigningKey {
        SigningKey::from_bytes(&[7; 32])
    }

    fn signed_image_manifest(
        signing_key: &SigningKey,
        url: &str,
        digest: &str,
        mime: &str,
    ) -> RemoteDocument<serde_json::Value> {
        let payload = serde_json::json!({
            "images": [{ "url": url, "sha256": digest, "mimeType": mime }]
        });
        let envelope = sign_payload(payload, signing_key);
        let (payload, signed_payload_digest) =
            verify_envelope_with_digest(&envelope, &signing_key.verifying_key())
                .expect("test image manifest signature");
        RemoteDocument {
            payload,
            signed_payload_digest: Some(signed_payload_digest),
            source: RemoteSource::Network,
            provenance: RemoteProvenance::Fresh,
            fetched_at_unix_seconds: Some(1_800_000_000),
            recovered_from: None,
            cache_warning: None,
            signature_verified: true,
        }
    }

    #[tokio::test]
    async fn valid_signed_response_is_verified_and_cached_with_fresh_provenance() {
        let temp = tempfile::tempdir().expect("tempdir");
        let key = signing_key();
        let clock = FakeClock::new(1_800_000_000);
        let transport = FakeTransport::new([response(sign_payload(
            serde_json::json!({"revision": 4, "enabled": true}),
            &key,
        ))]);
        let gateway = test_gateway(temp.path(), &key, transport.clone(), clock);

        let document: RemoteDocument<serde_json::Value> = gateway
            .fetch_recommended_settings()
            .await
            .expect("valid signed settings");

        assert_eq!(document.payload["revision"], 4);
        assert_eq!(document.source, RemoteSource::Network);
        assert_eq!(document.provenance, RemoteProvenance::Fresh);
        assert!(temp
            .path()
            .join(hex(&Sha256::digest(
                b"https://remote.example/recommended-settings.json"
            )))
            .exists());
        assert_eq!(transport.calls(), 1);
    }

    #[tokio::test]
    async fn signed_recommendation_produces_only_changed_allowlisted_preview_rows() {
        let temp = tempfile::tempdir().expect("tempdir");
        let key = signing_key();
        let payload = serde_json::json!({
            "schemaVersion": 1,
            "settings": {
                "transcribeModel": "unchanged-model",
                "liveModel": "models/recommended-live",
                "chunkMinutes": 5,
                "apiKey": "ignored-secret",
                "consentAcceptedVersion": 300
            },
            "templates": [],
            "unknownField": "ignored"
        });
        let expected_digest: [u8; 32] =
            Sha256::digest(serde_json::to_vec(&payload).unwrap()).into();
        let transport = FakeTransport::new([response(sign_payload(payload, &key))]);
        let gateway = test_gateway(
            temp.path(),
            &key,
            transport.clone(),
            FakeClock::new(1_800_000_000),
        );
        let db = crate::db::Db::open(temp.path()).expect("database");
        db.with_connection(|conn| {
            crate::db::repo::settings::upsert_many(
                conn,
                &[("transcribeModel", "\"unchanged-model\"".to_owned())],
            )?;
            Ok(())
        })
        .unwrap();

        let document: RemoteDocument<crate::settings::recommended::RecommendedSettingsDocument> =
            gateway
                .fetch_recommended_settings()
                .await
                .expect("signed recommended document");
        assert_eq!(document.signed_payload_digest, Some(expected_digest));
        let store = crate::settings::recommended::RecommendedPreviewStore::default();
        let preview = crate::settings::recommended::preview(&db, &store, document).unwrap();

        assert_eq!(transport.calls(), 1);
        assert_eq!(preview.changes.len(), 1);
        assert_eq!(
            preview.changes[0].field,
            crate::settings::recommended::RecommendedSettingField::LiveModel
        );
        assert_eq!(preview.changes[0].proposed_value, "recommended-live");
        let raw = db
            .with_connection(|conn| Ok(crate::db::repo::settings::read_all(conn)?))
            .unwrap();
        assert_eq!(
            raw.get("transcribeModel").map(String::as_str),
            Some("\"unchanged-model\"")
        );
        assert!(raw.get("liveModel").is_none());
        assert!(serde_json::to_string(&preview)
            .unwrap()
            .find("ignored-secret")
            .is_none());
    }

    #[tokio::test]
    async fn stale_verified_cache_rescues_ads_after_offline_failure() {
        let temp = tempfile::tempdir().expect("tempdir");
        let key = signing_key();
        let clock = FakeClock::new(1_800_000_000);
        let transport = FakeTransport::new([
            response(sign_payload(
                serde_json::json!({"creative":"verified"}),
                &key,
            )),
            Err(RemoteFailure::Timeout),
        ]);
        let gateway = test_gateway(temp.path(), &key, transport, clock.clone());

        let first: RemoteDocument<serde_json::Value> = gateway
            .fetch_ads(serde_json::json!({"creative":"embedded"}))
            .await;
        assert_eq!(first.provenance, RemoteProvenance::Fresh);
        clock.set(1_800_000_000 + CACHE_TTL_SECONDS + 1);

        let stale: RemoteDocument<serde_json::Value> = gateway
            .fetch_ads(serde_json::json!({"creative":"embedded"}))
            .await;
        assert_eq!(stale.payload["creative"], "verified");
        assert_eq!(stale.source, RemoteSource::VerifiedCache);
        assert_eq!(stale.provenance, RemoteProvenance::StaleCache);
        assert_eq!(stale.recovered_from, Some(RemoteFailure::Timeout));
    }

    #[tokio::test]
    async fn tampered_response_uses_embedded_ads_fallback_without_caching() {
        let temp = tempfile::tempdir().expect("tempdir");
        let key = signing_key();
        let valid = sign_payload(serde_json::json!({"creative":"remote"}), &key);
        let mut tampered_value: serde_json::Value =
            serde_json::from_slice(&valid).expect("valid envelope JSON");
        tampered_value["payload"]["creative"] = serde_json::json!("tampered");
        let tampered = serde_json::to_vec(&tampered_value).expect("tampered envelope JSON");
        let gateway = test_gateway(
            temp.path(),
            &key,
            FakeTransport::new([response(tampered)]),
            FakeClock::new(1_800_000_000),
        );

        let document: RemoteDocument<serde_json::Value> = gateway
            .fetch_ads(serde_json::json!({"creative":"embedded"}))
            .await;

        assert_eq!(document.payload["creative"], "embedded");
        assert_eq!(document.source, RemoteSource::Embedded);
        assert_eq!(document.provenance, RemoteProvenance::EmbeddedFallback);
        assert_eq!(
            document.recovered_from,
            Some(RemoteFailure::InvalidSignature)
        );
        assert!(!temp
            .path()
            .join(hex(&Sha256::digest(b"https://remote.example/ads.json")))
            .exists());

        for (bad_response, expected_failure) in [
            (response(b"{".to_vec()), RemoteFailure::InvalidEnvelope),
            (Err(RemoteFailure::Network), RemoteFailure::Network),
        ] {
            let temp = tempfile::tempdir().expect("fallback tempdir");
            let gateway = test_gateway(
                temp.path(),
                &key,
                FakeTransport::new([bad_response]),
                FakeClock::new(1_800_000_000),
            );
            let document: RemoteDocument<serde_json::Value> = gateway
                .fetch_ads(serde_json::json!({"creative":"embedded"}))
                .await;
            assert_eq!(document.payload["creative"], "embedded");
            assert_eq!(document.provenance, RemoteProvenance::EmbeddedFallback);
            assert_eq!(document.recovered_from, Some(expected_failure));
        }
    }

    #[tokio::test]
    async fn recommended_settings_requires_a_fresh_network_response_even_with_stale_cache() {
        let temp = tempfile::tempdir().expect("tempdir");
        let key = signing_key();
        let clock = FakeClock::new(1_800_000_000);
        let gateway = test_gateway(
            temp.path(),
            &key,
            FakeTransport::new([
                response(sign_payload(serde_json::json!({"revision":1}), &key)),
                Err(RemoteFailure::Network),
            ]),
            clock.clone(),
        );

        let first: RemoteDocument<serde_json::Value> = gateway
            .fetch_recommended_settings()
            .await
            .expect("first network download");
        assert_eq!(first.provenance, RemoteProvenance::Fresh);
        clock.set(1_800_000_000 + CACHE_TTL_SECONDS + 1);
        let failure = gateway
            .fetch_recommended_settings::<serde_json::Value>()
            .await
            .expect_err("stale cache cannot count as settings download");
        assert_eq!(failure, RemoteFailure::Network);
    }

    #[tokio::test]
    async fn oversized_documents_are_rejected_before_cache_write() {
        let temp = tempfile::tempdir().expect("tempdir");
        let key = signing_key();
        let gateway = test_gateway(
            temp.path(),
            &key,
            FakeTransport::new([response(vec![b'x'; MAX_DOCUMENT_BYTES + 1])]),
            FakeClock::new(1_800_000_000),
        );
        let failure = gateway
            .fetch_recommended_settings::<serde_json::Value>()
            .await
            .expect_err("oversized response");
        assert_eq!(failure, RemoteFailure::ResponseTooLarge);
        assert!(!temp
            .path()
            .join(hex(&Sha256::digest(
                b"https://remote.example/recommended-settings.json"
            )))
            .exists());
    }

    fn png_bytes(width: u32, height: u32) -> Vec<u8> {
        let image = image::RgbaImage::from_pixel(width, height, image::Rgba([10, 20, 30, 255]));
        let mut output = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(image)
            .write_to(&mut output, ImageFormat::Png)
            .expect("encode test PNG");
        output.into_inner()
    }

    #[tokio::test]
    async fn valid_image_is_digest_bound_decoded_and_cached_by_hash() {
        let temp = tempfile::tempdir().expect("tempdir");
        let key = signing_key();
        let bytes = png_bytes(2, 3);
        let digest: [u8; 32] = Sha256::digest(&bytes).into();
        let manifest = signed_image_manifest(
            &key,
            "https://remote.example/creative.png",
            &hex(&digest),
            "image/png",
        );
        let mut image_response = response(bytes.clone()).expect("response");
        image_response.content_type = Some("image/png".to_owned());
        let gateway = test_gateway(
            temp.path(),
            &key,
            FakeTransport::new([Ok(image_response)]),
            FakeClock::new(1_800_000_000),
        );

        let image = gateway
            .fetch_image(&manifest, "/images/0")
            .await
            .expect("verified PNG");
        assert_eq!(image.width, 2);
        assert_eq!(image.height, 3);
        assert_eq!(image.mime, ImageMime::Png);
        assert_eq!(image.source, RemoteSource::Network);
        assert!(temp
            .path()
            .join(hex(&Sha256::digest(b"https://remote.example/creative.png")))
            .exists());
    }

    #[tokio::test]
    async fn image_mime_digest_oversize_and_dimensions_are_rejected_before_persistence() {
        let key = signing_key();

        let temp = tempfile::tempdir().expect("mime tempdir");
        let bytes = png_bytes(1, 1);
        let digest: [u8; 32] = Sha256::digest(&bytes).into();
        let wrong_mime_manifest =
            signed_image_manifest(&key, "https://remote.example/a", &hex(&digest), "image/png");
        let mut wrong_mime = response(bytes.clone()).expect("response");
        wrong_mime.content_type = Some("image/jpeg".to_owned());
        let gateway = test_gateway(
            temp.path(),
            &key,
            FakeTransport::new([Ok(wrong_mime)]),
            FakeClock::new(1_800_000_000),
        );
        assert_eq!(
            gateway
                .fetch_image(&wrong_mime_manifest, "/images/0")
                .await
                .expect_err("MIME mismatch"),
            RemoteFailure::InvalidImageMime
        );
        assert!(!temp
            .path()
            .join(hex(&Sha256::digest(b"https://remote.example/a")))
            .exists());

        let temp = tempfile::tempdir().expect("digest tempdir");
        let mut valid_mime = response(bytes.clone()).expect("response");
        valid_mime.content_type = Some("image/png".to_owned());
        let digest_mismatch_manifest = signed_image_manifest(
            &key,
            "https://remote.example/a",
            &hex(&[0; 32]),
            "image/png",
        );
        let gateway = test_gateway(
            temp.path(),
            &key,
            FakeTransport::new([Ok(valid_mime)]),
            FakeClock::new(1_800_000_000),
        );
        assert_eq!(
            gateway
                .fetch_image(&digest_mismatch_manifest, "/images/0")
                .await
                .expect_err("digest mismatch"),
            RemoteFailure::ImageDigestMismatch
        );
        assert!(!temp
            .path()
            .join(hex(&Sha256::digest(b"https://remote.example/a")))
            .exists());

        let temp = tempfile::tempdir().expect("size tempdir");
        let oversized = vec![0u8; MAX_IMAGE_BYTES + 1];
        let digest: [u8; 32] = Sha256::digest(&oversized).into();
        let mut too_big = response(oversized).expect("response");
        too_big.content_type = Some("image/png".to_owned());
        let oversize_manifest =
            signed_image_manifest(&key, "https://remote.example/a", &hex(&digest), "image/png");
        let gateway = test_gateway(
            temp.path(),
            &key,
            FakeTransport::new([Ok(too_big)]),
            FakeClock::new(1_800_000_000),
        );
        assert_eq!(
            gateway
                .fetch_image(&oversize_manifest, "/images/0")
                .await
                .expect_err("image byte limit"),
            RemoteFailure::ResponseTooLarge
        );
        assert!(!temp
            .path()
            .join(hex(&Sha256::digest(b"https://remote.example/a")))
            .exists());

        let temp = tempfile::tempdir().expect("dimensions tempdir");
        let too_wide = png_bytes(MAX_IMAGE_WIDTH + 1, 1);
        let digest: [u8; 32] = Sha256::digest(&too_wide).into();
        let mut dimensions = response(too_wide).expect("response");
        dimensions.content_type = Some("image/png".to_owned());
        let dimensions_manifest =
            signed_image_manifest(&key, "https://remote.example/a", &hex(&digest), "image/png");
        let gateway = test_gateway(
            temp.path(),
            &key,
            FakeTransport::new([Ok(dimensions)]),
            FakeClock::new(1_800_000_000),
        );
        assert_eq!(
            gateway
                .fetch_image(&dimensions_manifest, "/images/0")
                .await
                .expect_err("image dimension limit"),
            RemoteFailure::ImageDimensionsExceeded
        );
        assert!(!temp
            .path()
            .join(hex(&Sha256::digest(b"https://remote.example/a")))
            .exists());
    }

    #[test]
    fn outbound_urls_allow_only_https_clicks_and_https_or_mailto_reports() {
        assert!(validate_click_url("https://ads.example/offer").is_ok());
        assert!(validate_click_url("file:///etc/passwd").is_err());
        assert!(validate_click_url("javascript:alert(1)").is_err());
        assert!(validate_click_url("http://ads.example/offer").is_err());

        let report = build_report_url("https://ads.example/report?source=sidebar", "creative id/1")
            .expect("safe report URL");
        let pairs: Vec<_> = report.query_pairs().collect();
        assert!(pairs
            .iter()
            .any(|(key, value)| key == "creative_id" && value == "creative id/1"));
        assert!(build_report_url("mailto:ads@example.com", "creative-1").is_ok());
        assert!(build_report_url("javascript:alert(1)", "creative-1").is_err());
        assert!(build_report_url("file:///tmp/report", "creative-1").is_err());
    }

    #[test]
    fn resource_urls_and_config_are_restricted_to_the_https_origin() {
        assert!(parse_origin("http://remote.example/").is_err());
        assert!(parse_origin("https://remote.example/path/").is_err());
        assert!(parse_origin("https://user@remote.example/").is_err());

        let key = signing_key();
        let temp = tempfile::tempdir().expect("tempdir");
        let gateway = test_gateway(
            temp.path(),
            &key,
            FakeTransport::new([]),
            FakeClock::new(1_800_000_000),
        );
        assert!(gateway
            .validate_resource_url("https://remote.example/image.png")
            .is_ok());
        assert_eq!(
            gateway
                .validate_resource_url("https://other.example/image.png")
                .expect_err("other origin"),
            RemoteFailure::DisallowedOrigin
        );
        assert_eq!(
            gateway
                .validate_resource_url("http://remote.example/image.png")
                .expect_err("plain HTTP"),
            RemoteFailure::DisallowedOrigin
        );
    }
}
