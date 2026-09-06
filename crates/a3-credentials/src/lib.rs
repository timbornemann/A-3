//! Native operating-system credential adapters for A^3.

use a3_application::{
    ModelProviderKind, ProviderApiKey, ProviderCredential, ProviderCredentialGeneration,
    ProviderCredentialStore, ProviderCredentialStoreFailure, ProviderCredentialStoreFuture,
};
use a3_domain::ModelProviderId;
use std::fmt;
use zeroize::Zeroizing;

const SERVICE: &str = "dev.timbornemann.a3.provider-api-key";
const ENVELOPE_PREFIX: &str = "a3-provider-api-key-v1";
const ENVELOPE_PREFIX_V2: &str = "a3-provider-api-key-v2";
const MAX_ENVELOPE_BYTES: usize = 8 * 1024;

/// Stores provider credentials in the platform-native credential manager.
#[derive(Clone, Copy, Default)]
pub struct NativeProviderCredentialStore;

impl NativeProviderCredentialStore {
    /// Creates the stateless native credential adapter.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    fn entry(
        provider_id: &ModelProviderId,
    ) -> Result<keyring::Entry, ProviderCredentialStoreFailure> {
        keyring::Entry::new(SERVICE, provider_id.as_str())
            .map_err(|_| ProviderCredentialStoreFailure::Unavailable)
    }

    fn entry_bound(
        provider_id: &ModelProviderId,
        origin_fingerprint: &str,
    ) -> Result<keyring::Entry, ProviderCredentialStoreFailure> {
        let account = format!("{}:{origin_fingerprint}", provider_id.as_str());
        keyring::Entry::new(SERVICE, &account)
            .map_err(|_| ProviderCredentialStoreFailure::Unavailable)
    }

    fn is_official_origin_binding(provider_id: &ModelProviderId, origin_fingerprint: &str) -> bool {
        ModelProviderKind::from_provider_id(provider_id.as_str())
            .is_some_and(|kind| kind.default_origin_fingerprint() == origin_fingerprint)
    }
}

impl fmt::Debug for NativeProviderCredentialStore {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("NativeProviderCredentialStore")
            .finish_non_exhaustive()
    }
}

impl ProviderCredentialStore for NativeProviderCredentialStore {
    fn load<'a>(
        &'a self,
        provider_id: &'a ModelProviderId,
    ) -> ProviderCredentialStoreFuture<'a, Option<ProviderCredential>> {
        Box::pin(async move {
            let entry = Self::entry(provider_id)?;
            let encoded = match entry.get_password() {
                Ok(value) => Zeroizing::new(value),
                Err(keyring::Error::NoEntry) => return Ok(None),
                Err(_) => return Err(ProviderCredentialStoreFailure::Unavailable),
            };
            if encoded.len() > MAX_ENVELOPE_BYTES {
                return Err(ProviderCredentialStoreFailure::Corrupt);
            }
            let mut fields = encoded.splitn(3, '\n');
            if fields.next() != Some(ENVELOPE_PREFIX) {
                return Err(ProviderCredentialStoreFailure::Corrupt);
            }
            let generation = fields
                .next()
                .and_then(|value| value.parse::<u64>().ok())
                .filter(|value| *value > 0)
                .and_then(|value| ProviderCredentialGeneration::new(value).ok())
                .ok_or(ProviderCredentialStoreFailure::Corrupt)?;
            let key = fields
                .next()
                .ok_or(ProviderCredentialStoreFailure::Corrupt)?;
            let secret = ProviderApiKey::from_bytes(key.as_bytes().to_vec())
                .map_err(|_| ProviderCredentialStoreFailure::Corrupt)?;
            Ok(Some(ProviderCredential::new(generation, secret)))
        })
    }

    fn store<'a>(
        &'a self,
        provider_id: &'a ModelProviderId,
        credential: &'a ProviderCredential,
    ) -> ProviderCredentialStoreFuture<'a, ()> {
        Box::pin(async move {
            let entry = Self::entry(provider_id)?;
            let key = std::str::from_utf8(credential.secret().as_bytes())
                .map_err(|_| ProviderCredentialStoreFailure::Corrupt)?;
            let encoded = Zeroizing::new(format!(
                "{ENVELOPE_PREFIX}\n{}\n{key}",
                credential.generation().get()
            ));
            if encoded.len() > MAX_ENVELOPE_BYTES {
                return Err(ProviderCredentialStoreFailure::ResourceLimit);
            }
            entry
                .set_password(&encoded)
                .map_err(|_| ProviderCredentialStoreFailure::Unavailable)
        })
    }

    fn delete<'a>(
        &'a self,
        provider_id: &'a ModelProviderId,
    ) -> ProviderCredentialStoreFuture<'a, ()> {
        Box::pin(async move {
            let entry = Self::entry(provider_id)?;
            match entry.delete_credential() {
                Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
                Err(_) => Err(ProviderCredentialStoreFailure::Unavailable),
            }
        })
    }

    fn load_bound<'a>(
        &'a self,
        provider_id: &'a ModelProviderId,
        origin_fingerprint: &'a str,
    ) -> ProviderCredentialStoreFuture<'a, Option<ProviderCredential>> {
        Box::pin(async move {
            let entry = Self::entry_bound(provider_id, origin_fingerprint)?;
            let encoded = match entry.get_password() {
                Ok(value) => Zeroizing::new(value),
                Err(keyring::Error::NoEntry) => {
                    let legacy_allowed =
                        Self::is_official_origin_binding(provider_id, origin_fingerprint);
                    if legacy_allowed {
                        return self.load(provider_id).await;
                    }
                    return Ok(None);
                }
                Err(_) => return Err(ProviderCredentialStoreFailure::Unavailable),
            };
            if encoded.len() > MAX_ENVELOPE_BYTES {
                return Err(ProviderCredentialStoreFailure::Corrupt);
            }
            let mut fields = encoded.splitn(4, '\n');
            if fields.next() != Some(ENVELOPE_PREFIX_V2)
                || fields.next() != Some(origin_fingerprint)
            {
                return Err(ProviderCredentialStoreFailure::Corrupt);
            }
            let generation = fields
                .next()
                .and_then(|value| value.parse::<u64>().ok())
                .filter(|value| *value > 0)
                .and_then(|value| ProviderCredentialGeneration::new(value).ok())
                .ok_or(ProviderCredentialStoreFailure::Corrupt)?;
            let key = fields
                .next()
                .ok_or(ProviderCredentialStoreFailure::Corrupt)?;
            let secret = ProviderApiKey::from_bytes(key.as_bytes().to_vec())
                .map_err(|_| ProviderCredentialStoreFailure::Corrupt)?;
            Ok(Some(ProviderCredential::new(generation, secret)))
        })
    }

    fn store_bound<'a>(
        &'a self,
        provider_id: &'a ModelProviderId,
        origin_fingerprint: &'a str,
        credential: &'a ProviderCredential,
    ) -> ProviderCredentialStoreFuture<'a, ()> {
        Box::pin(async move {
            let entry = Self::entry_bound(provider_id, origin_fingerprint)?;
            let key = std::str::from_utf8(credential.secret().as_bytes())
                .map_err(|_| ProviderCredentialStoreFailure::Corrupt)?;
            let encoded = Zeroizing::new(format!(
                "{ENVELOPE_PREFIX_V2}\n{origin_fingerprint}\n{}\n{key}",
                credential.generation().get()
            ));
            if encoded.len() > MAX_ENVELOPE_BYTES {
                return Err(ProviderCredentialStoreFailure::ResourceLimit);
            }
            entry
                .set_password(&encoded)
                .map_err(|_| ProviderCredentialStoreFailure::Unavailable)?;
            if Self::is_official_origin_binding(provider_id, origin_fingerprint) {
                let legacy = Self::entry(provider_id)?;
                match legacy.delete_credential() {
                    Ok(()) | Err(keyring::Error::NoEntry) => {}
                    Err(_) => return Err(ProviderCredentialStoreFailure::Unavailable),
                }
            }
            Ok(())
        })
    }

    fn delete_bound<'a>(
        &'a self,
        provider_id: &'a ModelProviderId,
        origin_fingerprint: &'a str,
    ) -> ProviderCredentialStoreFuture<'a, ()> {
        Box::pin(async move {
            let entry = Self::entry_bound(provider_id, origin_fingerprint)?;
            match entry.delete_credential() {
                Ok(()) | Err(keyring::Error::NoEntry) => {}
                Err(_) => return Err(ProviderCredentialStoreFailure::Unavailable),
            }
            if Self::is_official_origin_binding(provider_id, origin_fingerprint) {
                let legacy = Self::entry(provider_id)?;
                match legacy.delete_credential() {
                    Ok(()) | Err(keyring::Error::NoEntry) => {}
                    Err(_) => return Err(ProviderCredentialStoreFailure::Unavailable),
                }
            }
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{NativeProviderCredentialStore, SERVICE};
    use a3_application::{
        ProviderApiKey, ProviderCredential, ProviderCredentialGeneration, ProviderCredentialStore,
    };
    use a3_domain::ModelProviderId;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct CleanupEntry {
        account: String,
    }

    impl Drop for CleanupEntry {
        fn drop(&mut self) {
            if let Ok(entry) = keyring::Entry::new(SERVICE, &self.account) {
                let _ = entry.delete_credential();
            }
        }
    }

    #[test]
    #[ignore = "writes an isolated entry to the native OS credential service"]
    fn native_keyring_roundtrip_uses_a_versioned_generation_envelope()
    -> Result<(), Box<dyn std::error::Error>> {
        futures::executor::block_on(async {
            let suffix = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
            let account = format!("gemini-keyring-smoke-{}-{suffix}", std::process::id());
            let _cleanup = CleanupEntry {
                account: account.clone(),
            };
            let provider_id = ModelProviderId::try_from_string(account)?;
            let store = NativeProviderCredentialStore::new();
            let generation = ProviderCredentialGeneration::new(7)?;
            let credential = ProviderCredential::new(
                generation,
                ProviderApiKey::from_bytes(b"a3-native-keyring-smoke".to_vec())?,
            );

            store.store(&provider_id, &credential).await?;
            let loaded = store
                .load(&provider_id)
                .await?
                .ok_or("native keyring entry was not readable")?;
            assert_eq!(loaded.generation(), generation);
            assert_eq!(loaded.secret().as_bytes(), b"a3-native-keyring-smoke");
            assert!(!format!("{loaded:?}").contains("a3-native-keyring-smoke"));
            store.delete(&provider_id).await?;
            assert!(store.load(&provider_id).await?.is_none());
            Ok::<(), Box<dyn std::error::Error>>(())
        })
    }
}
