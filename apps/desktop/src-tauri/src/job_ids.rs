use a3_domain::JobId;
use std::error::Error;
use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};

/// Process-wide monotone allocator shared by every owner using the desktop scheduler.
#[derive(Debug)]
pub(crate) struct DesktopJobIds {
    next: AtomicU64,
}

impl DesktopJobIds {
    pub(crate) const fn new() -> Self {
        Self {
            next: AtomicU64::new(1),
        }
    }

    pub(crate) fn allocate(&self) -> Result<JobId, DesktopJobIdError> {
        let value = self
            .next
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
                current.checked_add(1)
            })
            .map_err(|_| DesktopJobIdError::Exhausted)?;
        Ok(JobId::new(value))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DesktopJobIdError {
    Exhausted,
}

impl fmt::Display for DesktopJobIdError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("desktop job identifier space is exhausted")
    }
}

impl Error for DesktopJobIdError {}

#[cfg(test)]
mod tests {
    use super::DesktopJobIds;
    use std::error::Error;

    #[test]
    fn allocator_is_process_wide_monotone_and_nonzero() -> Result<(), Box<dyn Error>> {
        let ids = DesktopJobIds::new();
        assert_eq!(ids.allocate()?.value(), 1);
        assert_eq!(ids.allocate()?.value(), 2);
        Ok(())
    }
}
