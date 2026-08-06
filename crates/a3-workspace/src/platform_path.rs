use a3_domain::RepositoryPath;
use std::io;
use std::path::{Path, PathBuf};

#[cfg(unix)]
pub(crate) fn bytes(path: &Path) -> Vec<u8> {
    use std::os::unix::ffi::OsStrExt;

    path.as_os_str().as_bytes().to_vec()
}

#[cfg(unix)]
pub(crate) fn repository_path(path: &RepositoryPath) -> io::Result<PathBuf> {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;

    Ok(PathBuf::from(OsStr::from_bytes(path.as_bytes())))
}

#[cfg(windows)]
pub(crate) fn repository_path(path: &RepositoryPath) -> io::Result<PathBuf> {
    let value = std::str::from_utf8(path.as_bytes()).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "repository path is not UTF-8 on Windows",
        )
    })?;
    if value.contains(['\\', ':']) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "repository path contains a Windows separator or stream delimiter",
        ));
    }
    Ok(PathBuf::from(value))
}

#[cfg(not(any(unix, windows)))]
pub(crate) fn repository_path(path: &RepositoryPath) -> io::Result<PathBuf> {
    let value = std::str::from_utf8(path.as_bytes()).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "repository path cannot be represented on this platform",
        )
    })?;
    Ok(PathBuf::from(value))
}

#[cfg(windows)]
pub(crate) fn bytes(path: &Path) -> Vec<u8> {
    use std::os::windows::ffi::OsStrExt;

    path.as_os_str()
        .encode_wide()
        .flat_map(u16::to_le_bytes)
        .collect()
}

#[cfg(not(any(unix, windows)))]
pub(crate) fn bytes(path: &Path) -> Vec<u8> {
    path.to_string_lossy().as_bytes().to_vec()
}
