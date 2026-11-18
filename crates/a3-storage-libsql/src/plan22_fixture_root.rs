//! Atomic bounded reservation of fresh private roots for Plan 22 native tests.
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

const MAX_RESERVATION_ATTEMPTS: u64 = 64;

pub(super) fn reserve_fixture_root(
    parent: &Path,
    process_id: u32,
    next: &AtomicU64,
) -> io::Result<PathBuf> {
    for _ in 0..MAX_RESERVATION_ATTEMPTS {
        let root = parent.join(format!(
            "a3-plan22-goal-{process_id}-{}",
            next.fetch_add(1, Ordering::Relaxed)
        ));
        match fs::create_dir(&root) {
            Ok(()) => return Ok(root),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "Plan 22 fixture reservation exhausted 64 occupied candidates",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error;

    type TestResult = Result<(), Box<dyn Error>>;

    struct TestRoot(PathBuf);

    impl TestRoot {
        fn new() -> io::Result<Self> {
            static NEXT_PARENT: AtomicU64 = AtomicU64::new(0);
            let parent = std::env::temp_dir();
            for _ in 0..64 {
                let path = parent.join(format!(
                    "a3-plan22-reservation-test-{}-{}",
                    std::process::id(),
                    NEXT_PARENT.fetch_add(1, Ordering::Relaxed)
                ));
                match fs::create_dir(&path) {
                    Ok(()) => return Ok(Self(path)),
                    Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                    Err(error) => return Err(error),
                }
            }
            Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "reservation regression parent candidates occupied",
            ))
        }

        fn cleanup(self) -> io::Result<()> {
            let path = fs::canonicalize(&self.0)?;
            let parent = fs::canonicalize(std::env::temp_dir())?;
            if path.parent() != Some(parent.as_path())
                || !path.file_name().is_some_and(|name| {
                    name.to_string_lossy()
                        .starts_with("a3-plan22-reservation-test-")
                })
            {
                return Err(io::Error::other(
                    "reservation regression cleanup escaped root",
                ));
            }
            fs::remove_dir_all(path)
        }
    }

    #[test]
    fn occupied_directory_is_preserved_and_next_root_is_fresh() -> TestResult {
        let parent = TestRoot::new()?;
        let occupied = parent.0.join("a3-plan22-goal-7-0");
        fs::create_dir(&occupied)?;
        let marker = occupied.join("owned-by-an-earlier-test");
        fs::write(&marker, b"preserve me")?;
        let next = AtomicU64::new(0);
        let root = reserve_fixture_root(&parent.0, 7, &next)?;
        assert_eq!(root, parent.0.join("a3-plan22-goal-7-1"));
        assert!(fs::read_dir(&root)?.next().is_none());
        assert_eq!(fs::read(&marker)?, b"preserve me");
        let second = reserve_fixture_root(&parent.0, 7, &next)?;
        assert_eq!(second, parent.0.join("a3-plan22-goal-7-2"));
        assert_ne!(root, second);
        assert_eq!(next.load(Ordering::Relaxed), 3);
        parent.cleanup()?;
        Ok(())
    }

    #[test]
    fn occupied_file_is_preserved_and_not_used_as_a_fixture() -> TestResult {
        let parent = TestRoot::new()?;
        let occupied = parent.0.join("a3-plan22-goal-7-0");
        fs::write(&occupied, b"existing file")?;
        let root = reserve_fixture_root(&parent.0, 7, &AtomicU64::new(0))?;
        assert_eq!(root, parent.0.join("a3-plan22-goal-7-1"));
        assert!(root.is_dir());
        assert_eq!(fs::read(occupied)?, b"existing file");
        parent.cleanup()?;
        Ok(())
    }

    #[test]
    fn reservation_stops_after_64_collisions_without_reusing_or_removing_any() -> TestResult {
        let parent = TestRoot::new()?;
        for serial in 0..MAX_RESERVATION_ATTEMPTS {
            fs::create_dir(parent.0.join(format!("a3-plan22-goal-7-{serial}")))?;
        }
        let next = AtomicU64::new(0);
        let error = reserve_fixture_root(&parent.0, 7, &next)
            .err()
            .ok_or("occupied roots were silently reused")?;
        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(next.load(Ordering::Relaxed), 64);
        assert_eq!(fs::read_dir(&parent.0)?.count(), 64);
        parent.cleanup()?;
        Ok(())
    }

    #[test]
    fn unrelated_io_error_is_returned_without_retry() -> TestResult {
        let parent = TestRoot::new()?;
        let next = AtomicU64::new(0);
        let error = reserve_fixture_root(&parent.0.join("missing-parent"), 7, &next)
            .err()
            .ok_or("nonexistent parent was ignored")?;
        assert_eq!(error.kind(), io::ErrorKind::NotFound);
        assert_eq!(next.load(Ordering::Relaxed), 1);
        parent.cleanup()?;
        Ok(())
    }
}
