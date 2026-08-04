use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

const MAX_TEMP_DIRECTORY_ATTEMPTS: u64 = 100;

static NEXT_TEMP_DIRECTORY: AtomicU64 = AtomicU64::new(0);

pub(crate) struct TempDirectory {
    path: PathBuf,
}

impl TempDirectory {
    pub(crate) fn new() -> io::Result<Self> {
        let base = std::env::temp_dir();
        for _ in 0..MAX_TEMP_DIRECTORY_ATTEMPTS {
            let sequence = NEXT_TEMP_DIRECTORY.fetch_add(1, Ordering::Relaxed);
            let path = base.join(format!("a3-storage-test-{}-{sequence}", std::process::id()));
            match fs::create_dir(&path) {
                Ok(()) => return Ok(Self { path }),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(error),
            }
        }
        Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "could not reserve a unique temporary storage test directory",
        ))
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDirectory {
    fn drop(&mut self) {
        if let Err(error) = fs::remove_dir_all(&self.path) {
            eprintln!(
                "could not remove temporary storage test directory {}: {error}",
                self.path.display()
            );
        }
    }
}
