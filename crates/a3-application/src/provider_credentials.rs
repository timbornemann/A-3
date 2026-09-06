use a3_domain::ModelProviderId;
use std::error::Error;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use zeroize::Zeroizing;

const MAX_PROVIDER_API_KEY_BYTES: usize = 4_096;
const MAX_CREDENTIAL_GENERATION: u64 = i64::MAX as u64;

/// Bounded API key whose managed bytes are overwritten when dropped.
pub struct ProviderApiKey {
    bytes: Zeroizing<Vec<u8>>,
}

impl ProviderApiKey {
    /// Trims surrounding ASCII whitespace and rejects empty, non-ASCII, or control-bearing keys.
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self, ProviderApiKeyError> {
        let bytes = Zeroizing::new(bytes);
        let start = bytes
            .iter()
            .position(|byte| !byte.is_ascii_whitespace())
            .unwrap_or(bytes.len());
        let end = bytes
            .iter()
            .rposition(|byte| !byte.is_ascii_whitespace())
            .map_or(start, |index| index + 1);
        let trimmed = Zeroizing::new(bytes[start..end].to_vec());
        if trimmed.is_empty() || trimmed.len() > MAX_PROVIDER_API_KEY_BYTES {
            return Err(ProviderApiKeyError::InvalidLength);
        }
        if !trimmed.is_ascii() || trimmed.iter().any(|byte| byte.is_ascii_control()) {
            return Err(ProviderApiKeyError::InvalidCharacter);
        }
        Ok(Self { bytes: trimmed })
    }

    /// Borrows the secret only at an authorized native boundary.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }
}

impl fmt::Debug for ProviderApiKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProviderApiKey")
            .field("redacted", &true)
            .finish_non_exhaustive()
    }
}

/// Provider API key did not satisfy the bounded secret envelope.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderApiKeyError {
    /// The trimmed key was empty or exceeded 4096 bytes.
    InvalidLength,
    /// The key contained a non-ASCII byte or embedded control character.
    InvalidCharacter,
}

impl fmt::Display for ProviderApiKeyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidLength => "provider API key length is invalid",
            Self::InvalidCharacter => "provider API key contains an invalid character",
        })
    }
}

impl Error for ProviderApiKeyError {}

/// Monotone content-free generation shared by settings metadata and the OS credential entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ProviderCredentialGeneration(u64);

impl ProviderCredentialGeneration {
    /// Creates a generation representable in local persistence.
    pub const fn new(value: u64) -> Result<Self, ProviderCredentialGenerationError> {
        if value > MAX_CREDENTIAL_GENERATION {
            Err(ProviderCredentialGenerationError)
        } else {
            Ok(Self(value))
        }
    }

    /// Returns the initial generation before any credential mutation.
    #[must_use]
    pub const fn initial() -> Self {
        Self(0)
    }

    /// Returns the persistence integer.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }

    /// Advances the generation for one external credential mutation.
    pub const fn next(self) -> Result<Self, ProviderCredentialGenerationError> {
        match self.0.checked_add(1) {
            Some(value) if value <= MAX_CREDENTIAL_GENERATION => Ok(Self(value)),
            _ => Err(ProviderCredentialGenerationError),
        }
    }
}

/// Credential generation exceeded the durable integer range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProviderCredentialGenerationError;

impl fmt::Display for ProviderCredentialGenerationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("provider credential generation is out of range")
    }
}

impl Error for ProviderCredentialGenerationError {}

/// One secret bound to its durable content-free generation.
pub struct ProviderCredential {
    generation: ProviderCredentialGeneration,
    secret: ProviderApiKey,
}

impl ProviderCredential {
    /// Binds an API key to the corresponding settings generation.
    #[must_use]
    pub const fn new(generation: ProviderCredentialGeneration, secret: ProviderApiKey) -> Self {
        Self { generation, secret }
    }

    /// Returns the content-free generation.
    #[must_use]
    pub const fn generation(&self) -> ProviderCredentialGeneration {
        self.generation
    }

    /// Borrows the secret for a native provider adapter.
    #[must_use]
    pub const fn secret(&self) -> &ProviderApiKey {
        &self.secret
    }

    /// Consumes the envelope and returns the secret.
    #[must_use]
    pub fn into_secret(self) -> ProviderApiKey {
        self.secret
    }
}

impl fmt::Debug for ProviderCredential {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProviderCredential")
            .field("generation", &self.generation)
            .field("secret", &self.secret)
            .finish()
    }
}

/// Future returned by the object-safe OS credential boundary.
pub type ProviderCredentialStoreFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, ProviderCredentialStoreFailure>> + Send + 'a>>;

/// External secure storage for provider credentials.
pub trait ProviderCredentialStore: fmt::Debug + Send + Sync {
    /// Loads one provider credential, or `None` when no entry exists.
    fn load<'a>(
        &'a self,
        provider_id: &'a ModelProviderId,
    ) -> ProviderCredentialStoreFuture<'a, Option<ProviderCredential>>;

    /// Replaces one provider credential atomically at the native backend boundary.
    fn store<'a>(
        &'a self,
        provider_id: &'a ModelProviderId,
        credential: &'a ProviderCredential,
    ) -> ProviderCredentialStoreFuture<'a, ()>;

    /// Deletes one provider entry; an already absent entry is successful.
    fn delete<'a>(
        &'a self,
        provider_id: &'a ModelProviderId,
    ) -> ProviderCredentialStoreFuture<'a, ()>;

    /// Loads a credential bound to an exact canonical-origin fingerprint.
    fn load_bound<'a>(
        &'a self,
        provider_id: &'a ModelProviderId,
        _origin_fingerprint: &'a str,
    ) -> ProviderCredentialStoreFuture<'a, Option<ProviderCredential>> {
        self.load(provider_id)
    }

    /// Stores a credential bound to an exact canonical-origin fingerprint.
    fn store_bound<'a>(
        &'a self,
        provider_id: &'a ModelProviderId,
        _origin_fingerprint: &'a str,
        credential: &'a ProviderCredential,
    ) -> ProviderCredentialStoreFuture<'a, ()> {
        self.store(provider_id, credential)
    }

    /// Deletes the credential bound to an exact canonical-origin fingerprint.
    fn delete_bound<'a>(
        &'a self,
        provider_id: &'a ModelProviderId,
        _origin_fingerprint: &'a str,
    ) -> ProviderCredentialStoreFuture<'a, ()> {
        self.delete(provider_id)
    }
}

/// Stable content-free OS credential storage failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderCredentialStoreFailure {
    /// The native credential service is unavailable or locked.
    Unavailable,
    /// The stored versioned envelope was malformed.
    Corrupt,
    /// The native backend rejected the bounded entry size.
    ResourceLimit,
}

impl fmt::Display for ProviderCredentialStoreFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Unavailable => "provider credential storage is unavailable",
            Self::Corrupt => "provider credential storage contains invalid data",
            Self::ResourceLimit => "provider credential exceeds a native storage limit",
        })
    }
}

impl Error for ProviderCredentialStoreFailure {}

#[cfg(test)]
mod tests {
    use super::{ProviderApiKey, ProviderApiKeyError, ProviderCredentialGeneration};

    #[test]
    fn api_key_trims_outer_whitespace_and_redacts_debug() -> Result<(), Box<dyn std::error::Error>>
    {
        let key = ProviderApiKey::from_bytes(b"  secret-value\r\n".to_vec())?;
        assert_eq!(key.as_bytes(), b"secret-value");
        assert!(!format!("{key:?}").contains("secret-value"));
        Ok(())
    }

    #[test]
    fn api_key_rejects_empty_controls_and_non_ascii() {
        assert!(matches!(
            ProviderApiKey::from_bytes(b" \r\n".to_vec()),
            Err(ProviderApiKeyError::InvalidLength)
        ));
        assert_eq!(
            ProviderApiKey::from_bytes(b"secret\nvalue".to_vec()).err(),
            Some(ProviderApiKeyError::InvalidCharacter)
        );
        assert_eq!(
            ProviderApiKey::from_bytes(vec![0xff]).err(),
            Some(ProviderApiKeyError::InvalidCharacter)
        );
    }

    #[test]
    fn credential_generation_is_bounded_and_monotone() -> Result<(), Box<dyn std::error::Error>> {
        let first = ProviderCredentialGeneration::initial().next()?;
        assert_eq!(first.get(), 1);
        assert!(
            ProviderCredentialGeneration::new(i64::MAX as u64)?
                .next()
                .is_err()
        );
        Ok(())
    }
}
