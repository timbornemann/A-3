use std::error::Error;
use std::fmt;

const SHA1_HEX_LENGTH: usize = 40;
const SHA256_HEX_LENGTH: usize = 64;
const MAX_REFERENCE_LENGTH: usize = 1_024;

/// Validated SHA-1 or SHA-256 object identifier read from Git.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct GitObjectId(String);

impl GitObjectId {
    /// Validates and normalizes a hexadecimal Git object identifier.
    pub fn try_from_hex(value: impl Into<String>) -> Result<Self, GitObjectIdError> {
        let value = value.into();
        if value.len() != SHA1_HEX_LENGTH && value.len() != SHA256_HEX_LENGTH {
            return Err(GitObjectIdError::InvalidLength(value.len()));
        }
        if !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(GitObjectIdError::InvalidCharacter);
        }
        Ok(Self(value.to_ascii_lowercase()))
    }

    /// Returns the lowercase hexadecimal identifier.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Invalid Git object identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitObjectIdError {
    /// The identifier was neither SHA-1 nor SHA-256 length.
    InvalidLength(usize),
    /// The identifier contained a non-hexadecimal character.
    InvalidCharacter,
}

impl fmt::Display for GitObjectIdError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLength(length) => {
                write!(formatter, "Git object ID has invalid length {length}")
            }
            Self::InvalidCharacter => {
                formatter.write_str("Git object ID contains a non-hexadecimal character")
            }
        }
    }
}

impl Error for GitObjectIdError {}

/// Validated full Git reference name such as `refs/heads/main`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct GitReferenceName(String);

impl GitReferenceName {
    /// Validates a full reference name already accepted by the Git adapter.
    pub fn try_from_full_name(value: impl Into<String>) -> Result<Self, GitReferenceNameError> {
        let value = value.into();
        if value.is_empty() || value.len() > MAX_REFERENCE_LENGTH {
            return Err(GitReferenceNameError::InvalidLength(value.len()));
        }
        if !value.starts_with("refs/") || value.chars().any(char::is_control) {
            return Err(GitReferenceNameError::InvalidFormat);
        }
        Ok(Self(value))
    }

    /// Returns the full reference name.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Invalid full Git reference name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitReferenceNameError {
    /// The reference name was empty or exceeded the bounded representation.
    InvalidLength(usize),
    /// The reference was not a full `refs/...` name or contained a control character.
    InvalidFormat,
}

impl fmt::Display for GitReferenceNameError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLength(length) => {
                write!(formatter, "Git reference name has invalid length {length}")
            }
            Self::InvalidFormat => formatter.write_str("Git reference name has invalid format"),
        }
    }
}

impl Error for GitReferenceNameError {}

/// Current Git HEAD state of a worktree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitHead {
    /// HEAD resolves to an object, optionally through a symbolic branch reference.
    Born {
        /// Commit or other peeled object currently referenced by HEAD.
        object_id: GitObjectId,
        /// Full branch reference, or `None` for detached HEAD.
        reference: Option<GitReferenceName>,
    },
    /// HEAD points at a branch that has no commit yet.
    Unborn {
        /// Full branch reference that will receive the first commit.
        reference: GitReferenceName,
    },
}

#[cfg(test)]
mod tests {
    use super::{GitObjectId, GitObjectIdError, GitReferenceName, GitReferenceNameError};

    #[test]
    fn object_id_accepts_sha1_and_normalizes_case() -> Result<(), GitObjectIdError> {
        let id = GitObjectId::try_from_hex("ABCDEF0123456789ABCDEF0123456789ABCDEF01")?;
        assert_eq!(id.as_str(), "abcdef0123456789abcdef0123456789abcdef01");
        Ok(())
    }

    #[test]
    fn full_reference_rejects_non_ref_names() {
        assert_eq!(
            GitReferenceName::try_from_full_name("main"),
            Err(GitReferenceNameError::InvalidFormat)
        );
    }
}
