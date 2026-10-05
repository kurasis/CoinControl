//! Application settings stored as validated JSON values.

use crate::{Result, Settings, Store, StoreError};

const SETTINGS_KEY: &str = "app";
const SUPPORTED_LANGUAGES: [&str; 2] = ["en", "ru"];

impl Store {
    pub async fn get_settings(&self) -> Result<Settings> {
        let raw: Option<String> =
            sqlx::query_scalar("SELECT value_json FROM settings WHERE key = ?")
                .bind(SETTINGS_KEY)
                .fetch_optional(&self.pool)
                .await?;
        match raw {
            // Unknown or malformed values fall back to defaults instead of failing startup.
            Some(json) => Ok(serde_json::from_str(&json).unwrap_or_default()),
            None => Ok(Settings::default()),
        }
    }

    pub async fn update_settings(&self, settings: &Settings) -> Result<Settings> {
        if let Some(lang) = &settings.language
            && !SUPPORTED_LANGUAGES.contains(&lang.as_str())
        {
            return Err(StoreError::Invalid(format!(
                "unsupported language {lang:?}"
            )));
        }
        if !(15..=3_600).contains(&settings.price_refresh_seconds) {
            return Err(StoreError::Invalid(
                "price refresh must be 15-3600 seconds".into(),
            ));
        }
        if !(5..=1_440).contains(&settings.sweep_interval_minutes) {
            return Err(StoreError::Invalid(
                "sweep interval must be 5-1440 minutes".into(),
            ));
        }
        let json =
            serde_json::to_string(settings).map_err(|e| StoreError::Invalid(e.to_string()))?;
        let _guard = self.write_lock.lock().await;
        sqlx::query(
            "INSERT INTO settings (key, value_json, updated_at) VALUES (?, ?, ?)
             ON CONFLICT(key) DO UPDATE SET value_json = excluded.value_json, updated_at = excluded.updated_at",
        )
        .bind(SETTINGS_KEY)
        .bind(json)
        .bind(self.now())
        .execute(&self.pool)
        .await?;
        Ok(settings.clone())
    }
}
