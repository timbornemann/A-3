#![allow(
    dead_code,
    reason = "each integration-test crate uses a different subset of the shared fixture helpers"
)]

use std::error::Error;
use std::ffi::OsStr;
use std::fs;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use futures::executor::block_on;

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

pub(crate) fn run_libsql_test<F>(future: F) -> Result<(), Box<dyn Error>>
where
    F: Future<Output = Result<(), Box<dyn Error>>>,
{
    run_libsql_test_selected(future, false)
}

pub(crate) fn run_libsql_test_selected<F>(
    future: F,
    include_ignored: bool,
) -> Result<(), Box<dyn Error>>
where
    F: Future<Output = Result<(), Box<dyn Error>>>,
{
    #[cfg(not(windows))]
    let _ = include_ignored;
    #[cfg(windows)]
    let current_thread = std::thread::current();
    #[cfg(windows)]
    let test_name = current_thread.name().ok_or_else(|| {
        std::io::Error::other("libSQL repository-index test has no harness thread name")
    })?;
    #[cfg(windows)]
    if std::env::var_os("A3_LIBSQL_ISOLATED_TEST").as_deref()
        != Some(std::ffi::OsStr::new(test_name))
    {
        const MAX_NATIVE_ATTEMPTS: u8 = 3;
        const STATUS_ACCESS_VIOLATION: i32 = 0xC000_0005_u32 as i32;
        let success_marker = libsql_success_marker(test_name);
        for attempt in 1..=MAX_NATIVE_ATTEMPTS {
            remove_libsql_success_marker(&success_marker)?;
            let mut command = std::process::Command::new(std::env::current_exe()?);
            command.arg(test_name).arg("--exact");
            if include_ignored {
                command.arg("--include-ignored");
            }
            let mut child = command
                .arg("--test-threads=1")
                .env("A3_LIBSQL_ISOLATED_TEST", test_name)
                .env("A3_REPO_INDEX_SUCCESS_MARKER", &success_marker)
                .spawn()?;
            let child_id = child.id();
            let status = child.wait()?;
            cleanup_libsql_workspaces(child_id)?;
            let contract_completed = success_marker.is_file();
            remove_libsql_success_marker(&success_marker)?;
            if contract_completed {
                return Ok(());
            }
            if status.code() == Some(STATUS_ACCESS_VIOLATION) && attempt < MAX_NATIVE_ATTEMPTS {
                continue;
            }
            return Err(std::io::Error::other(format!(
                "isolated libSQL repository-index test {test_name} failed on attempt {attempt} with {status} before completion evidence"
            ))
            .into());
        }
        return Err(std::io::Error::other(format!(
            "isolated libSQL repository-index test {test_name} exhausted its native retry bound"
        ))
        .into());
    }
    let result = block_on(future);
    #[cfg(windows)]
    match result {
        Ok(()) => {
            let marker = std::env::var_os("A3_REPO_INDEX_SUCCESS_MARKER")
                .ok_or_else(|| std::io::Error::other("libSQL success marker is missing"))?;
            std::fs::write(marker, b"complete")?;
            std::process::exit(0);
        }
        Err(error) => {
            eprintln!("libSQL repository-index test failed: {error}");
            std::process::exit(1);
        }
    }
    #[cfg(not(windows))]
    result
}

#[cfg(windows)]
fn libsql_success_marker(test_name: &str) -> PathBuf {
    let safe_name = test_name
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '_' {
                character
            } else {
                '_'
            }
        })
        .collect::<String>();
    std::env::temp_dir().join(format!(
        "a3-repo-index-parent-{}-{safe_name}.complete",
        std::process::id()
    ))
}

#[cfg(windows)]
fn remove_libsql_success_marker(path: &Path) -> std::io::Result<()> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

#[cfg(windows)]
fn cleanup_libsql_workspaces(child_id: u32) -> std::io::Result<()> {
    let temporary_root = std::env::temp_dir();
    let prefix = format!("a3-repo-index-{child_id}-");
    for entry in std::fs::read_dir(&temporary_root)? {
        let entry = entry?;
        if entry.file_name().to_string_lossy().starts_with(&prefix) {
            let path = entry.path();
            if path.parent() == Some(temporary_root.as_path()) {
                std::fs::remove_dir_all(path)?;
            }
        }
    }
    Ok(())
}
