//! Small, process-local cache of expensive read DTOs. A dedicated connection's
//! SQLite data_version invalidates it on every commit, including external writers.
//! In-memory test stores skip caching because their single connection cannot
//! observe its own commits through data_version.
use crate::{Result, Store, StoreError};
use serde::{Serialize, de::DeserializeOwned};
use std::collections::BTreeMap;
use std::future::Future;
use std::time::{Duration, Instant};

#[derive(Default)]
pub(crate) struct ViewCache {
    version: i64,
    entries: BTreeMap<String, (Instant, serde_json::Value)>,
}

impl Store {
    pub(crate) async fn cached_view<T: Serialize + DeserializeOwned>(
        &self,
        key: String,
        compute: impl Future<Output = Result<T>>,
    ) -> Result<T> {
        let Some(watcher) = &self.read_watcher else {
            return compute.await;
        };
        let version: i64 = sqlx::query_scalar("PRAGMA data_version")
            .fetch_one(watcher)
            .await?;
        // Chart end-time advances with the clock. Other DTOs are entirely
        // data-derived; retain them longer while SQLite's version is unchanged.
        let ttl = if key.starts_with("chart:") {
            Duration::from_secs(1)
        } else {
            Duration::from_secs(30)
        };
        {
            let cache = self.view_cache.lock().await;
            if cache.version == version
                && let Some((at, value)) = cache.entries.get(&key)
                && at.elapsed() < ttl
            {
                return serde_json::from_value(value.clone())
                    .map_err(|e| StoreError::Corrupt(format!("cached view: {e}")));
            }
        }
        let result = compute.await?;
        let after: i64 = sqlx::query_scalar("PRAGMA data_version")
            .fetch_one(watcher)
            .await?;
        if version == after {
            let value = serde_json::to_value(&result)
                .map_err(|e| StoreError::Corrupt(format!("cached view: {e}")))?;
            let mut cache = self.view_cache.lock().await;
            if cache.version != version || cache.entries.len() >= 32 {
                cache.entries.clear();
                cache.version = version;
            }
            cache.entries.insert(key, (Instant::now(), value));
        }
        Ok(result)
    }
}
