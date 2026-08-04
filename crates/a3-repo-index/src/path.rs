//! Lossless conversion between Git repository paths and operating-system paths.

use a3_domain::RepositoryPath;
use std::fs::{File, Metadata};
use std::io;
use std::path::{Path, PathBuf};

/// Safe observation of a repository path without following a final symbolic link.
#[derive(Debug)]
pub(crate) enum RepositoryPathObservation {
    /// The candidate disappeared before it could be observed.
    Missing,
    /// The candidate or one of its ancestors is a symbolic link or reparse point.
    SymbolicLink,
    /// The candidate is available with no symlink ancestor.
    Present { path: PathBuf, metadata: Metadata },
}

pub(crate) fn observe_repository_path(
    root: &Path,
    repository_path: &RepositoryPath,
) -> io::Result<RepositoryPathObservation> {
    let total_components = repository_path
        .as_bytes()
        .split(|byte| *byte == b'/')
        .count();
    let components = repository_path.as_bytes().split(|byte| *byte == b'/');
    let mut path = root.to_path_buf();
    let mut component_count = 0usize;
    for component in components {
        component_count = component_count.saturating_add(1);
        path.push(component_to_os_path(component)?);
        if component_count == total_components {
            break;
        }
        let metadata = match std::fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Ok(RepositoryPathObservation::Missing);
            }
            Err(error) => return Err(error),
        };
        if is_symlink_or_reparse(&metadata) {
            return Ok(RepositoryPathObservation::SymbolicLink);
        }
        if !metadata.is_dir() {
            return Ok(RepositoryPathObservation::Missing);
        }
    }

    let metadata = match std::fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(RepositoryPathObservation::Missing);
        }
        Err(error) => return Err(error),
    };
    if is_symlink_or_reparse(&metadata) {
        return Ok(RepositoryPathObservation::SymbolicLink);
    }

    let canonical = std::fs::canonicalize(&path)?;
    if !canonical.starts_with(root) {
        return Ok(RepositoryPathObservation::SymbolicLink);
    }
    Ok(RepositoryPathObservation::Present { path, metadata })
}

pub(crate) fn open_regular_no_follow(path: &Path) -> io::Result<File> {
    #[cfg(unix)]
    {
        use rustix::fs::{Mode, OFlags, open};

        let descriptor = open(
            path,
            OFlags::RDONLY | OFlags::CLOEXEC | OFlags::NOFOLLOW,
            Mode::empty(),
        )?;
        let file = File::from(descriptor);
        if !file.metadata()?.is_file() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "repository path is not a regular file",
            ));
        }
        Ok(file)
    }

    #[cfg(windows)]
    {
        use std::os::windows::fs::{MetadataExt, OpenOptionsExt};

        const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
        let file = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
            .open(path)?;
        let metadata = file.metadata()?;
        if !metadata.is_file() || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "repository path is not a regular non-reparse file",
            ));
        }
        Ok(file)
    }

    #[cfg(not(any(unix, windows)))]
    {
        let file = File::open(path)?;
        if !file.metadata()?.is_file() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "repository path is not a regular file",
            ));
        }
        Ok(file)
    }
}

#[cfg(unix)]
fn component_to_os_path(component: &[u8]) -> io::Result<PathBuf> {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;

    Ok(PathBuf::from(OsStr::from_bytes(component)))
}

#[cfg(windows)]
fn component_to_os_path(component: &[u8]) -> io::Result<PathBuf> {
    let component = std::str::from_utf8(component).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "Git path is not UTF-8 on Windows",
        )
    })?;
    if component.contains(['\\', ':']) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Git path contains a Windows separator or stream delimiter",
        ));
    }
    Ok(PathBuf::from(component))
}

#[cfg(not(any(unix, windows)))]
fn component_to_os_path(component: &[u8]) -> io::Result<PathBuf> {
    let component = std::str::from_utf8(component).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "Git path cannot be represented on this platform",
        )
    })?;
    Ok(PathBuf::from(component))
}

#[cfg(windows)]
fn is_symlink_or_reparse(metadata: &Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;

    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
    metadata.file_type().is_symlink()
        || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
fn is_symlink_or_reparse(metadata: &Metadata) -> bool {
    metadata.file_type().is_symlink()
}
