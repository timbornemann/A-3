use a3_domain::{CanonicalDirectory, CanonicalDirectoryError};
use std::error::Error;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Filesystem entry kind admitted by the workspace path policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathEntryKind {
    /// Regular file.
    File,
    /// Directory.
    Directory,
}

/// Existing canonical path proven to remain within one selected root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalWorkspacePath {
    path: PathBuf,
    kind: PathEntryKind,
}

impl CanonicalWorkspacePath {
    /// Returns the canonical path.
    #[must_use]
    pub fn as_path(&self) -> &Path {
        &self.path
    }

    /// Returns whether the entry is a regular file or directory.
    #[must_use]
    pub const fn kind(&self) -> PathEntryKind {
        self.kind
    }
}

/// Canonical root boundary selected explicitly by the user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathPolicy {
    root: CanonicalDirectory,
}

impl PathPolicy {
    /// Canonicalizes an explicitly selected directory and establishes it as the only root.
    pub fn from_selected_root(path: impl AsRef<Path>) -> Result<Self, PathPolicyError> {
        let root = canonicalize_directory(path.as_ref())?;
        Ok(Self { root })
    }

    /// Returns the selected canonical root.
    #[must_use]
    pub const fn root(&self) -> &CanonicalDirectory {
        &self.root
    }

    /// Resolves an existing relative or absolute path and proves it remains within the root.
    pub fn resolve_existing(
        &self,
        candidate: impl AsRef<Path>,
    ) -> Result<CanonicalWorkspacePath, PathPolicyError> {
        let candidate = candidate.as_ref();
        let joined = if candidate.is_absolute() {
            candidate.to_path_buf()
        } else {
            self.root.as_path().join(candidate)
        };
        let canonical = canonicalize(&joined)?;
        if !canonical.starts_with(self.root.as_path()) {
            return Err(PathPolicyError::OutsideRoot {
                root: self.root.as_path().to_path_buf(),
                candidate: canonical,
            });
        }
        let metadata = metadata(&canonical)?;
        let kind = if metadata.is_file() {
            PathEntryKind::File
        } else if metadata.is_dir() {
            PathEntryKind::Directory
        } else {
            return Err(PathPolicyError::UnsupportedFileType(canonical));
        };
        Ok(CanonicalWorkspacePath {
            path: canonical,
            kind,
        })
    }
}

pub(crate) fn canonicalize_directory(path: &Path) -> Result<CanonicalDirectory, PathPolicyError> {
    let canonical = canonicalize(path)?;
    let metadata = metadata(&canonical)?;
    if !metadata.is_dir() {
        return Err(PathPolicyError::NotDirectory(canonical));
    }
    CanonicalDirectory::from_canonicalized(canonical).map_err(PathPolicyError::InvalidCanonicalPath)
}

fn canonicalize(path: &Path) -> Result<PathBuf, PathPolicyError> {
    fs::canonicalize(path).map_err(|source| PathPolicyError::Canonicalize {
        path: path.to_path_buf(),
        source,
    })
}

fn metadata(path: &Path) -> Result<fs::Metadata, PathPolicyError> {
    fs::metadata(path).map_err(|source| PathPolicyError::Metadata {
        path: path.to_path_buf(),
        source,
    })
}

/// Failure while establishing or enforcing a workspace root boundary.
#[derive(Debug)]
pub enum PathPolicyError {
    /// The operating system could not canonicalize a path.
    Canonicalize {
        /// Path that failed canonicalization.
        path: PathBuf,
        /// Filesystem failure.
        source: io::Error,
    },
    /// The operating system could not inspect a canonical path.
    Metadata {
        /// Path that could not be inspected.
        path: PathBuf,
        /// Filesystem failure.
        source: io::Error,
    },
    /// The selected root resolved to a non-directory entry.
    NotDirectory(PathBuf),
    /// A resolved path escaped the selected root, including through a symlink.
    OutsideRoot {
        /// Canonical selected root.
        root: PathBuf,
        /// Canonical candidate outside that root.
        candidate: PathBuf,
    },
    /// A pipe, socket, device, or other unsupported entry type was selected.
    UnsupportedFileType(PathBuf),
    /// The domain rejected the adapter's canonical representation.
    InvalidCanonicalPath(CanonicalDirectoryError),
}

impl fmt::Display for PathPolicyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Canonicalize { path, .. } => {
                write!(formatter, "could not canonicalize {}", path.display())
            }
            Self::Metadata { path, .. } => {
                write!(formatter, "could not inspect {}", path.display())
            }
            Self::NotDirectory(path) => {
                write!(
                    formatter,
                    "selected root is not a directory: {}",
                    path.display()
                )
            }
            Self::OutsideRoot { candidate, .. } => {
                write!(
                    formatter,
                    "path is outside the selected root: {}",
                    candidate.display()
                )
            }
            Self::UnsupportedFileType(path) => {
                write!(
                    formatter,
                    "unsupported filesystem entry type: {}",
                    path.display()
                )
            }
            Self::InvalidCanonicalPath(error) => {
                write!(formatter, "invalid canonical path: {error}")
            }
        }
    }
}

impl Error for PathPolicyError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Canonicalize { source, .. } | Self::Metadata { source, .. } => Some(source),
            Self::InvalidCanonicalPath(error) => Some(error),
            Self::NotDirectory(_) | Self::OutsideRoot { .. } | Self::UnsupportedFileType(_) => None,
        }
    }
}
