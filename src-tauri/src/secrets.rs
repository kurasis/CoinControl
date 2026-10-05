//! Provider credential storage (SPECIFICATION.md §10).
//!
//! Keys live in the OS credential store (Windows Credential Manager, macOS
//! Keychain), never in SQLite and never in the renderer bundle. When no
//! persistent store is available, keys are held for the session only and the
//! UI says so. No command ever returns a stored key to the frontend.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Mutex;

use serde::Serialize;
use zeroize::Zeroizing;

#[cfg(not(feature = "native-e2e"))]
const SERVICE: &str = "com.coincontrol.portfoliodesk";
// Automation must never overwrite a user's production credentials.
#[cfg(feature = "native-e2e")]
const SERVICE: &str = "com.coincontrol.portfoliodesk.native-e2e";

/// A credential value. Debug/Display output is always redacted.
pub struct Secret(Zeroizing<String>);

impl Secret {
    pub fn new(value: String) -> Self {
        Secret(Zeroizing::new(value))
    }

    /// Exposes the value to a provider adapter. Never log or serialize the result.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(<redacted>)")
    }
}

/// Where a configured key is kept.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum KeyStorage {
    OsCredentialStore,
    SessionOnly,
}

pub trait SecretStore: Send + Sync {
    fn save(&self, provider: &str, secret: Secret) -> Result<KeyStorage, String>;
    fn remove(&self, provider: &str) -> Result<(), String>;
    fn status(&self, provider: &str) -> Option<KeyStorage>;
    fn get(&self, provider: &str) -> Option<Secret>;
}

/// OS credential store with an explicit, reported session-only fallback.
#[derive(Default)]
pub struct OsSecretStore {
    session: Mutex<BTreeMap<String, Secret>>,
}

impl OsSecretStore {
    fn entry(provider: &str) -> keyring::Result<keyring::Entry> {
        keyring::Entry::new(SERVICE, provider)
    }

    fn session(&self) -> std::sync::MutexGuard<'_, BTreeMap<String, Secret>> {
        self.session.lock().unwrap_or_else(|p| p.into_inner())
    }
}

impl SecretStore for OsSecretStore {
    fn save(&self, provider: &str, secret: Secret) -> Result<KeyStorage, String> {
        if secret.0.trim().is_empty() {
            return Err("The key is empty.".into());
        }
        let persisted = Self::entry(provider).and_then(|e| e.set_password(&secret.0));
        match persisted {
            Ok(()) => {
                self.session().remove(provider);
                Ok(KeyStorage::OsCredentialStore)
            }
            Err(err) => {
                // Never fall back to plaintext on disk: keep it for this session only.
                tracing::warn!(provider, error = %err, "OS credential store unavailable; keeping key for this session only");
                self.session().insert(provider.to_owned(), secret);
                Ok(KeyStorage::SessionOnly)
            }
        }
    }

    fn remove(&self, provider: &str) -> Result<(), String> {
        self.session().remove(provider);
        match Self::entry(provider).and_then(|e| e.delete_credential()) {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(err) => Err(format!(
                "Could not remove the key from the OS credential store: {err}"
            )),
        }
    }

    fn status(&self, provider: &str) -> Option<KeyStorage> {
        if self.session().contains_key(provider) {
            return Some(KeyStorage::SessionOnly);
        }
        Self::entry(provider)
            .and_then(|e| e.get_password())
            .ok()
            .map(|value| {
                drop(Zeroizing::new(value));
                KeyStorage::OsCredentialStore
            })
    }

    fn get(&self, provider: &str) -> Option<Secret> {
        if let Some(s) = self.session().get(provider) {
            return Some(Secret::new(s.0.to_string()));
        }
        Self::entry(provider)
            .and_then(|e| e.get_password())
            .ok()
            .map(Secret::new)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secrets_are_redacted_in_debug_output() {
        let s = Secret::new("sk-test-SENTINEL-0000".into());
        assert_eq!(format!("{s:?}"), "Secret(<redacted>)");
    }
}
