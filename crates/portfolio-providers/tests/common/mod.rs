#![allow(dead_code)]

use std::sync::Arc;
use std::time::Duration;

use portfolio_core::clock::FixedClock;
use portfolio_providers::http::HttpConfig;
use portfolio_store::{ProfileKind, Store};

pub const NOW: i64 = 1_791_100_000;
/// Dummy sentinel credential: must never appear in any error, log, or report.
pub const SENTINEL_KEY: &str = "SENTINEL-not-a-real-key-7f3a9c";

pub fn fast() -> HttpConfig {
    HttpConfig {
        timeout: Duration::from_secs(5),
        max_body_bytes: 1024 * 1024,
        min_interval: Duration::ZERO,
        max_retries: 2,
        max_retry_after: Duration::from_secs(2),
        backoff: Duration::from_millis(10),
    }
}

pub async fn store() -> Store {
    Store::open_in_memory(ProfileKind::Test, Arc::new(FixedClock(NOW)))
        .await
        .unwrap()
}

pub fn fixture(path: &str) -> String {
    std::fs::read_to_string(format!(
        "{}/tests/fixtures/{path}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}
