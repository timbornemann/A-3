#![allow(
    dead_code,
    reason = "acceptance scenarios use different subsets of the shared fixture helpers"
)]

use futures::executor::block_on;
use std::collections::BTreeMap;
use std::error::Error;
use std::ffi::OsStr;
use std::fs;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_DIRECTORY_ID: AtomicU64 = AtomicU64::new(1);

pub(crate) struct TempDirectory {
    path: PathBuf,
}

impl TempDirectory {
    pub(crate) fn new() -> Result<Self, Box<dyn Error>> {
        for _ in 0..100 {
            let id = NEXT_DIRECTORY_ID.fetch_add(1, Ordering::Relaxed);
            let path =
                std::env::temp_dir().join(format!("a3-agent-harness-{}-{id}", std::process::id()));
            match fs::create_dir(&path) {
                Ok(()) => return Ok(Self { path }),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(error.into()),
            }
        }
        Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            "could not reserve a unique agent-harness workspace",
        )
        .into())
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

    pub(crate) fn git<I, S>(&self, arguments: I) -> Result<(), Box<dyn Error>>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let output = Command::new("git")
            .args(arguments)
            .current_dir(&self.path)
            .output()?;
        if output.status.success() {
            Ok(())
        } else {
            Err(std::io::Error::other("temporary fixture Git command failed").into())
        }
    }

    pub(crate) fn repository_tree(&self) -> Result<BTreeMap<String, Vec<u8>>, Box<dyn Error>> {
        let mut files = BTreeMap::new();
        collect_repository_files(&self.path, &self.path, &mut files)?;
        Ok(files)
    }
}

impl Drop for TempDirectory {
    fn drop(&mut self) {
        #[cfg(windows)]
        if std::env::var_os("A3_AGENT_HARNESS_RETAIN_WORKSPACE").as_deref() == Some(OsStr::new("1"))
        {
            return;
        }
        let _ignored = fs::remove_dir_all(&self.path);
    }
}

fn collect_repository_files(
    root: &Path,
    directory: &Path,
    files: &mut BTreeMap<String, Vec<u8>>,
) -> Result<(), Box<dyn Error>> {
    let mut entries = fs::read_dir(directory)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(fs::DirEntry::file_name);
    for entry in entries {
        if entry.path() == root.join(".git") {
            continue;
        }
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            collect_repository_files(root, &entry.path(), files)?;
        } else if file_type.is_file() {
            let relative = entry
                .path()
                .strip_prefix(root)?
                .to_string_lossy()
                .replace('\\', "/");
            files.insert(relative, fs::read(entry.path())?);
        } else {
            return Err(std::io::Error::other(
                "agent-harness fixture contains an unsupported filesystem entry",
            )
            .into());
        }
    }
    Ok(())
}

pub(crate) fn run_libsql_test<F>(future: F) -> Result<(), Box<dyn Error>>
where
    F: Future<Output = Result<(), Box<dyn Error>>>,
{
    #[cfg(windows)]
    let current_thread = std::thread::current();
    #[cfg(windows)]
    let test_name = current_thread.name().ok_or_else(|| {
        std::io::Error::other("libSQL agent-harness test has no harness thread name")
    })?;
    #[cfg(windows)]
    if std::env::var_os("A3_AGENT_HARNESS_ISOLATED_TEST").as_deref() != Some(OsStr::new(test_name))
    {
        const MAX_NATIVE_ATTEMPTS: u8 = 3;
        const STATUS_ACCESS_VIOLATION: i32 = 0xC000_0005_u32 as i32;
        let success_marker = success_marker(test_name);
        for attempt in 1..=MAX_NATIVE_ATTEMPTS {
            remove_success_marker(&success_marker)?;
            let mut child = Command::new(std::env::current_exe()?)
                .arg(test_name)
                .arg("--exact")
                .arg("--nocapture")
                .arg("--test-threads=1")
                .env("A3_AGENT_HARNESS_ISOLATED_TEST", test_name)
                .env("A3_AGENT_HARNESS_SUCCESS_MARKER", &success_marker)
                .spawn()?;
            let child_id = child.id();
            let status = child.wait()?;
            cleanup_workspaces(child_id)?;
            let completed = success_marker.is_file();
            remove_success_marker(&success_marker)?;
            if completed {
                return Ok(());
            }
            if status.code() == Some(STATUS_ACCESS_VIOLATION) && attempt < MAX_NATIVE_ATTEMPTS {
                continue;
            }
            return Err(std::io::Error::other(format!(
                "isolated agent-harness test {test_name} failed on attempt {attempt} with {status} before completion evidence"
            ))
            .into());
        }
        return Err(std::io::Error::other(format!(
            "isolated agent-harness test {test_name} exhausted its native retry bound"
        ))
        .into());
    }
    let result = block_on(future);
    #[cfg(windows)]
    match result {
        Ok(()) => {
            let marker = std::env::var_os("A3_AGENT_HARNESS_SUCCESS_MARKER")
                .ok_or_else(|| std::io::Error::other("agent-harness success marker is missing"))?;
            fs::write(marker, b"complete")?;
            std::process::exit(0);
        }
        Err(error) => {
            eprintln!("libSQL agent-harness test failed: {error:?}");
            std::process::exit(1);
        }
    }
    #[cfg(not(windows))]
    result
}

#[cfg(windows)]
fn success_marker(test_name: &str) -> PathBuf {
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
        "a3-agent-harness-parent-{}-{safe_name}.complete",
        std::process::id()
    ))
}

#[cfg(windows)]
fn remove_success_marker(path: &Path) -> std::io::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

#[cfg(windows)]
fn cleanup_workspaces(child_id: u32) -> std::io::Result<()> {
    let temporary_root = std::env::temp_dir();
    let prefix = format!("a3-agent-harness-{child_id}-");
    for entry in fs::read_dir(&temporary_root)? {
        let entry = entry?;
        if entry.file_name().to_string_lossy().starts_with(&prefix) {
            let path = entry.path();
            if path.parent() == Some(temporary_root.as_path()) {
                fs::remove_dir_all(path)?;
            }
        }
    }
    Ok(())
}
