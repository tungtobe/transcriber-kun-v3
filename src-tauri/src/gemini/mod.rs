//! The single Gemini REST gateway.
//!
//! Feature code never constructs a URL, adds an API key header, or calls
//! `reqwest` directly. The gateway owns that contract and receives only a
//! normalized request at its transport port, which keeps consent, key
//! rotation, pagination, cancellation, and error redaction in one place.

pub mod keys;
pub mod params;

use std::collections::HashSet;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use specta::Type;
use tokio::sync::Notify;
use tokio::time::Instant;

use crate::consent;
use crate::core::error::{AppError, Code};
use crate::core::Sensitive;
use crate::secrets::KeyId;

use self::keys::{KeyPoolHandle, Priority, ReportAction, RequestOutcome};
use self::params::{
    DEFAULT_MODELS_PAGE_SIZE, GEMINI_BASE_URL, KEY_TEST_TIMEOUT, MAX_TRANSCRIBE_REQUEST_BYTES,
    MODELS_LIST_TIMEOUT, MODELS_PATH, TRANSCRIBE_CHUNK_TIMEOUT,
};

/// Preserve the Story 1.5 seam for callers that only need a consent-gated
/// one-shot operation. The real gateway below uses the same invariant.
pub fn guarded_request<T>(
    accepted_version: u32,
    declined: bool,
    transport: impl FnOnce() -> Result<T, AppError>,
) -> Result<T, AppError> {
    if !consent::is_current(accepted_version, declined) {
        return Err(AppError::new(
            Code::Blocked,
            "Gemini transport requires current consent",
        ));
    }
    transport()
}

/// The small consent snapshot needed by the gateway. IPC reads this from the
/// durable settings database immediately before invoking a gateway operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConsentSnapshot {
    pub accepted_version: u32,
    pub declined: bool,
}

impl ConsentSnapshot {
    pub const fn new(accepted_version: u32, declined: bool) -> Self {
        Self {
            accepted_version,
            declined,
        }
    }

    pub const fn is_current(self) -> bool {
        consent::is_current(self.accepted_version, self.declined)
    }
}

fn require_consent(snapshot: ConsentSnapshot) -> Result<(), AppError> {
    if snapshot.is_current() {
        Ok(())
    } else {
        Err(AppError::new(
            Code::Blocked,
            "Gemini transport requires current consent",
        ))
    }
}

/// Progress signals surfaced during a transcribe Job's Gemini calls (story
/// 2.4). All methods default to no-ops so a caller only implements what it
/// needs; `post_job`/`transcribe_chunk` keep taking `Option<Arc<dyn
/// JobObserver>>` and `None` keeps the exact previous behavior (spec Tasks:
/// "`None` giữ hành vi cũ").
pub trait JobObserver: Send + Sync {
    /// `true` right before awaiting a key lease (initial acquire or a
    /// quota/auth/server retry), `false` once that wait resolves (lease
    /// obtained or a terminal error).
    fn waiting_quota(&self, _waiting: bool) {}
    /// 1-based position of the key currently leased, in listed order. Never
    /// the key material or its opaque id.
    fn key_in_use(&self, _ordinal: u32) {}
    /// Attempt number for the current chunk request, starting at 1.
    fn attempt(&self, _attempt: u32) {}
}

/// Model families exposed to the rest of the application. Filtering is
/// performed from API capability metadata, never from a model alias/name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum ModelKind {
    Transcribe,
    Live,
    Memo,
}

/// Safe model metadata returned to the frontend and later feature modules.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ModelInfo {
    pub name: String,
    pub display_name: String,
    pub supported_generation_methods: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct KeyTestResult {
    pub key_id: KeyId,
    pub valid: bool,
}

/// A cancellation handle for one foreground gateway operation. It is
/// notification based (not a timer/poll loop), so dropping a request really
/// cancels the in-flight transport future.
#[derive(Clone, Default)]
pub struct CancellationToken {
    inner: Arc<CancellationState>,
}

#[derive(Default)]
struct CancellationState {
    cancelled: AtomicBool,
    notify: Notify,
}

impl fmt::Debug for CancellationToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CancellationToken")
            .field("cancelled", &self.is_cancelled())
            .finish()
    }
}

impl CancellationToken {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.inner.cancelled.store(true, Ordering::Release);
        self.inner.notify.notify_waiters();
    }

    pub fn is_cancelled(&self) -> bool {
        self.inner.cancelled.load(Ordering::Acquire)
    }

    /// Resolves once `cancel()` has been called. Public so a
    /// `transcribe::registry::ChunkTranscriber` fake (story 2.4 tests) can
    /// await cancellation directly instead of busy-polling `is_cancelled()`.
    pub async fn cancelled(&self) {
        loop {
            let notified = self.inner.notify.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if self.is_cancelled() {
                return;
            }
            notified.await;
        }
    }
}

/// The normalized method set accepted by the gateway transport.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpMethod {
    Get,
    Post,
}

#[derive(Clone, PartialEq, Eq)]
pub struct TransportHeader {
    pub name: String,
    pub value: Sensitive<String>,
}

impl fmt::Debug for TransportHeader {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TransportHeader")
            .field("name", &self.name)
            .field("value", &self.value)
            .finish()
    }
}

/// A request contains a path and query only — never a full URL or a key in
/// query parameters. Header values stay wrapped while crossing the port.
#[derive(Clone, PartialEq, Eq)]
pub struct TransportRequest {
    pub method: HttpMethod,
    pub path: String,
    pub query: Vec<(String, String)>,
    pub headers: Vec<TransportHeader>,
    pub body: Option<Sensitive<String>>,
}

impl fmt::Debug for TransportRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TransportRequest")
            .field("method", &self.method)
            .field("path", &self.path)
            .field("query", &self.query)
            .field("headers", &self.headers)
            .field("body", &self.body.as_ref().map(|_| "[redacted]"))
            .finish()
    }
}

impl TransportRequest {
    fn models(secret: Sensitive<String>, page_token: Option<&str>) -> Self {
        let mut query = vec![("pageSize".to_string(), DEFAULT_MODELS_PAGE_SIZE.to_string())];
        if let Some(page_token) = page_token {
            query.push(("pageToken".to_string(), page_token.to_owned()));
        }
        Self {
            method: HttpMethod::Get,
            path: MODELS_PATH.to_string(),
            query,
            headers: vec![TransportHeader {
                name: "x-goog-api-key".to_string(),
                value: secret,
            }],
            body: None,
        }
    }

    fn job_post(path: &str, secret: Sensitive<String>, body: String) -> Self {
        Self {
            method: HttpMethod::Post,
            path: path.to_string(),
            query: Vec::new(),
            headers: vec![
                TransportHeader {
                    name: "x-goog-api-key".to_string(),
                    value: secret,
                },
                TransportHeader {
                    name: "content-type".to_string(),
                    value: Sensitive::new("application/json".to_string()),
                },
            ],
            body: Some(Sensitive::new(body)),
        }
    }

    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|header| header.name.eq_ignore_ascii_case(name))
            .map(|header| header.value.expose().as_str())
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct TransportResponse {
    pub status: u16,
    pub body: String,
}

impl fmt::Debug for TransportResponse {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TransportResponse")
            .field("status", &self.status)
            .field("body", &"[redacted]")
            .finish()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportError {
    Tls,
    Timeout,
    Network,
}

pub type TransportFuture =
    Pin<Box<dyn Future<Output = Result<TransportResponse, TransportError>> + Send>>;

/// Port used by the gateway. Production uses [`ReqwestTransport`]; tests
/// provide a deterministic fake that can lock method/path/header and return
/// pages or transport failures without opening a socket.
pub trait GeminiTransport: Send + Sync + 'static {
    fn send(&self, request: TransportRequest) -> TransportFuture;
}

/// Production REST transport. Reqwest's `rustls` feature is configured in
/// Cargo.toml with its platform verifier, so OS trust stores (including
/// corporate proxy CAs) are used and bundled WebPKI roots are not.
#[derive(Clone)]
pub struct ReqwestTransport {
    client: reqwest::Client,
    base_url: reqwest::Url,
}

impl fmt::Debug for ReqwestTransport {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ReqwestTransport { client: [redacted], base_url: [redacted] }")
    }
}

impl ReqwestTransport {
    pub fn new() -> Result<Self, AppError> {
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| AppError::new(Code::Tls, "Gemini TLS client could not be initialized"))?;
        let base_url = reqwest::Url::parse(GEMINI_BASE_URL)
            .map_err(|_| AppError::new(Code::Tls, "Gemini endpoint could not be initialized"))?;
        Ok(Self { client, base_url })
    }

    pub fn with_client(client: reqwest::Client) -> Result<Self, AppError> {
        let base_url = reqwest::Url::parse(GEMINI_BASE_URL)
            .map_err(|_| AppError::new(Code::Tls, "Gemini endpoint could not be initialized"))?;
        Ok(Self { client, base_url })
    }
}

impl GeminiTransport for ReqwestTransport {
    fn send(&self, request: TransportRequest) -> TransportFuture {
        let client = self.client.clone();
        let base_url = self.base_url.clone();
        Box::pin(async move {
            let mut url = base_url
                .join(&request.path)
                .map_err(|_| TransportError::Network)?;
            {
                let mut pairs = url.query_pairs_mut();
                for (name, value) in &request.query {
                    pairs.append_pair(name, value);
                }
            }

            let method = match request.method {
                HttpMethod::Get => reqwest::Method::GET,
                HttpMethod::Post => reqwest::Method::POST,
            };
            let mut builder = client.request(method, url);
            for header in request.headers {
                let name = reqwest::header::HeaderName::from_bytes(header.name.as_bytes())
                    .map_err(|_| TransportError::Network)?;
                let value = reqwest::header::HeaderValue::from_str(header.value.expose())
                    .map_err(|_| TransportError::Network)?;
                builder = builder.header(name, value);
            }
            if let Some(body) = request.body {
                builder = builder.body(body.into_inner());
            }

            let response = builder.send().await.map_err(map_reqwest_error)?;
            let status = response.status().as_u16();
            let body = response.text().await.map_err(map_reqwest_error)?;
            Ok(TransportResponse { status, body })
        })
    }
}

fn map_reqwest_error(error: reqwest::Error) -> TransportError {
    if error.is_timeout() {
        return TransportError::Timeout;
    }

    let diagnostic = format!("{error:?}").to_ascii_lowercase();
    if [
        "tls",
        "rustls",
        "certificate",
        "cert verify",
        "unknown issuer",
        "invalid peer certificate",
    ]
    .iter()
    .any(|marker| diagnostic.contains(marker))
    {
        TransportError::Tls
    } else {
        TransportError::Network
    }
}

#[derive(Clone)]
pub struct GeminiGateway {
    transport: Arc<dyn GeminiTransport>,
    key_pool: KeyPoolHandle,
}

impl fmt::Debug for GeminiGateway {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GeminiGateway")
            .field("transport", &"[redacted]")
            .field("key_pool", &"[actor]")
            .finish()
    }
}

impl GeminiGateway {
    pub fn new(transport: Arc<dyn GeminiTransport>, key_pool: KeyPoolHandle) -> Self {
        Self {
            transport,
            key_pool,
        }
    }

    pub fn production(key_pool: KeyPoolHandle) -> Result<Self, AppError> {
        Ok(Self::new(Arc::new(ReqwestTransport::new()?), key_pool))
    }

    pub async fn models_list(
        &self,
        kind: ModelKind,
        consent: ConsentSnapshot,
        cancellation: CancellationToken,
    ) -> Result<Vec<ModelInfo>, AppError> {
        self.models_list_with_timeout(kind, consent, cancellation, MODELS_LIST_TIMEOUT)
            .await
    }

    pub async fn list_models(
        &self,
        kind: ModelKind,
        consent: ConsentSnapshot,
        cancellation: CancellationToken,
    ) -> Result<Vec<ModelInfo>, AppError> {
        self.models_list(kind, consent, cancellation).await
    }

    pub async fn models_list_with_timeout(
        &self,
        kind: ModelKind,
        consent: ConsentSnapshot,
        cancellation: CancellationToken,
        timeout: Duration,
    ) -> Result<Vec<ModelInfo>, AppError> {
        require_consent(consent)?;
        let deadline = Instant::now() + timeout;
        let priority = priority_for(kind);
        let mut next_page_token = None;
        let mut seen_page_tokens = HashSet::new();
        let mut models = Vec::new();
        let mut seen_models = HashSet::new();

        loop {
            let remaining = remaining(deadline)?;
            let acquire = self.key_pool.acquire_with_budget(priority, remaining);
            tokio::pin!(acquire);
            let mut lease = tokio::select! {
                result = &mut acquire => result?,
                _ = cancellation.cancelled() => return Err(cancelled_error()),
            };

            let page = loop {
                if cancellation.is_cancelled() {
                    let _ = self.key_pool.cancel(lease.request_id.clone()).await;
                    return Err(cancelled_error());
                }
                let request =
                    TransportRequest::models(lease.secret.clone(), next_page_token.as_deref());
                let response = self
                    .send(request, deadline, &cancellation, &lease.request_id)
                    .await;
                let response = match response {
                    Ok(response) => response,
                    Err(error) => {
                        let _ = self.key_pool.report(lease, outcome_for_error(&error)).await;
                        return Err(error);
                    }
                };

                if let Some((outcome, error)) = classify_http_status(response.status) {
                    match outcome {
                        RequestOutcome::Quota | RequestOutcome::Auth | RequestOutcome::Server => {
                            let request_id = lease.request_id.clone();
                            let report = self.key_pool.report(lease, outcome);
                            tokio::pin!(report);
                            let reported = tokio::select! {
                                result = &mut report => result,
                                _ = cancellation.cancelled() => {
                                    let _ = self.key_pool.cancel(request_id).await;
                                    return Err(cancelled_error());
                                }
                            };
                            match reported {
                                Ok(ReportAction::Retry(next)) => {
                                    lease = next;
                                    continue;
                                }
                                Ok(ReportAction::Complete) => return Err(error),
                                Err(report_error) => return Err(report_error),
                            }
                        }
                        RequestOutcome::Request | RequestOutcome::Timeout => {
                            let _ = self.key_pool.report(lease, outcome).await;
                            return Err(error);
                        }
                        RequestOutcome::Success => unreachable!(),
                    }
                }

                let parsed = match parse_models_page(&response.body) {
                    Ok(parsed) => parsed,
                    Err(error) => {
                        let _ = self.key_pool.report(lease, RequestOutcome::Request).await;
                        return Err(error);
                    }
                };
                let _ = self.key_pool.report(lease, RequestOutcome::Success).await;
                break parsed;
            };

            for model in page.models {
                if supports_kind(&model, kind) && seen_models.insert(model.name.clone()) {
                    models.push(model);
                }
            }

            let Some(token) = page.next_page_token else {
                break;
            };
            if token.is_empty() || !seen_page_tokens.insert(token.clone()) {
                return Err(AppError::new(
                    Code::Shape,
                    "Gemini models pagination token repeated or invalid",
                ));
            }
            next_page_token = Some(token);
        }

        Ok(models)
    }

    pub async fn keys_test(
        &self,
        key_id: KeyId,
        consent: ConsentSnapshot,
        cancellation: CancellationToken,
    ) -> Result<KeyTestResult, AppError> {
        self.keys_test_with_timeout(key_id, consent, cancellation, KEY_TEST_TIMEOUT)
            .await
    }

    pub async fn keys_test_with_timeout(
        &self,
        key_id: KeyId,
        consent: ConsentSnapshot,
        cancellation: CancellationToken,
        timeout: Duration,
    ) -> Result<KeyTestResult, AppError> {
        require_consent(consent)?;
        let deadline = Instant::now() + timeout;
        let remaining = remaining(deadline)?;
        let lease = {
            let acquire = self
                .key_pool
                .acquire_for_key(&key_id, Priority::Job, remaining);
            tokio::pin!(acquire);
            tokio::select! {
                result = &mut acquire => result?,
                _ = cancellation.cancelled() => return Err(cancelled_error()),
            }
        };
        let request = TransportRequest::models(lease.secret.clone(), None);
        let response = match self
            .send(request, deadline, &cancellation, &lease.request_id)
            .await
        {
            Ok(response) => response,
            Err(error) => {
                let _ = self
                    .key_pool
                    .report_for_key(lease, outcome_for_error(&error))
                    .await;
                return Err(error);
            }
        };

        if let Some((outcome, error)) = classify_http_status(response.status) {
            let report = self.key_pool.report_for_key(lease, outcome).await;
            return match report {
                Ok(()) => Err(error),
                Err(report_error) if report_error.code == Code::Auth => Err(report_error),
                Err(_) => Err(error),
            };
        }

        if let Err(error) = parse_models_page(&response.body) {
            let _ = self
                .key_pool
                .report_for_key(lease, RequestOutcome::Success)
                .await;
            return Err(error);
        }
        self.key_pool
            .report_for_key(lease, RequestOutcome::Success)
            .await?;
        Ok(KeyTestResult {
            key_id,
            valid: true,
        })
    }

    /// Send one sensitive JSON request for a transcribe Job through the shared
    /// consent, key-pool, cancellation, and deadline path. The deadline starts
    /// before key acquisition, so queueing and transport share the same 120 s.
    pub async fn post_job(
        &self,
        path: &str,
        body: String,
        consent: ConsentSnapshot,
        cancellation: CancellationToken,
    ) -> Result<TransportResponse, AppError> {
        self.post_job_observed(path, body, consent, cancellation, None)
            .await
    }

    /// Same contract as [`Self::post_job`], plus optional progress signals
    /// for a transcribe Job (story 2.4 Tasks). `observer: None` behaves
    /// exactly like [`Self::post_job`].
    pub async fn post_job_observed(
        &self,
        path: &str,
        body: String,
        consent: ConsentSnapshot,
        cancellation: CancellationToken,
        observer: Option<Arc<dyn JobObserver>>,
    ) -> Result<TransportResponse, AppError> {
        require_consent(consent)?;
        validate_job_path(path)?;
        if body.len() >= MAX_TRANSCRIBE_REQUEST_BYTES {
            return Err(AppError::new(
                Code::Format,
                "Gemini transcribe request exceeds the inline JSON size limit",
            ));
        }

        let body = Sensitive::new(body);
        let deadline = Instant::now() + TRANSCRIBE_CHUNK_TIMEOUT;
        let mut lease = {
            let budget = remaining(deadline)?;
            if let Some(observer) = &observer {
                observer.waiting_quota(true);
            }
            let acquire = self.key_pool.acquire_with_budget(Priority::Job, budget);
            tokio::pin!(acquire);
            let result = tokio::select! {
                result = &mut acquire => result,
                _ = cancellation.cancelled() => return Err(cancelled_error()),
            };
            if let Some(observer) = &observer {
                observer.waiting_quota(false);
            }
            result?
        };
        self.report_lease_acquired(&lease, &observer).await;

        loop {
            if cancellation.is_cancelled() {
                let _ = self.key_pool.cancel(lease.request_id.clone()).await;
                return Err(cancelled_error());
            }
            let request =
                TransportRequest::job_post(path, lease.secret.clone(), body.expose().clone());
            let response = match self
                .send(request, deadline, &cancellation, &lease.request_id)
                .await
            {
                Ok(response) => response,
                Err(error) => {
                    let outcome = outcome_for_error(&error);
                    if outcome == RequestOutcome::Server {
                        // Same retry-then-continue path as a 5xx HTTP status
                        // below: a transport network blip stays inside the
                        // attempt budget instead of failing the chunk on the
                        // first try.
                        let request_id = lease.request_id.clone();
                        if let Some(observer) = &observer {
                            observer.waiting_quota(true);
                        }
                        let report = self.key_pool.report(lease, outcome);
                        tokio::pin!(report);
                        let reported = tokio::select! {
                            result = &mut report => result,
                            _ = cancellation.cancelled() => {
                                let _ = self.key_pool.cancel(request_id).await;
                                return Err(cancelled_error());
                            }
                        };
                        if let Some(observer) = &observer {
                            observer.waiting_quota(false);
                        }
                        match reported {
                            Ok(ReportAction::Retry(next)) => {
                                lease = next;
                                self.report_lease_acquired(&lease, &observer).await;
                                continue;
                            }
                            Ok(ReportAction::Complete) => return Err(error),
                            Err(report_error) => return Err(report_error),
                        }
                    }
                    let _ = self.key_pool.report(lease, outcome).await;
                    return Err(error);
                }
            };

            if let Some((outcome, error)) = classify_http_status(response.status) {
                match outcome {
                    RequestOutcome::Quota | RequestOutcome::Auth | RequestOutcome::Server => {
                        let request_id = lease.request_id.clone();
                        if let Some(observer) = &observer {
                            observer.waiting_quota(true);
                        }
                        let report = self.key_pool.report(lease, outcome);
                        tokio::pin!(report);
                        let reported = tokio::select! {
                            result = &mut report => result,
                            _ = cancellation.cancelled() => {
                                let _ = self.key_pool.cancel(request_id).await;
                                return Err(cancelled_error());
                            }
                        };
                        if let Some(observer) = &observer {
                            observer.waiting_quota(false);
                        }
                        match reported {
                            Ok(ReportAction::Retry(next)) => {
                                lease = next;
                                self.report_lease_acquired(&lease, &observer).await;
                            }
                            Ok(ReportAction::Complete) => return Err(error),
                            Err(report_error) => return Err(report_error),
                        }
                    }
                    RequestOutcome::Request | RequestOutcome::Timeout => {
                        let _ = self.key_pool.report(lease, outcome).await;
                        return Err(error);
                    }
                    RequestOutcome::Success => unreachable!(),
                }
                continue;
            }

            self.key_pool.report(lease, RequestOutcome::Success).await?;
            return Ok(response);
        }
    }

    async fn report_lease_acquired(
        &self,
        lease: &self::keys::KeyLease,
        observer: &Option<Arc<dyn JobObserver>>,
    ) {
        let Some(observer) = observer else { return };
        observer.attempt(u32::from(lease.attempt()));
        if let Some(ordinal) = self.key_pool.ordinal_of(&lease.key_id).await {
            observer.key_in_use(ordinal);
        }
    }

    async fn send(
        &self,
        request: TransportRequest,
        deadline: Instant,
        cancellation: &CancellationToken,
        request_id: &str,
    ) -> Result<TransportResponse, AppError> {
        if cancellation.is_cancelled() {
            let _ = self.key_pool.cancel(request_id.to_owned()).await;
            return Err(cancelled_error());
        }

        let transport = self.transport.send(request);
        tokio::pin!(transport);
        tokio::select! {
            result = tokio::time::timeout_at(deadline, &mut transport) => match result {
                Ok(Ok(response)) => Ok(response),
                Ok(Err(error)) => Err(transport_error(error)),
                Err(_) => Err(AppError::new(Code::Timeout, "Gemini request timed out")),
            },
            _ = cancellation.cancelled() => {
                let _ = self.key_pool.cancel(request_id.to_owned()).await;
                Err(cancelled_error())
            }
        }
    }
}

fn validate_job_path(path: &str) -> Result<(), AppError> {
    if path.starts_with("/v1beta/")
        && !path.contains('?')
        && !path.contains('#')
        && !path.contains('\\')
        && !path.split('/').any(|component| component == "..")
    {
        Ok(())
    } else {
        Err(AppError::new(
            Code::Request,
            "Gemini Job request path is invalid",
        ))
    }
}

fn priority_for(kind: ModelKind) -> Priority {
    match kind {
        ModelKind::Live => Priority::Live,
        ModelKind::Transcribe => Priority::Job,
        ModelKind::Memo => Priority::Memo,
    }
}

fn remaining(deadline: Instant) -> Result<Duration, AppError> {
    let now = Instant::now();
    deadline
        .checked_duration_since(now)
        .ok_or_else(|| AppError::new(Code::Timeout, "Gemini operation timed out"))
}

fn cancelled_error() -> AppError {
    AppError::new(Code::Blocked, "Gemini operation was cancelled")
}

fn outcome_for_error(error: &AppError) -> RequestOutcome {
    match error.code {
        Code::Timeout => RequestOutcome::Timeout,
        // Transport-level network failures (connection/DNS -- never an HTTP
        // status) are treated the same as a 5xx response: retried inside the
        // existing attempt budget, never resent on another key after a
        // timeout.
        Code::Network => RequestOutcome::Server,
        _ => RequestOutcome::Request,
    }
}

fn transport_error(error: TransportError) -> AppError {
    match error {
        TransportError::Tls => AppError::new(Code::Tls, "Gemini TLS or CA verification failed"),
        TransportError::Timeout => AppError::new(Code::Timeout, "Gemini request timed out"),
        TransportError::Network => AppError::new(Code::Network, "Gemini network request failed"),
    }
}

fn classify_http_status(status: u16) -> Option<(RequestOutcome, AppError)> {
    let (outcome, code, detail) = match status {
        401 | 403 => (
            RequestOutcome::Auth,
            Code::Auth,
            "Gemini authentication failed",
        ),
        404 => (
            RequestOutcome::Request,
            Code::Model,
            "Gemini model endpoint was not found",
        ),
        429 => (
            RequestOutcome::Quota,
            Code::Quota,
            "Gemini quota was exhausted",
        ),
        451 => (
            RequestOutcome::Request,
            Code::Blocked,
            "Gemini request was blocked",
        ),
        400..=499 => (
            RequestOutcome::Request,
            Code::Request,
            "Gemini request was rejected",
        ),
        500..=599 => (
            RequestOutcome::Server,
            Code::Network,
            "Gemini service is unavailable",
        ),
        _ => return None,
    };
    Some((outcome, AppError::new(code, detail)))
}

#[derive(Debug, Deserialize)]
struct ModelsPageDto {
    models: Option<Vec<ModelDto>>,
    #[serde(rename = "nextPageToken")]
    next_page_token: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ModelDto {
    name: Option<String>,
    #[serde(rename = "displayName")]
    display_name: Option<String>,
    #[serde(rename = "supportedGenerationMethods", default)]
    supported_generation_methods: Vec<String>,
}

#[derive(Debug)]
struct ModelsPage {
    models: Vec<ModelInfo>,
    next_page_token: Option<String>,
}

fn parse_models_page(body: &str) -> Result<ModelsPage, AppError> {
    let dto: ModelsPageDto = serde_json::from_str(body)
        .map_err(|_| AppError::new(Code::Shape, "Gemini models response has invalid shape"))?;
    let models = dto
        .models
        .ok_or_else(|| AppError::new(Code::Shape, "Gemini models response omitted models"))?
        .into_iter()
        .map(|model| {
            let name = model
                .name
                .filter(|name| !name.is_empty())
                .ok_or_else(|| AppError::new(Code::Shape, "Gemini model omitted its name"))?;
            Ok(ModelInfo {
                display_name: model.display_name.unwrap_or_else(|| name.clone()),
                name,
                supported_generation_methods: model.supported_generation_methods,
            })
        })
        .collect::<Result<Vec<_>, AppError>>()?;
    Ok(ModelsPage {
        models,
        next_page_token: dto.next_page_token,
    })
}

fn supports_kind(model: &ModelInfo, kind: ModelKind) -> bool {
    let allowed = match kind {
        ModelKind::Transcribe | ModelKind::Memo => &["generateContent"][..],
        ModelKind::Live => &["bidiGenerateContent"][..],
    };
    model
        .supported_generation_methods
        .iter()
        .any(|method| allowed.iter().any(|allowed| method == allowed))
}

/// Test-only helpers for building a real [`GeminiGateway`] backed by a fake
/// transport and an in-memory key pool — no network, no real clock. `pub(crate)`
/// (not private to `mod tests` below) so other modules' tests can drive the
/// real Gemini stack end to end (story 2.5 Tasks: "test tích hợp
/// `GatewayTranscriber` qua transport giả cho 4 kịch bản AR-36", exercised
/// from `transcribe::registry::tests`). `mod tests` below uses these same
/// helpers instead of keeping its own duplicate copies.
#[cfg(test)]
pub(crate) mod test_support {
    use super::*;
    use crate::gemini::keys::{Clock, KeyPoolHandle, KeyProvider};
    use crate::secrets::KeyMaterial;
    use std::sync::Mutex;
    use tokio::sync::Notify;

    pub(crate) struct FakeProvider {
        keys: Mutex<Vec<KeyMaterial>>,
    }

    impl KeyProvider for FakeProvider {
        fn load_keys(&self) -> Result<Vec<KeyMaterial>, AppError> {
            Ok(self.keys.lock().unwrap().clone())
        }
    }

    pub(crate) struct FakeClock {
        now: Mutex<Instant>,
        changed: Notify,
    }

    impl FakeClock {
        pub(crate) fn new() -> Arc<Self> {
            Arc::new(Self {
                now: Mutex::new(Instant::now()),
                changed: Notify::new(),
            })
        }
    }

    impl FakeClock {
        /// Advances the fake clock and wakes anything blocked in
        /// [`Clock::sleep_until`] (a cooldown/backoff wait, or a deadline).
        /// Exposed so tests can drive a `Server`/`Quota` cooldown to
        /// completion deterministically (see `gateway_with_clock`).
        pub(crate) fn advance(&self, duration: Duration) {
            {
                let mut now = self.now.lock().unwrap();
                *now += duration;
            }
            self.changed.notify_waiters();
        }
    }

    impl Clock for FakeClock {
        fn now(&self) -> Instant {
            *self.now.lock().unwrap()
        }

        fn sleep_until(&self, deadline: Instant) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
            Box::pin(async move {
                while self.now() < deadline {
                    self.changed.notified().await;
                }
            })
        }
    }

    pub(crate) fn key(id: &str, secret: &str) -> KeyMaterial {
        KeyMaterial::new(KeyId::from_opaque(id), secret.to_string())
    }

    /// Same gateway as [`gateway_with`], plus the [`FakeClock`] handle so a
    /// test can control exactly when a `Server`/`Quota` cooldown elapses
    /// (story epic-2-p1 "backoff gates the 2nd dispatch").
    pub(crate) async fn gateway_with_clock(
        keys: Vec<KeyMaterial>,
        transport: Arc<dyn GeminiTransport>,
    ) -> (GeminiGateway, Arc<FakeClock>) {
        let provider = Arc::new(FakeProvider {
            keys: Mutex::new(keys),
        });
        let clock = FakeClock::new();
        let (pool, actor) = KeyPoolHandle::channel(provider, clock.clone());
        tokio::spawn(actor.run());
        pool.refresh().await.unwrap();
        (GeminiGateway::new(transport, pool), clock)
    }

    /// Builds a gateway over a fake transport and an in-memory key pool whose
    /// clock free-runs (auto-advanced in the background) so a `Server`/
    /// `Quota` cooldown set by a fault-injected response always elapses on
    /// its own -- most tests only care that a retry eventually lands, not the
    /// exact backoff timing (for that, use `gateway_with_clock`).
    pub(crate) async fn gateway_with(
        keys: Vec<KeyMaterial>,
        transport: Arc<dyn GeminiTransport>,
    ) -> GeminiGateway {
        let (gateway, clock) = gateway_with_clock(keys, transport).await;
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_millis(5)).await;
                clock.advance(Duration::from_millis(250));
            }
        });
        gateway
    }

    #[derive(Clone)]
    pub(crate) struct FakeTransport {
        responses: Arc<Mutex<Vec<Result<TransportResponse, TransportError>>>>,
        requests: Arc<Mutex<Vec<TransportRequest>>>,
    }

    impl FakeTransport {
        pub(crate) fn new(responses: Vec<Result<TransportResponse, TransportError>>) -> Arc<Self> {
            Arc::new(Self {
                responses: Arc::new(Mutex::new(responses)),
                requests: Arc::new(Mutex::new(Vec::new())),
            })
        }

        pub(crate) fn requests(&self) -> Vec<TransportRequest> {
            self.requests.lock().unwrap().clone()
        }
    }

    impl GeminiTransport for FakeTransport {
        fn send(&self, request: TransportRequest) -> TransportFuture {
            self.requests.lock().unwrap().push(request);
            let response = self.responses.lock().unwrap().remove(0);
            Box::pin(async move { response })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::{gateway_with, gateway_with_clock, key, FakeTransport};
    use super::*;
    use std::sync::Mutex;

    #[derive(Default)]
    struct PendingTransport {
        requests: Mutex<Vec<TransportRequest>>,
    }

    impl GeminiTransport for PendingTransport {
        fn send(&self, request: TransportRequest) -> TransportFuture {
            self.requests.lock().unwrap().push(request);
            Box::pin(std::future::pending())
        }
    }

    fn page(models: &str, token: Option<&str>) -> TransportResponse {
        let token = token
            .map(|token| format!(",\"nextPageToken\":\"{token}\""))
            .unwrap_or_default();
        TransportResponse {
            status: 200,
            body: format!("{{\"models\":[{models}]{token}}}"),
        }
    }

    fn model(name: &str, methods: &str) -> String {
        format!(
            "{{\"name\":\"{name}\",\"displayName\":\"{name}\",\"supportedGenerationMethods\":[{methods}]}}"
        )
    }

    #[tokio::test]
    async fn consent_blocks_before_pool_or_transport() {
        let transport = FakeTransport::new(vec![]);
        let gateway = gateway_with(vec![key("a", "AIzaA123456789")], transport.clone()).await;
        let error = gateway
            .models_list(
                ModelKind::Transcribe,
                ConsentSnapshot::new(0, false),
                CancellationToken::new(),
            )
            .await
            .unwrap_err();
        assert_eq!(error.code, Code::Blocked);
        assert!(transport.requests().is_empty());
    }

    #[tokio::test]
    async fn list_reads_pages_dedupes_and_filters_only_metadata_capabilities() {
        let transport = FakeTransport::new(vec![
            Ok(page(
                &format!(
                    "{},{}",
                    model("models/a", "\"generateContent\""),
                    model("models/live", "\"bidiGenerateContent\"")
                ),
                Some("page-2"),
            )),
            Ok(page(
                &format!(
                    "{},{}",
                    model("models/a", "\"generateContent\""),
                    model("models/b", "\"countTokens\"")
                ),
                None,
            )),
        ]);
        let gateway = gateway_with(vec![key("a", "AIzaA123456789")], transport.clone()).await;
        let models = gateway
            .models_list(
                ModelKind::Transcribe,
                ConsentSnapshot::new(1, false),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert_eq!(
            models
                .iter()
                .map(|model| model.name.as_str())
                .collect::<Vec<_>>(),
            ["models/a"]
        );
        let requests = transport.requests();
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0].method, HttpMethod::Get);
        assert_eq!(requests[0].path, MODELS_PATH);
        assert_eq!(requests[0].header("x-goog-api-key"), Some("AIzaA123456789"));
        assert!(requests[0].query.iter().all(|(name, _)| name != "key"));
        assert_eq!(
            requests[1]
                .query
                .iter()
                .find(|(name, _)| name == "pageToken")
                .map(|(_, value)| value.as_str()),
            Some("page-2")
        );
    }

    #[tokio::test]
    async fn live_filter_uses_capability_metadata_not_model_name() {
        let transport = FakeTransport::new(vec![Ok(page(
            &format!(
                "{},{}",
                model("models/name-says-live-but-is-rest", "\"generateContent\""),
                model("models/opaque-alias", "\"bidiGenerateContent\"")
            ),
            None,
        ))]);
        let gateway = gateway_with(vec![key("a", "AIzaA123456789")], transport).await;
        let models = gateway
            .models_list(
                ModelKind::Live,
                ConsentSnapshot::new(1, false),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert_eq!(
            models
                .iter()
                .map(|model| model.name.as_str())
                .collect::<Vec<_>>(),
            ["models/opaque-alias"]
        );
    }

    #[tokio::test]
    async fn repeated_page_token_is_shape_error() {
        let transport = FakeTransport::new(vec![
            Ok(page(
                &model("models/a", "\"generateContent\""),
                Some("same"),
            )),
            Ok(page(
                &model("models/b", "\"generateContent\""),
                Some("same"),
            )),
        ]);
        let gateway = gateway_with(vec![key("a", "AIzaA123456789")], transport).await;
        let error = gateway
            .models_list(
                ModelKind::Transcribe,
                ConsentSnapshot::new(1, false),
                CancellationToken::new(),
            )
            .await
            .unwrap_err();
        assert_eq!(error.code, Code::Shape);
    }

    #[tokio::test]
    async fn quota_rotates_to_the_next_key() {
        let transport = FakeTransport::new(vec![
            Ok(TransportResponse {
                status: 429,
                body: "quota body containing AIzaSECRET".to_string(),
            }),
            Ok(page(&model("models/a", "\"generateContent\""), None)),
        ]);
        let gateway = gateway_with(
            vec![key("a", "AIzaA123456789"), key("b", "AQ.B123456789")],
            transport.clone(),
        )
        .await;
        let models = gateway
            .models_list(
                ModelKind::Transcribe,
                ConsentSnapshot::new(1, false),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert_eq!(models.len(), 1);
        let requests = transport.requests();
        assert_eq!(requests[0].header("x-goog-api-key"), Some("AIzaA123456789"));
        assert_eq!(requests[1].header("x-goog-api-key"), Some("AQ.B123456789"));
    }

    #[tokio::test]
    async fn post_job_retries_5xx_serially_and_locks_request_contract() {
        let body = r#"{"contents":[{"parts":[{"inline_data":{"mime_type":"audio/flac","data":"Zm9v"}}]}]}"#;
        let transport = FakeTransport::new(vec![
            Ok(TransportResponse {
                status: 503,
                body: "temporary failure".to_string(),
            }),
            Ok(TransportResponse {
                status: 200,
                body: "{\"candidates\":[]}".to_string(),
            }),
        ]);
        let gateway = gateway_with(vec![key("a", "AIzaA123456789")], transport.clone()).await;
        let response = gateway
            .post_job(
                "/v1beta/models/models/opaque:generateContent",
                body.to_string(),
                ConsentSnapshot::new(1, false),
                CancellationToken::new(),
            )
            .await
            .unwrap();

        assert_eq!(response.status, 200);
        let requests = transport.requests();
        assert_eq!(requests.len(), 2);
        for request in requests {
            assert_eq!(request.method, HttpMethod::Post);
            assert_eq!(request.path, "/v1beta/models/models/opaque:generateContent");
            assert!(request.query.is_empty());
            assert_eq!(request.header("x-goog-api-key"), Some("AIzaA123456789"));
            assert_eq!(request.header("content-type"), Some("application/json"));
            assert_eq!(request.body.as_ref().unwrap().expose(), body);
        }
    }

    // Epic 2 P1 backend fixes: a transport-level `Code::Network` failure
    // (connection/DNS, never an HTTP status) retries inside the same
    // attempt budget as a 5xx, and the retry stays gated behind the
    // `Server` cooldown/backoff (`SERVER_BACKOFF`) just like `classify_http_status`'s
    // `RequestOutcome::Server` branch.
    #[tokio::test]
    async fn post_job_retries_a_transport_network_blip_then_succeeds() {
        let transport = FakeTransport::new(vec![
            Err(TransportError::Network),
            Ok(TransportResponse {
                status: 200,
                body: "{\"candidates\":[]}".to_string(),
            }),
        ]);
        let gateway = gateway_with(vec![key("a", "AIzaA123456789")], transport.clone()).await;
        let response = gateway
            .post_job(
                "/v1beta/interactions",
                "{}".to_string(),
                ConsentSnapshot::new(1, false),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert_eq!(response.status, 200);
        assert_eq!(transport.requests().len(), 2);
    }

    #[tokio::test]
    async fn post_job_server_backoff_gates_the_second_dispatch() {
        let transport = FakeTransport::new(vec![
            Ok(TransportResponse {
                status: 503,
                body: "temporary failure".to_string(),
            }),
            Ok(TransportResponse {
                status: 200,
                body: "{\"candidates\":[]}".to_string(),
            }),
        ]);
        let (gateway, clock) =
            gateway_with_clock(vec![key("a", "AIzaA123456789")], transport.clone()).await;
        let operation = tokio::spawn(async move {
            gateway
                .post_job(
                    "/v1beta/interactions",
                    "{}".to_string(),
                    ConsentSnapshot::new(1, false),
                    CancellationToken::new(),
                )
                .await
        });
        while transport.requests().is_empty() {
            tokio::task::yield_now().await;
        }
        tokio::task::yield_now().await;
        assert_eq!(
            transport.requests().len(),
            1,
            "the retry must not dispatch before the backoff elapses"
        );
        assert!(
            !operation.is_finished(),
            "the operation must still be waiting on the backoff"
        );
        clock.advance(params::SERVER_BACKOFF_MAX);
        let response = tokio::time::timeout(Duration::from_secs(1), operation)
            .await
            .expect("the backoff must eventually release the retry")
            .unwrap()
            .unwrap();
        assert_eq!(response.status, 200);
        assert_eq!(transport.requests().len(), 2);
    }

    // Story 2.5 Tasks: "thêm test chuỗi response cho: 429 xoay key, 3×5xx rồi
    // thành công, 4×5xx cạn ngân sách, 400/404/451 không gửi lại, timeout
    // không fan-out, 401 key 1 rồi key 2 thành công -- khoá AC retry ở đúng
    // tầng sở hữu bộ đếm (`gemini/`)." 400/404/451 is covered by
    // `post_job_rejects_nonretryable_http_errors_and_oversized_body` below
    // and timeout fan-out by `post_job_deadline_is_terminal_and_request_body_is_redacted`
    // above; the four tests here cover the remaining rows of the AR-36
    // matrix (spec I/O Matrix "429 xoay key", "5xx", "401 giữa chừng").

    #[tokio::test]
    async fn post_job_quota_rotates_to_the_next_key_then_succeeds() {
        let transport = FakeTransport::new(vec![
            Ok(TransportResponse {
                status: 429,
                body: "quota body containing AIzaSECRET".to_string(),
            }),
            Ok(TransportResponse {
                status: 200,
                body: "{\"candidates\":[]}".to_string(),
            }),
        ]);
        let gateway = gateway_with(
            vec![key("a", "AIzaA123456789"), key("b", "AQ.B123456789")],
            transport.clone(),
        )
        .await;
        let response = gateway
            .post_job(
                "/v1beta/interactions",
                "{}".to_string(),
                ConsentSnapshot::new(1, false),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert_eq!(response.status, 200);
        let requests = transport.requests();
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0].header("x-goog-api-key"), Some("AIzaA123456789"));
        assert_eq!(requests[1].header("x-goog-api-key"), Some("AQ.B123456789"));
    }

    #[tokio::test]
    async fn post_job_retries_three_server_errors_then_succeeds_on_the_fourth_attempt() {
        let transport = FakeTransport::new(vec![
            Ok(TransportResponse {
                status: 503,
                body: "e1".to_string(),
            }),
            Ok(TransportResponse {
                status: 503,
                body: "e2".to_string(),
            }),
            Ok(TransportResponse {
                status: 503,
                body: "e3".to_string(),
            }),
            Ok(TransportResponse {
                status: 200,
                body: "{\"candidates\":[]}".to_string(),
            }),
        ]);
        let gateway = gateway_with(vec![key("a", "AIzaA123456789")], transport.clone()).await;
        let observer = Arc::new(RecordingObserver::default());
        let response = gateway
            .post_job_observed(
                "/v1beta/interactions",
                "{}".to_string(),
                ConsentSnapshot::new(1, false),
                CancellationToken::new(),
                Some(observer.clone() as Arc<dyn JobObserver>),
            )
            .await
            .unwrap();
        assert_eq!(response.status, 200);
        assert_eq!(
            transport.requests().len(),
            usize::from(params::MAX_ATTEMPTS)
        );
        assert_eq!(*observer.attempts.lock().unwrap(), vec![1, 2, 3, 4]);
    }

    #[tokio::test]
    async fn post_job_exhausts_the_attempt_budget_after_four_server_errors() {
        let transport = FakeTransport::new(vec![
            Ok(TransportResponse {
                status: 503,
                body: "e1".to_string(),
            }),
            Ok(TransportResponse {
                status: 503,
                body: "e2".to_string(),
            }),
            Ok(TransportResponse {
                status: 503,
                body: "e3".to_string(),
            }),
            Ok(TransportResponse {
                status: 503,
                body: "e4".to_string(),
            }),
        ]);
        let gateway = gateway_with(vec![key("a", "AIzaA123456789")], transport.clone()).await;
        let error = gateway
            .post_job(
                "/v1beta/interactions",
                "{}".to_string(),
                ConsentSnapshot::new(1, false),
                CancellationToken::new(),
            )
            .await
            .unwrap_err();
        assert_eq!(error.code, Code::Network);
        assert_eq!(
            transport.requests().len(),
            usize::from(params::MAX_ATTEMPTS)
        );
    }

    #[tokio::test]
    async fn post_job_auth_rejection_rotates_to_the_next_key_then_succeeds() {
        let transport = FakeTransport::new(vec![
            Ok(TransportResponse {
                status: 401,
                body: "rejected".to_string(),
            }),
            Ok(TransportResponse {
                status: 200,
                body: "{\"candidates\":[]}".to_string(),
            }),
        ]);
        let gateway = gateway_with(
            vec![key("a", "AIzaA123456789"), key("b", "AQ.B123456789")],
            transport.clone(),
        )
        .await;
        let response = gateway
            .post_job(
                "/v1beta/interactions",
                "{}".to_string(),
                ConsentSnapshot::new(1, false),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert_eq!(response.status, 200);
        let requests = transport.requests();
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0].header("x-goog-api-key"), Some("AIzaA123456789"));
        assert_eq!(requests[1].header("x-goog-api-key"), Some("AQ.B123456789"));
    }

    #[tokio::test]
    async fn post_job_rejects_nonretryable_http_errors_and_oversized_body() {
        for (status, code) in [
            (400, Code::Request),
            (404, Code::Model),
            (451, Code::Blocked),
        ] {
            let transport = FakeTransport::new(vec![Ok(TransportResponse {
                status,
                body: "rejected".to_string(),
            })]);
            let gateway = gateway_with(vec![key("a", "AIzaA123456789")], transport.clone()).await;
            let error = gateway
                .post_job(
                    "/v1beta/interactions",
                    "{}".to_string(),
                    ConsentSnapshot::new(1, false),
                    CancellationToken::new(),
                )
                .await
                .unwrap_err();
            assert_eq!(error.code, code);
            assert_eq!(transport.requests().len(), 1);
        }

        let transport = FakeTransport::new(vec![]);
        let gateway = gateway_with(vec![key("a", "AIzaA123456789")], transport.clone()).await;
        let error = gateway
            .post_job(
                "/v1beta/interactions",
                "x".repeat(MAX_TRANSCRIBE_REQUEST_BYTES),
                ConsentSnapshot::new(1, false),
                CancellationToken::new(),
            )
            .await
            .unwrap_err();
        assert_eq!(error.code, Code::Format);
        assert!(transport.requests().is_empty());
    }

    #[tokio::test]
    async fn post_job_deadline_is_terminal_and_request_body_is_redacted() {
        let transport = FakeTransport::new(vec![Err(TransportError::Timeout)]);
        let gateway = gateway_with(vec![key("a", "AIzaA123456789")], transport.clone()).await;
        let body = "private audio and transcript bytes";
        let error = gateway
            .post_job(
                "/v1beta/interactions",
                body.to_string(),
                ConsentSnapshot::new(1, false),
                CancellationToken::new(),
            )
            .await
            .unwrap_err();
        assert_eq!(error.code, Code::Timeout);
        assert_eq!(transport.requests().len(), 1);
        assert!(!format!("{:?}", transport.requests()[0]).contains(body));
    }

    #[tokio::test]
    async fn cancellation_and_transport_failures_are_typed_and_redacted() {
        for (transport_error, code) in [
            (TransportError::Tls, Code::Tls),
            (TransportError::Timeout, Code::Timeout),
            (TransportError::Network, Code::Network),
        ] {
            let transport = FakeTransport::new(vec![Err(transport_error)]);
            let gateway = gateway_with(vec![key("a", "AIzaA123456789")], transport).await;
            let error = gateway
                .models_list(
                    ModelKind::Transcribe,
                    ConsentSnapshot::new(1, false),
                    CancellationToken::new(),
                )
                .await
                .unwrap_err();
            assert_eq!(error.code, code);
            assert!(!error.detail_redacted.contains("AIza"));
        }

        let transport = FakeTransport::new(vec![]);
        let gateway = gateway_with(vec![key("a", "AIzaA123456789")], transport).await;
        let cancellation = CancellationToken::new();
        cancellation.cancel();
        let error = gateway
            .models_list(
                ModelKind::Transcribe,
                ConsentSnapshot::new(1, false),
                cancellation,
            )
            .await
            .unwrap_err();
        assert_eq!(error.code, Code::Blocked);
    }

    #[tokio::test]
    async fn cancellation_interrupts_an_inflight_transport() {
        let transport = Arc::new(PendingTransport::default());
        let gateway = gateway_with(vec![key("a", "AIzaA123456789")], transport.clone()).await;
        let cancellation = CancellationToken::new();
        let operation_cancellation = cancellation.clone();
        let operation = tokio::spawn(async move {
            gateway
                .models_list(
                    ModelKind::Transcribe,
                    ConsentSnapshot::new(1, false),
                    operation_cancellation,
                )
                .await
        });
        while transport.requests.lock().unwrap().is_empty() {
            tokio::task::yield_now().await;
        }
        cancellation.cancel();
        let error = tokio::time::timeout(Duration::from_millis(100), operation)
            .await
            .expect("cancellation must interrupt transport")
            .expect("gateway task must not panic")
            .unwrap_err();
        assert_eq!(error.code, Code::Blocked);
        assert_eq!(transport.requests.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn key_test_isolated_to_target() {
        let transport = FakeTransport::new(vec![Ok(page(
            &model("models/a", "\"generateContent\""),
            None,
        ))]);
        let gateway = gateway_with(
            vec![key("a", "AIzaA123456789"), key("b", "AQ.B123456789")],
            transport.clone(),
        )
        .await;
        let result = gateway
            .keys_test(
                KeyId::from_opaque("b"),
                ConsentSnapshot::new(1, false),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert_eq!(result.key_id, KeyId::from_opaque("b"));
        assert_eq!(
            transport.requests()[0].header("x-goog-api-key"),
            Some("AQ.B123456789")
        );
    }

    #[tokio::test]
    async fn rejected_target_key_never_falls_back_and_only_success_restores_it() {
        let transport = FakeTransport::new(vec![
            Ok(TransportResponse {
                status: 401,
                body: "rejected".to_string(),
            }),
            Ok(page(&model("models/a", "\"generateContent\""), None)),
            Ok(page(&model("models/b", "\"generateContent\""), None)),
        ]);
        let gateway = gateway_with(
            vec![key("a", "AIzaA123456789"), key("b", "AQ.B123456789")],
            transport.clone(),
        )
        .await;

        let error = gateway
            .keys_test(
                KeyId::from_opaque("b"),
                ConsentSnapshot::new(1, false),
                CancellationToken::new(),
            )
            .await
            .unwrap_err();
        assert_eq!(error.code, Code::Auth);

        gateway
            .models_list(
                ModelKind::Transcribe,
                ConsentSnapshot::new(1, false),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        gateway
            .keys_test(
                KeyId::from_opaque("b"),
                ConsentSnapshot::new(1, false),
                CancellationToken::new(),
            )
            .await
            .unwrap();

        let requests = transport.requests();
        assert_eq!(requests.len(), 3);
        assert_eq!(requests[0].header("x-goog-api-key"), Some("AQ.B123456789"));
        assert_eq!(requests[1].header("x-goog-api-key"), Some("AIzaA123456789"));
        assert_eq!(requests[2].header("x-goog-api-key"), Some("AQ.B123456789"));
    }

    #[derive(Default)]
    struct RecordingObserver {
        waiting: Mutex<Vec<bool>>,
        ordinals: Mutex<Vec<u32>>,
        attempts: Mutex<Vec<u32>>,
    }

    impl JobObserver for RecordingObserver {
        fn waiting_quota(&self, waiting: bool) {
            self.waiting.lock().unwrap().push(waiting);
        }
        fn key_in_use(&self, ordinal: u32) {
            self.ordinals.lock().unwrap().push(ordinal);
        }
        fn attempt(&self, attempt: u32) {
            self.attempts.lock().unwrap().push(attempt);
        }
    }

    #[tokio::test]
    async fn post_job_observed_reports_ordinal_attempt_and_quota_wait_around_a_retry() {
        let transport = FakeTransport::new(vec![
            Ok(TransportResponse {
                status: 429,
                body: "quota".to_string(),
            }),
            Ok(TransportResponse {
                status: 200,
                body: "{\"candidates\":[]}".to_string(),
            }),
        ]);
        let gateway = gateway_with(
            vec![key("a", "AIzaA123456789"), key("b", "AQ.B123456789")],
            transport.clone(),
        )
        .await;
        let observer = Arc::new(RecordingObserver::default());
        gateway
            .post_job_observed(
                "/v1beta/interactions",
                "{}".to_string(),
                ConsentSnapshot::new(1, false),
                CancellationToken::new(),
                Some(observer.clone() as Arc<dyn JobObserver>),
            )
            .await
            .unwrap();

        assert_eq!(*observer.attempts.lock().unwrap(), vec![1, 2]);
        assert_eq!(*observer.ordinals.lock().unwrap(), vec![1, 2]);
        // waiting flips true/false around the initial acquire and again
        // around the quota-triggered retry.
        assert_eq!(
            *observer.waiting.lock().unwrap(),
            vec![true, false, true, false]
        );
    }

    #[test]
    fn http_statuses_and_default_models_are_stable() {
        for (status, outcome, code) in [
            (401, RequestOutcome::Auth, Code::Auth),
            (403, RequestOutcome::Auth, Code::Auth),
            (404, RequestOutcome::Request, Code::Model),
            (429, RequestOutcome::Quota, Code::Quota),
            (451, RequestOutcome::Request, Code::Blocked),
            (400, RequestOutcome::Request, Code::Request),
            (503, RequestOutcome::Server, Code::Network),
        ] {
            let (actual_outcome, error) = classify_http_status(status).unwrap();
            assert_eq!(actual_outcome, outcome, "status {status}");
            assert_eq!(error.code, code, "status {status}");
        }
        assert!(classify_http_status(200).is_none());
        assert_eq!(
            outcome_for_error(&AppError::new(Code::Network, "x")),
            RequestOutcome::Server
        );
        assert_eq!(
            outcome_for_error(&AppError::new(Code::Timeout, "x")),
            RequestOutcome::Timeout
        );
        assert_eq!(
            outcome_for_error(&AppError::new(Code::Tls, "x")),
            RequestOutcome::Request
        );
        assert_eq!(params::DEFAULT_TRANSCRIBE_MODEL, "gemini-flash-lite-latest");
        assert_eq!(params::DEFAULT_MEMO_MODEL, "gemini-flash-lite-latest");
        assert_eq!(
            params::DEFAULT_LIVE_MODEL,
            "gemini-3.5-live-translate-preview"
        );
    }

    #[test]
    fn transport_debug_never_reveals_key_or_response_body() {
        let request =
            TransportRequest::models(Sensitive::new("AIzaSECRET123456".to_string()), None);
        let response = TransportResponse {
            status: 200,
            body: "body containing AIzaSECRET".to_string(),
        };
        assert!(!format!("{request:?}").contains("AIzaSECRET"));
        assert!(!format!("{response:?}").contains("body containing"));
    }
}

#[cfg(test)]
mod consent_tests {
    use super::*;

    #[test]
    fn current_consent_opens_transport_once() {
        let mut calls = 0;
        let result = guarded_request(1, false, || {
            calls += 1;
            Ok::<_, AppError>("fake response")
        });
        assert_eq!(result.unwrap(), "fake response");
        assert_eq!(calls, 1);
    }

    #[test]
    fn pending_declined_and_stale_never_open_transport() {
        for (version, declined) in [(0, false), (1, true), (2, false)] {
            let mut calls = 0;
            let result = guarded_request(version, declined, || {
                calls += 1;
                Ok::<_, AppError>(())
            });
            assert!(result.is_err());
            assert_eq!(calls, 0);
        }
    }
}
