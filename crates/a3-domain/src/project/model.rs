use std::error::Error;
use std::fmt;

const MAX_MODEL_PROVIDER_ID_BYTES: usize = 128;
const MAX_MODEL_ID_BYTES: usize = 512;

/// Stable provider identity containing neither endpoint nor credential material.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ModelProviderId(String);

impl ModelProviderId {
    /// Validates a bounded provider-neutral identifier.
    pub fn try_from_string(value: String) -> Result<Self, ModelIdentityError> {
        validate_identifier(&value, ModelIdentityKind::Provider)?;
        Ok(Self(value))
    }

    /// Returns the safe provider identifier.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Opaque provider-native model identity with no inferred capabilities.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ModelId(String);

impl ModelId {
    /// Validates a bounded opaque model name used only as provider data.
    pub fn try_from_string(value: String) -> Result<Self, ModelIdentityError> {
        validate_identifier(&value, ModelIdentityKind::Model)?;
        Ok(Self(value))
    }

    /// Returns the opaque model identifier.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Model identity field rejected at the provider-neutral boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelIdentityKind {
    /// Stable provider identifier.
    Provider,
    /// Provider-native model identifier.
    Model,
}

/// Invalid provider or model identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelIdentityError {
    /// Identifier was empty or exceeded its UTF-8 allocation boundary.
    InvalidLength {
        /// Rejected field.
        kind: ModelIdentityKind,
        /// Observed byte count.
        actual: usize,
    },
    /// Identifier contained whitespace, control characters, or unsupported punctuation.
    UnsafeCharacter {
        /// Rejected field.
        kind: ModelIdentityKind,
    },
}

impl fmt::Display for ModelIdentityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLength { kind, actual } => {
                write!(formatter, "{kind} identifier has invalid length {actual}")
            }
            Self::UnsafeCharacter { kind } => {
                write!(formatter, "{kind} identifier contains an unsafe character")
            }
        }
    }
}

impl Error for ModelIdentityError {}

impl fmt::Display for ModelIdentityKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Provider => "model-provider",
            Self::Model => "model",
        })
    }
}

fn validate_identifier(value: &str, kind: ModelIdentityKind) -> Result<(), ModelIdentityError> {
    let maximum = match kind {
        ModelIdentityKind::Provider => MAX_MODEL_PROVIDER_ID_BYTES,
        ModelIdentityKind::Model => MAX_MODEL_ID_BYTES,
    };
    if value.is_empty() || value.len() > maximum {
        return Err(ModelIdentityError::InvalidLength {
            kind,
            actual: value.len(),
        });
    }
    let safe = value.bytes().all(|byte| {
        byte.is_ascii_alphanumeric()
            || matches!(byte, b'-' | b'_' | b'.')
            || matches!(kind, ModelIdentityKind::Model) && matches!(byte, b'/' | b':' | b'@')
    });
    if !safe {
        return Err(ModelIdentityError::UnsafeCharacter { kind });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{ModelId, ModelIdentityError, ModelProviderId};

    #[test]
    fn identities_are_bounded_opaque_and_never_endpoint_shaped() {
        assert!(ModelProviderId::try_from_string("ollama".to_owned()).is_ok());
        assert!(ModelId::try_from_string("hf.co/org/model:Q4_K_M".to_owned()).is_ok());
        assert!(matches!(
            ModelProviderId::try_from_string("https://provider".to_owned()),
            Err(ModelIdentityError::UnsafeCharacter { .. })
        ));
        assert!(ModelId::try_from_string("model\nsecret".to_owned()).is_err());
        assert!(ModelId::try_from_string("x".repeat(513)).is_err());
    }
}
