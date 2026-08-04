#![allow(
    dead_code,
    reason = "each integration-test crate uses a different subset of the shared fixture helpers"
)]

use std::error::Error;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_DIRECTORY_ID: AtomicU64 = AtomicU64::new(1);

pub(crate) struct TempDirectory {
    path: PathBuf,
}

impl TempDirectory {
    pub(crate) fn new() -> Result<Self, Box<dyn Error>> {
        let id = NEXT_DIRECTORY_ID.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("a3-repo-index-{}-{id}", std::process::id()));
        fs::create_dir(&path)?;
        Ok(Self { path })
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    pub(crate) fn write(
        &self,
        relative: impl AsRef<Path>,
        content: impl AsRef<[u8]>,
    ) -> Result<(), Box<dyn Error>> {
        let path = self.path.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, content)?;
        Ok(())
    }

    pub(crate) fn create_sparse_file(
        &self,
        relative: impl AsRef<Path>,
        length: u64,
    ) -> Result<(), Box<dyn Error>> {
        let path = self.path.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::File::create(path)?.set_len(length)?;
        Ok(())
    }

    pub(crate) fn git<I, S>(&self, arguments: I) -> Result<(), Box<dyn Error>>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let output = Command::new("git")
            .args(arguments)
            .current_dir(&self.path)
            .output()?;
        if !output.status.success() {
            return Err(format!(
                "git command failed: {}",
                String::from_utf8_lossy(&output.stderr)
            )
            .into());
        }
        Ok(())
    }

    #[cfg(unix)]
    pub(crate) fn link_directory(
        &self,
        relative: impl AsRef<Path>,
        target: &Path,
    ) -> Result<(), Box<dyn Error>> {
        use std::os::unix::fs::symlink;

        symlink(target, self.path.join(relative))?;
        Ok(())
    }

    #[cfg(windows)]
    pub(crate) fn link_directory(
        &self,
        relative: impl AsRef<Path>,
        target: &Path,
    ) -> Result<(), Box<dyn Error>> {
        let link = self.path.join(relative);
        let output = Command::new("cmd.exe")
            .args([OsStr::new("/C"), OsStr::new("mklink"), OsStr::new("/J")])
            .arg(&link)
            .arg(target)
            .output()?;
        if !output.status.success() {
            return Err(format!(
                "directory junction creation failed: {}",
                String::from_utf8_lossy(&output.stderr)
            )
            .into());
        }
        Ok(())
    }
}

impl Drop for TempDirectory {
    fn drop(&mut self) {
        let _ignored = fs::remove_dir_all(&self.path);
    }
}
