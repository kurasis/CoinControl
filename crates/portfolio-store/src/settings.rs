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
        let mut settings: Settings = match raw {
            // Unknown or malformed values fall back to defaults instead of failing startup.
            Some(json) => serde_json::from_str(&json).unwrap_or_default(),
            None => Settings::default(),
        };
        if settings
            .timezone
            .as_deref()
            .is_some_and(|zone| jiff_tzdb::get(zone).is_none())
        {
            // Older databases/backups may contain unchecked overrides. Preserve
            // all other preferences and use the OS zone without rewriting data.
            settings.timezone = None;
        }
        Ok(settings)
    }

    pub async fn update_settings(&self, settings: &Settings) -> Result<Settings> {
        if let Some(lang) = &settings.language
            && !SUPPORTED_LANGUAGES.contains(&lang.as_str())
        {
            return Err(StoreError::Invalid(format!(
                "unsupported language {lang:?}"
            )));
        }
        if settings
            .timezone
            .as_deref()
            .is_some_and(|zone| jiff_tzdb::get(zone).is_none())
        {
            return Err(StoreError::Invalid(
                "timezone must be an IANA name or null for the system timezone".into(),
            ));
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ProfileKind, ThemePreference};
    use portfolio_core::clock::FixedClock;
    use std::sync::Arc;

    #[tokio::test]
    async fn timezone_save_accepts_iana_names_and_system_default() {
        let store = Store::open_in_memory(ProfileKind::Test, Arc::new(FixedClock(1_790_000_000)))
            .await
            .unwrap();
        for zone in [
            None,
            Some("UTC"),
            Some("Africa/Nairobi"),
            Some("America/New_York"),
            Some("Asia/Calcutta"),
            Some("Etc/GMT+3"),
            Some("europe/london"),
        ] {
            let settings = Settings {
                timezone: zone.map(str::to_owned),
                ..Settings::default()
            };
            assert_eq!(store.update_settings(&settings).await.unwrap(), settings);
            assert_eq!(store.get_settings().await.unwrap(), settings);
        }
    }

    #[tokio::test]
    async fn invalid_timezone_save_leaves_previous_settings_untouched() {
        let store = Store::open_in_memory(ProfileKind::Test, Arc::new(FixedClock(1_790_000_000)))
            .await
            .unwrap();
        let saved = Settings {
            timezone: Some("Africa/Nairobi".into()),
            privacy_mode: true,
            ..Settings::default()
        };
        store.update_settings(&saved).await.unwrap();
        for zone in [
            "",
            " UTC ",
            "Invalid/Timezone",
            "+03:00",
            "GMT+3",
            "../Europe/London",
        ] {
            let invalid = Settings {
                timezone: Some(zone.into()),
                ..saved.clone()
            };
            assert!(
                matches!(
                    store.update_settings(&invalid).await,
                    Err(StoreError::Invalid(_))
                ),
                "{zone}"
            );
            assert_eq!(store.get_settings().await.unwrap(), saved);
        }
    }

    #[tokio::test]
    async fn restored_invalid_timezone_keeps_other_preferences_and_backup_data() {
        let clock = Arc::new(FixedClock(1_790_000_000));
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(
            &dir.path().join("original.sqlite"),
            ProfileKind::Test,
            clock.clone(),
        )
        .await
        .unwrap();
        let settings = Settings {
            timezone: Some("Invalid/Timezone".into()),
            language: Some("ru".into()),
            theme: ThemePreference::Light,
            privacy_mode: true,
            network_console_enabled: true,
            price_refresh_seconds: 120,
            sweep_interval_minutes: 30,
        };
        // Simulate a database written by an older build without save validation.
        let json = serde_json::to_string(&settings).unwrap();
        sqlx::query("INSERT INTO settings (key, value_json, updated_at) VALUES ('app', ?, 0)")
            .bind(&json)
            .execute(&store.pool)
            .await
            .unwrap();
        let backup = store.export_backup().await.unwrap();
        let restored = Store::open(
            &dir.path().join("restored.sqlite"),
            ProfileKind::Test,
            clock,
        )
        .await
        .unwrap();
        restored
            .restore_backup(&backup, &dir.path().join("safety.ccbackup"))
            .await
            .unwrap();
        let mut expected = settings;
        expected.timezone = None;
        assert_eq!(store.get_settings().await.unwrap(), expected);
        assert_eq!(restored.get_settings().await.unwrap(), expected);
        let raw: String = sqlx::query_scalar("SELECT value_json FROM settings WHERE key = 'app'")
            .fetch_one(&restored.pool)
            .await
            .unwrap();
        assert_eq!(
            raw, json,
            "reading settings must not rewrite backup contents"
        );
    }
    #[test]
    fn old_settings_keep_language_privacy_and_intervals_when_console_is_added() {
        let settings: Settings = serde_json::from_str(r#"{"language":"ru","theme":"light","timezone":"Africa/Nairobi","privacy_mode":true,"price_refresh_seconds":120,"sweep_interval_minutes":30}"#).unwrap();
        assert!(!settings.network_console_enabled);
        assert_eq!(settings.language.as_deref(), Some("ru"));
        assert!(settings.privacy_mode);
        assert_eq!(settings.price_refresh_seconds, 120);
        assert_eq!(settings.sweep_interval_minutes, 30);
    }
}
