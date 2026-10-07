//! Shared HTTP transport for provider adapters.
//!
//! One `HttpClient` per provider credential. It paces requests, enforces a
//! request budget (counting retries), retries only what is safe to retry,
//! caps response sizes, and produces errors that never contain credentials or
//! full request URLs: messages name the provider and the endpoint only.

use std::sync::Arc;
use std::sync::Mutex as StdMutex;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::{Duration, Instant};

use reqwest::header::{HeaderMap, HeaderValue, RETRY_AFTER};
use reqwest::{Method, StatusCode};
use serde::de::DeserializeOwned;
use tokio::sync::Mutex;
use url::Url;

use crate::error::ProviderError;
use crate::network_log::NetworkLog;

pub(crate) type PacingGate = Arc<Mutex<Option<Instant>>>;

/// Transport policy for one provider.
#[derive(Debug, Clone)]
pub struct HttpConfig {
    /// Per-attempt timeout covering connect, headers, and body.
    pub timeout: Duration,
    /// Responses larger than this are rejected rather than buffered.
    pub max_body_bytes: usize,
    /// Minimum spacing between request starts (fair use / rate limits).
    pub min_interval: Duration,
    /// Retries after the first attempt for 429, 5xx, timeouts, and resets.
    pub max_retries: u32,
    /// Longest `Retry-After` the client will sleep for; longer waits fail fast.
    pub max_retry_after: Duration,
    /// Base delay of the exponential backoff without `Retry-After`.
    pub backoff: Duration,
}

impl Default for HttpConfig {
    fn default() -> Self {
        HttpConfig {
            timeout: Duration::from_secs(30),
            max_body_bytes: 16 * 1024 * 1024,
            min_interval: Duration::from_secs(1),
            max_retries: 2,
            max_retry_after: Duration::from_secs(30),
            backoff: Duration::from_millis(500),
        }
    }
}

/// Request budget shared by every client using the same credential.
#[derive(Debug, Default)]
pub struct Budget {
    limit: Option<u32>,
    used: AtomicU32,
    credit_limit: Option<u32>,
    credits: AtomicU32,
    stopped: StdMutex<Option<ProviderError>>,
    last_start: Mutex<Option<Instant>>,
}

impl Budget {
    pub fn unlimited() -> Arc<Self> {
        Arc::new(Budget::default())
    }

    pub fn limited(limit: u32) -> Arc<Self> {
        Arc::new(Budget {
            limit: Some(limit),
            used: AtomicU32::new(0),
            credit_limit: None,
            credits: AtomicU32::new(0),
            stopped: StdMutex::new(None),
            last_start: Mutex::new(None),
        })
    }

    pub fn limited_with_credits(requests: u32, credits: u32) -> Arc<Self> {
        Arc::new(Budget {
            limit: Some(requests),
            credit_limit: Some(credits),
            ..Budget::default()
        })
    }

    fn take_cost(&self, cost: u32) -> bool {
        self.credits
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |used| {
                let next = used.checked_add(cost)?;
                self.credit_limit
                    .is_none_or(|limit| next <= limit)
                    .then_some(next)
            })
            .is_ok()
    }

    pub fn credits(&self) -> u32 {
        self.credits.load(Ordering::Relaxed)
    }

    /// Reserves one request; `false` when the budget is spent.
    fn try_take(&self) -> bool {
        if self.stopped_error().is_some() {
            return false;
        }
        let Some(limit) = self.limit else {
            self.used.fetch_add(1, Ordering::Relaxed);
            return true;
        };
        self.used
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |u| {
                (u < limit).then_some(u + 1)
            })
            .is_ok()
    }

    fn stopped_error(&self) -> Option<ProviderError> {
        self.stopped.lock().expect("budget stop lock").clone()
    }

    fn stop(&self, error: &ProviderError) {
        self.stopped
            .lock()
            .expect("budget stop lock")
            .get_or_insert_with(|| error.clone());
    }

    pub fn used(&self) -> u32 {
        self.used.load(Ordering::Relaxed)
    }
}

/// Requests sent and the last failure since the counters were last taken.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Usage {
    pub requests: u32,
    pub credits: u32,
    pub last_error: Option<String>,
}

pub struct HttpClient {
    provider: &'static str,
    client: reqwest::Client,
    config: HttpConfig,
    budget: Arc<Budget>,
    usage: StdMutex<Usage>,
    cancelled: StdMutex<Arc<AtomicBool>>,
    forbidden: StdMutex<std::collections::BTreeMap<String, ProviderError>>,
    network_log: StdMutex<Arc<NetworkLog>>,
    rate_limit_failover: AtomicBool,
    pacing_class: StdMutex<Option<PacingGate>>,
}

/// A successful (2xx) response body.
#[derive(Debug)]
pub struct Body {
    pub status: StatusCode,
    pub bytes: Vec<u8>,
}

impl Body {
    pub fn json<T: DeserializeOwned>(
        &self,
        provider: &'static str,
        endpoint: &'static str,
    ) -> Result<T, ProviderError> {
        serde_json::from_slice(&self.bytes).map_err(|e| ProviderError::InvalidResponse {
            provider,
            endpoint,
            // Serde's Display can echo an invalid field value, including a
            // credential reflected by the server. Report only its location.
            detail: format!("unexpected JSON at line {} column {}", e.line(), e.column()),
        })
    }
}

impl HttpClient {
    /// `headers` carry authentication; their values are marked sensitive so
    /// they are redacted from any debug output.
    pub fn new(
        provider: &'static str,
        config: HttpConfig,
        budget: Arc<Budget>,
        mut headers: HeaderMap,
    ) -> Result<Self, ProviderError> {
        for value in headers.values_mut() {
            value.set_sensitive(true);
        }
        headers
            .entry(reqwest::header::ACCEPT)
            .or_insert(HeaderValue::from_static("application/json"));
        let client = reqwest::Client::builder()
            .default_headers(headers)
            .user_agent(concat!("PortfolioDesk/", env!("CARGO_PKG_VERSION")))
            .timeout(config.timeout)
            .connect_timeout(config.timeout.min(Duration::from_secs(15)))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|e| ProviderError::Network {
                provider,
                endpoint: "client",
                detail: e.without_url().to_string(),
            })?;
        Ok(HttpClient {
            provider,
            client,
            config,
            budget,
            usage: StdMutex::new(Usage::default()),
            cancelled: StdMutex::new(Arc::new(AtomicBool::new(false))),
            forbidden: StdMutex::new(std::collections::BTreeMap::new()),
            rate_limit_failover: AtomicBool::new(false),
            pacing_class: StdMutex::new(None),
            network_log: StdMutex::new(Arc::new(NetworkLog::default())),
        })
    }

    pub fn provider(&self) -> &'static str {
        self.provider
    }
    pub fn set_network_log(&self, log: Arc<NetworkLog>) {
        *self.network_log.lock().expect("network log attachment") = log;
    }
    pub fn rpc_retry_policy(&self) -> (u32, Duration) {
        (self.config.max_retries, self.config.backoff)
    }

    pub fn set_cancellation(&self, flag: Arc<AtomicBool>) {
        *self.cancelled.lock().expect("cancellation lock") = flag;
    }
    fn check_cancelled(&self) -> Result<(), ProviderError> {
        if self
            .cancelled
            .lock()
            .expect("cancellation lock")
            .load(Ordering::Relaxed)
        {
            Err(ProviderError::Cancelled {
                provider: self.provider,
            })
        } else {
            Ok(())
        }
    }

    pub fn budget(&self) -> &Arc<Budget> {
        &self.budget
    }

    /// Returns and resets the usage counters (for persisting to `provider_usage`).
    pub fn take_usage(&self) -> Usage {
        std::mem::take(&mut *self.usage.lock().expect("usage lock"))
    }

    fn count_request(&self) {
        self.usage.lock().expect("usage lock").requests += 1;
    }

    /// Records the final error of a request (a retried, then successful,
    /// request is not an error).
    fn note_failure(&self, error: &ProviderError) {
        if matches!(error, ProviderError::RateLimited { .. })
            && self.rate_limit_failover.load(Ordering::Relaxed)
        {
            // Quota remains visible in the neutral network log and persisted
            // pause; it is not a fatal source error in Settings.
            return;
        }
        self.usage.lock().expect("usage lock").last_error = Some(error.to_string());
    }

    /// Synchronization has alternative sources: return throttling immediately
    /// instead of spending requests or waiting before trying the next source.
    /// Standalone clients retain their configured bounded retry policy.
    pub fn prefer_rate_limit_failover(&self) {
        self.rate_limit_failover.store(true, Ordering::Relaxed);
    }

    pub fn record_rpc_failure(&self, error: &ProviderError, url: &Url) {
        if let ProviderError::NetworkForbidden { endpoint, .. } = error {
            self.forbidden
                .lock()
                .expect("network access lock")
                .insert(self.access_scope(url, endpoint), error.clone());
        }
        if error.stops_provider() {
            self.budget.stop(error);
        }
        self.note_failure(error);
    }

    fn access_scope(&self, url: &Url, endpoint: &'static str) -> String {
        let origin = url.origin().ascii_serialization();
        match self.provider {
            "publicnode" => format!("{origin}/{endpoint}"),
            "drpc" | "ankr" => format!(
                "{origin}/{}/{endpoint}",
                url.path_segments().and_then(|mut p| p.next()).unwrap_or("")
            ),
            _ => origin,
        }
    }
    pub async fn get(&self, endpoint: &'static str, url: Url) -> Result<Body, ProviderError> {
        self.send(endpoint, Method::GET, url, None, 0).await
    }

    pub async fn get_cost(
        &self,
        endpoint: &'static str,
        url: Url,
        cost: u32,
    ) -> Result<Body, ProviderError> {
        self.send(endpoint, Method::GET, url, None, cost).await
    }

    pub async fn post_json(
        &self,
        endpoint: &'static str,
        url: Url,
        body: &serde_json::Value,
    ) -> Result<Body, ProviderError> {
        self.post_json_cost(endpoint, url, body, 0).await
    }

    pub async fn post_json_cost(
        &self,
        endpoint: &'static str,
        url: Url,
        body: &serde_json::Value,
        cost: u32,
    ) -> Result<Body, ProviderError> {
        self.send(endpoint, Method::POST, url, Some(body), cost)
            .await
    }

    /// Distinct API rate limits still share the credential's request/credit budget.
    /// Every physical attempt, including transport and RPC retries, uses this gate.
    pub(crate) fn set_pacing_gate(&self, gate: PacingGate) {
        *self.pacing_class.lock().expect("pacing class") = Some(gate);
    }

    async fn pace(&self) {
        let gate = self.pacing_class.lock().expect("pacing class").clone();
        let mut last = match &gate {
            Some(gate) => gate.lock().await,
            None => self.budget.last_start.lock().await,
        };
        if let Some(prev) = *last {
            let elapsed = prev.elapsed();
            if elapsed < self.config.min_interval {
                tokio::time::sleep(self.config.min_interval - elapsed).await;
            }
        }
        *last = Some(Instant::now());
    }

    async fn send(
        &self,
        endpoint: &'static str,
        method: Method,
        url: Url,
        json: Option<&serde_json::Value>,
        cost: u32,
    ) -> Result<Body, ProviderError> {
        let provider = self.provider;
        let mut attempt = 0u32;
        loop {
            self.check_cancelled()?;
            let denied = self
                .forbidden
                .lock()
                .expect("network access lock")
                .get(&self.access_scope(&url, endpoint))
                .cloned();
            if let Some(error) = denied {
                self.note_failure(&error);
                return Err(error);
            }
            if let Some(error) = self.budget.stopped_error() {
                self.note_failure(&error);
                return Err(error);
            }
            self.pace().await;
            self.check_cancelled()?;
            if !self.budget.take_cost(cost) {
                let error = ProviderError::BudgetExhausted { provider };
                self.budget.stop(&error);
                self.note_failure(&error);
                return Err(error);
            }
            if !self.budget.try_take() {
                self.budget.credits.fetch_sub(cost, Ordering::Relaxed);
                let error = self
                    .budget
                    .stopped_error()
                    .unwrap_or(ProviderError::BudgetExhausted { provider });
                self.note_failure(&error);
                return Err(error);
            }
            self.usage.lock().expect("usage lock").credits += cost;
            let mut request = self.client.request(method.clone(), url.clone());
            if let Some(body) = json {
                request = request.json(body);
            }
            let log = self
                .network_log
                .lock()
                .expect("network log attachment")
                .clone();
            let request_id = log.begin(provider, method.as_str(), &url, endpoint, attempt + 1);
            let started = Instant::now();
            let mut http_status = None;
            let outcome = match request.send().await {
                Ok(response) => {
                    http_status = Some(response.status().as_u16());
                    self.read(endpoint, response).await
                }
                Err(e) => Err(transport_error(provider, endpoint, &e)),
            };
            if let Some(id) = request_id {
                let rpc_error = outcome.as_ref().ok().and_then(|body| {
                    if !matches!(
                        provider,
                        "helius" | "alchemy" | "drpc" | "publicnode" | "chainstack" | "ankr"
                    ) {
                        return None;
                    }
                    let value: serde_json::Value = serde_json::from_slice(&body.bytes).ok()?;
                    let error = value.get("error").filter(|e| !e.is_null())?;
                    let code = error.get("code").and_then(serde_json::Value::as_i64);
                    let message = error
                        .get("message")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("")
                        .to_ascii_lowercase();
                    Some((
                        code,
                        crate::rpc::is_rate_limit(provider, code.unwrap_or(0), &message),
                    ))
                });
                let rpc_code = rpc_error.and_then(|(code, _)| code);
                let status = match &outcome {
                    Ok(_) if rpc_error.is_some_and(|(_, limited)| limited) => "rate_limited",
                    Err((ProviderError::RateLimited { .. }, _))
                        if self.rate_limit_failover.load(Ordering::Relaxed) =>
                    {
                        "rate_limited"
                    }
                    Ok(_) if rpc_error.is_some() => "rpc_error",
                    Ok(_) => "success",
                    Err((ProviderError::Timeout { .. }, _)) => "timeout",
                    Err((ProviderError::Network { .. }, _)) => "connection_error",
                    Err((error, wait))
                        if error.is_retryable()
                            && attempt < self.config.max_retries
                            && wait.is_none_or(|w| w <= self.config.max_retry_after) =>
                    {
                        "retrying"
                    }
                    Err(_) => "failed",
                };
                log.finish(id, status, http_status, rpc_code, started);
            }
            self.count_request();
            let (error, retry_after) = match outcome {
                Ok(body) => return Ok(body),
                Err(failure) => failure,
            };
            if !error.is_retryable()
                || attempt >= self.config.max_retries
                || (matches!(error, ProviderError::RateLimited { .. })
                    && self.rate_limit_failover.load(Ordering::Relaxed))
            {
                if matches!(error, ProviderError::NetworkForbidden { .. }) {
                    self.forbidden
                        .lock()
                        .expect("network access lock")
                        .insert(self.access_scope(&url, endpoint), error.clone());
                }
                if error.stops_provider() {
                    self.budget.stop(&error);
                }
                self.note_failure(&error);
                return Err(error);
            }
            let wait = match retry_after {
                Some(wait) if wait > self.config.max_retry_after => {
                    if error.stops_provider() {
                        self.budget.stop(&error);
                    }
                    self.note_failure(&error);
                    return Err(error);
                }
                Some(wait) => wait,
                None => self.config.backoff * 2u32.saturating_pow(attempt),
            };
            tracing::debug!(provider, endpoint, ?wait, "retrying after {error}");
            tokio::time::sleep(wait).await;
            attempt += 1;
        }
    }

    /// Reads a response with the size cap. Errors carry an optional `Retry-After`.
    async fn read(
        &self,
        endpoint: &'static str,
        mut response: reqwest::Response,
    ) -> Result<Body, (ProviderError, Option<Duration>)> {
        let provider = self.provider;
        let status = response.status();
        let retry_after = parse_retry_after(response.headers());
        if status == StatusCode::TOO_MANY_REQUESTS {
            // The status/header is sufficient. Do not wait for or buffer an
            // untrusted error body before advancing to an independent source.
            return Err((
                ProviderError::RateLimited {
                    provider,
                    endpoint,
                    retry_after_secs: retry_after.map(|d| d.as_secs()),
                },
                retry_after,
            ));
        }
        if let Some(len) = response.content_length()
            && len > self.config.max_body_bytes as u64
        {
            return Err((ProviderError::TooLarge { provider, endpoint }, None));
        }
        let mut bytes = Vec::new();
        loop {
            match response.chunk().await {
                Ok(Some(chunk)) => {
                    if bytes.len() + chunk.len() > self.config.max_body_bytes {
                        return Err((ProviderError::TooLarge { provider, endpoint }, None));
                    }
                    bytes.extend_from_slice(&chunk);
                }
                Ok(None) => break,
                Err(e) => return Err(transport_error(provider, endpoint, &e)),
            }
        }
        if status.is_success() {
            return Ok(Body { status, bytes });
        }
        // Keyed reserves and RPC providers may echo even short credentials in
        // arbitrary HTTP error bodies. Keep the status and omit the body.
        let detail = if matches!(
            provider,
            "helius"
                | "alchemy"
                | "ankr"
                | "drpc"
                | "publicnode"
                | "chainstack"
                | "blockscout"
                | "etherscan"
                | "toncenter"
        ) {
            String::new()
        } else {
            provider_message(&bytes)
        };
        let error = match status {
            StatusCode::PAYMENT_REQUIRED => {
                ProviderError::CapabilityUnavailable { provider, endpoint }
            }
            StatusCode::BAD_REQUEST if provider == "drpc" => {
                ProviderError::CapabilityUnavailable { provider, endpoint }
            }
            StatusCode::FORBIDDEN
                if matches!(provider, "alchemy" | "publicnode" | "drpc" | "ankr") =>
            {
                ProviderError::NetworkForbidden { provider, endpoint }
            }
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => ProviderError::Auth {
                provider,
                endpoint,
                status: status.as_u16(),
            },
            StatusCode::NOT_FOUND => ProviderError::NotFound { provider, endpoint },
            s if s.is_server_error() => ProviderError::Server {
                provider,
                endpoint,
                status: s.as_u16(),
            },
            s => ProviderError::Http {
                provider,
                endpoint,
                status: s.as_u16(),
                detail,
            },
        };
        Err((error, retry_after))
    }
}

fn transport_error(
    provider: &'static str,
    endpoint: &'static str,
    e: &reqwest::Error,
) -> (ProviderError, Option<Duration>) {
    if e.is_timeout() {
        return (ProviderError::Timeout { provider, endpoint }, None);
    }
    // `without_url` strips the request URL, which may carry a query-string key.
    let detail = sanitize(&format!("{}", RedactedError(e)));
    (
        ProviderError::Network {
            provider,
            endpoint,
            detail,
        },
        None,
    )
}

struct RedactedError<'a>(&'a reqwest::Error);

impl std::fmt::Display for RedactedError<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut kind = if self.0.is_connect() {
            "connection failed"
        } else if self.0.is_body() || self.0.is_decode() {
            "response interrupted"
        } else if self.0.is_request() {
            "request failed"
        } else {
            "transport error"
        };
        if self.0.is_redirect() {
            kind = "unexpected redirect";
        }
        f.write_str(kind)
    }
}

/// Removes anything that looks like a URL or a long token from a message.
fn sanitize(text: &str) -> String {
    text.split_whitespace()
        .map(|w| {
            let url_like = w.contains("://") || w.contains("api-key") || w.contains("apikey");
            let token_like =
                w.len() > 32 && w.chars().all(|c| c.is_ascii_alphanumeric() || c == '-');
            if url_like || token_like {
                "<redacted>"
            } else {
                w
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Extracts a short human message from common provider error bodies.
fn provider_message(bytes: &[u8]) -> String {
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(bytes) else {
        return String::new();
    };
    let pick = |v: &serde_json::Value| -> Option<String> {
        for key in ["detail", "description", "message", "title", "error"] {
            if let Some(s) = v.get(key).and_then(|x| x.as_str()) {
                return Some(s.to_owned());
            }
        }
        None
    };
    let text = value
        .get("errors")
        .and_then(|e| e.get(0))
        .and_then(pick)
        .or_else(|| value.get("error").and_then(pick))
        .or_else(|| pick(&value))
        .unwrap_or_default();
    let mut text = sanitize(&text);
    text.truncate(200);
    text
}

fn parse_retry_after(headers: &HeaderMap) -> Option<Duration> {
    let value = headers.get(RETRY_AFTER)?.to_str().ok()?.trim();
    // Only the delta-seconds form; an HTTP date is treated as "unknown".
    value.parse::<u64>().ok().map(Duration::from_secs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_drops_urls_and_long_tokens() {
        let s = sanitize("error sending request for url (https://x.io/?api-key=abc) ok");
        assert!(!s.contains("https"), "{s}");
        assert!(!s.contains("abc"), "{s}");
        let s = sanitize("token 0123456789abcdef0123456789abcdef0123 rejected");
        assert_eq!(s, "token <redacted> rejected");
    }

    #[test]
    fn budget_counts_and_stops() {
        let b = Budget::limited(2);
        assert!(b.try_take());
        assert!(b.try_take());
        assert!(!b.try_take());
        assert_eq!(b.used(), 2);
    }

    #[test]
    fn provider_messages_are_extracted() {
        assert_eq!(
            provider_message(br#"{"errors":[{"title":"Too many requests","detail":"throttled"}]}"#),
            "throttled"
        );
        assert_eq!(
            provider_message(br#"{"error":{"code":401,"description":"Invalid API key"}}"#),
            "Invalid API key"
        );
        assert_eq!(provider_message(b"<html>"), "");
    }
}
