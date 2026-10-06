//! Opt-in in-memory HTTP diagnostics. Never store URLs, headers, bodies or errors.
use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use url::Url;

const CAPACITY: usize = 500;

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct NetworkRequest {
    pub id: u32,
    pub started_at_ms: i64,
    pub provider: String,
    pub method: String,
    /// Only the URL origin; no credential path, query, fragment or username.
    pub origin: String,
    /// Compile-time adapter operation name, never a request body or URL path.
    pub operation: String,
    pub attempt: u32,
    pub status: String,
    pub http_status: Option<u16>,
    pub rpc_code: Option<i64>,
    pub duration_ms: Option<u32>,
}

#[derive(Default)]
struct LogState {
    enabled: bool,
    next_id: u32,
    entries: VecDeque<NetworkRequest>,
}

#[derive(Default)]
pub struct NetworkLog(Mutex<LogState>);

impl NetworkLog {
    pub fn set_enabled(&self, enabled: bool) {
        let mut state = self.0.lock().expect("network log");
        state.enabled = enabled;
        if !enabled {
            state.entries.clear();
        }
    }

    pub fn clear(&self) {
        self.0.lock().expect("network log").entries.clear();
    }

    pub fn entries(&self) -> Vec<NetworkRequest> {
        self.0
            .lock()
            .expect("network log")
            .entries
            .iter()
            .rev()
            .cloned()
            .collect()
    }

    pub(crate) fn begin(
        &self,
        provider: &'static str,
        method: &str,
        url: &Url,
        operation: &'static str,
        attempt: u32,
    ) -> Option<u32> {
        let mut state = self.0.lock().expect("network log");
        if !state.enabled {
            return None;
        }
        state.next_id = state.next_id.wrapping_add(1);
        let id = state.next_id;
        if state.entries.len() == CAPACITY {
            state.entries.pop_front();
        }
        state.entries.push_back(NetworkRequest {
            id,
            started_at_ms: i64::try_from(
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis(),
            )
            .unwrap_or(i64::MAX),
            provider: provider.into(),
            method: method.into(),
            origin: url.origin().ascii_serialization(),
            operation: operation.into(),
            attempt,
            status: "pending".into(),
            http_status: None,
            rpc_code: None,
            duration_ms: None,
        });
        Some(id)
    }

    pub(crate) fn finish(
        &self,
        id: u32,
        status: &str,
        http_status: Option<u16>,
        rpc_code: Option<i64>,
        started: Instant,
    ) {
        let mut state = self.0.lock().expect("network log");
        if let Some(entry) = state.entries.iter_mut().find(|entry| entry.id == id) {
            entry.status = status.into();
            entry.http_status = http_status;
            entry.rpc_code = rpc_code;
            entry.duration_ms =
                Some(u32::try_from(started.elapsed().as_millis()).unwrap_or(u32::MAX));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_is_opt_in_bounded_and_cannot_retain_credential_urls() {
        let log = NetworkLog::default();
        let url =
            Url::parse("https://name:tiny@eth-mainnet.g.alchemy.com/v2/tiny?api-key=tiny#tiny")
                .unwrap();
        assert!(
            log.begin("alchemy", "POST", &url, "eth_chainId", 1)
                .is_none()
        );
        log.set_enabled(true);
        for _ in 0..CAPACITY + 2 {
            log.begin("alchemy", "POST", &url, "eth_chainId", 1);
        }
        let entries = log.entries();
        assert_eq!(entries.len(), CAPACITY);
        assert_eq!(entries.last().unwrap().id, 3);
        assert!(!serde_json::to_string(&entries).unwrap().contains("tiny"));
        assert_eq!(entries[0].origin, "https://eth-mainnet.g.alchemy.com");
        log.set_enabled(false);
        log.set_enabled(true);
        log.finish(entries[0].id, "success", Some(200), None, Instant::now());
        assert!(
            log.entries().is_empty(),
            "an in-flight completion cannot resurrect a cleared log"
        );
    }
}
