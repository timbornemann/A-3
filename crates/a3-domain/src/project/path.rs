use std::error::Error;
use std::fmt;
use std::path::{Component, Path, PathBuf};

/// Absolute, lexically normalized directory path produced by a filesystem adapter.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CanonicalDirectory(PathBuf);

impl CanonicalDirectory {
    /// Accepts a path after the filesystem adapter has resolved links and canonicalized it.
    pub fn from_canonicalized(path: PathBuf) -> Result<Self, CanonicalDirectoryError> {
        if !path.is_absolute() {
            return Err(CanonicalDirectoryError::NotAbsolute);
        }
        if path
            .components()
            .any(|component| matches!(component, Component::CurDir | Component::ParentDir))
        {
            return Err(CanonicalDirectoryError::NotNormalized);
        }
        Ok(Self(path))
    }

    /// Returns the canonical operating-system path.
    #[must_use]
    pub fn as_path(&self) -> &Path {
        &self.0
    }

    /// Consumes the value and returns its canonical path.
    #[must_use]
    pub fn into_path_buf(self) -> PathBuf {
        self.0
    }
}

/// Invalid canonical-directory representation supplied by an adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CanonicalDirectoryError {
    /// The path was not absolute.
    NotAbsolute,
    /// The path retained `.` or `..` components.
    NotNormalized,
}

impl fmt::Display for CanonicalDirectoryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotAbsolute => formatter.write_str("canonical directory must be absolute"),
            Self::NotNormalized => {
                formatter.write_str("canonical directory must not contain dot components")
            }
        }
    }
}

impl Error for CanonicalDirectoryError {}

#[cfg(test)]
mod tests {
    use super::{CanonicalDirectory, CanonicalDirectoryError};
    use std::path::PathBuf;

    #[test]
    fn canonical_directory_rejects_relative_paths() {
        assert_eq!(
            CanonicalDirectory::from_canonicalized(PathBuf::from("repository")),
            Err(CanonicalDirectoryError::NotAbsolute)
        );
    }
}
