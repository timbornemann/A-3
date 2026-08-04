use std::error::Error;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const CATALOG_FILE_NAME: &str = "catalog.db";

/// Canonical, application-owned local storage paths supplied by the desktop path adapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageLayout {
    root: PathBuf,
    catalog: PathBuf,
}

impl StorageLayout {
    /// Creates and canonicalizes the application data root without touching repository contents.
    pub fn prepare(app_data_root: impl AsRef<Path>) -> Result<Self, StorageLayoutError> {
        let requested = app_data_root.as_ref();
        if !requested.is_absolute() {
            return Err(StorageLayoutError::RootNotAbsolute);
        }
        fs::create_dir_all(requested).map_err(|source| StorageLayoutError::CreateRoot {
            path: requested.to_path_buf(),
            source,
        })?;
        let root =
            fs::canonicalize(requested).map_err(|source| StorageLayoutError::CanonicalizeRoot {
                path: requested.to_path_buf(),
                source,
            })?;
        let metadata = fs::metadata(&root).map_err(|source| StorageLayoutError::InspectRoot {
            path: root.clone(),
            source,
        })?;
        if !metadata.is_dir() {
            return Err(StorageLayoutError::RootNotDirectory(root));
        }

        let layout = Self {
            catalog: root.join(CATALOG_FILE_NAME),
            root,
        };
        layout.validate_catalog_target()?;
        Ok(layout)
    }

    /// Returns the canonical application data root.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Returns the only path at which the global catalog may be opened.
    #[must_use]
    pub fn catalog_path(&self) -> &Path {
        &self.catalog
    }

    pub(crate) fn validate_catalog_target(&self) -> Result<(), StorageLayoutError> {
        let metadata = match fs::symlink_metadata(&self.catalog) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(source) => {
                return Err(StorageLayoutError::InspectCatalog {
                    path: self.catalog.clone(),
                    source,
                });
            }
        };
        if metadata.file_type().is_symlink() {
            return Err(StorageLayoutError::CatalogIsSymbolicLink(
                self.catalog.clone(),
            ));
        }
        if !metadata.is_file() {
            return Err(StorageLayoutError::CatalogNotRegularFile(
                self.catalog.clone(),
            ));
        }
        let canonical = fs::canonicalize(&self.catalog).map_err(|source| {
            StorageLayoutError::CanonicalizeCatalog {
                path: self.catalog.clone(),
                source,
            }
        })?;
        if !canonical.starts_with(&self.root) {
            return Err(StorageLayoutError::CatalogOutsideRoot { canonical });
        }
        Ok(())
    }
}

/// Failure while establishing the private application-data storage boundary.
#[derive(Debug)]
pub enum StorageLayoutError {
    /// The desktop adapter supplied a relative application-data path.
    RootNotAbsolute,
    /// The application-data directory could not be created.
    CreateRoot {
        /// Requested root path.
        path: PathBuf,
        /// Operating-system failure.
        source: io::Error,
    },
    /// The created root could not be canonicalized.
    CanonicalizeRoot {
        /// Requested root path.
        path: PathBuf,
        /// Operating-system failure.
        source: io::Error,
    },
    /// Root metadata could not be read.
    InspectRoot {
        /// Canonical root path.
        path: PathBuf,
        /// Operating-system failure.
        source: io::Error,
    },
    /// The requested root resolves to a non-directory entry.
    RootNotDirectory(PathBuf),
    /// Catalog metadata could not be read.
    InspectCatalog {
        /// Catalog path.
        path: PathBuf,
        /// Operating-system failure.
        source: io::Error,
    },
    /// An existing catalog path is a symbolic link and is rejected.
    CatalogIsSymbolicLink(PathBuf),
    /// An existing catalog path is not a regular file.
    CatalogNotRegularFile(PathBuf),
    /// An existing catalog file could not be canonicalized.
    CanonicalizeCatalog {
        /// Catalog path.
        path: PathBuf,
        /// Operating-system failure.
        source: io::Error,
    },
    /// The existing catalog resolved outside the application-data root.
    CatalogOutsideRoot {
        /// Canonical path outside the approved root.
        canonical: PathBuf,
    },
}

impl fmt::Display for StorageLayoutError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RootNotAbsolute => formatter.write_str("application data root must be absolute"),
            Self::CreateRoot { path, .. } => {
                write!(
                    formatter,
                    "could not create storage root {}",
                    path.display()
                )
            }
            Self::CanonicalizeRoot { path, .. } => {
                write!(
                    formatter,
                    "could not canonicalize storage root {}",
                    path.display()
                )
            }
            Self::InspectRoot { path, .. } => {
                write!(
                    formatter,
                    "could not inspect storage root {}",
                    path.display()
                )
            }
            Self::RootNotDirectory(path) => {
                write!(
                    formatter,
                    "storage root is not a directory: {}",
                    path.display()
                )
            }
            Self::InspectCatalog { path, .. } => {
                write!(
                    formatter,
                    "could not inspect catalog path {}",
                    path.display()
                )
            }
            Self::CatalogIsSymbolicLink(path) => {
                write!(
                    formatter,
                    "catalog path must not be a symbolic link: {}",
                    path.display()
                )
            }
            Self::CatalogNotRegularFile(path) => {
                write!(
                    formatter,
                    "catalog path is not a regular file: {}",
                    path.display()
                )
            }
            Self::CanonicalizeCatalog { path, .. } => {
                write!(
                    formatter,
                    "could not canonicalize catalog path {}",
                    path.display()
                )
            }
            Self::CatalogOutsideRoot { canonical } => write!(
                formatter,
                "catalog path resolved outside application data root: {}",
                canonical.display()
            ),
        }
    }
}

impl Error for StorageLayoutError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::CreateRoot { source, .. }
            | Self::CanonicalizeRoot { source, .. }
            | Self::InspectRoot { source, .. }
            | Self::InspectCatalog { source, .. }
            | Self::CanonicalizeCatalog { source, .. } => Some(source),
            Self::RootNotAbsolute
            | Self::RootNotDirectory(_)
            | Self::CatalogIsSymbolicLink(_)
            | Self::CatalogNotRegularFile(_)
            | Self::CatalogOutsideRoot { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{StorageLayout, StorageLayoutError};

    #[test]
    fn layout_rejects_relative_roots() {
        assert!(matches!(
            StorageLayout::prepare("relative"),
            Err(StorageLayoutError::RootNotAbsolute)
        ));
    }

    #[test]
    fn layout_rejects_a_file_as_root() -> Result<(), Box<dyn std::error::Error>> {
        let result = StorageLayout::prepare(std::env::current_exe()?);

        assert!(matches!(result, Err(StorageLayoutError::CreateRoot { .. })));
        Ok(())
    }
}
