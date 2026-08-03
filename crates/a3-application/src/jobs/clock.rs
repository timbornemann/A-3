/// Monotone scheduler timestamp expressed as elapsed milliseconds.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct JobTimestamp(u64);

impl JobTimestamp {
    /// Creates a timestamp from a clock-owned monotone millisecond value.
    #[must_use]
    pub const fn from_millis(milliseconds: u64) -> Self {
        Self(milliseconds)
    }

    /// Returns elapsed milliseconds for serialization or diagnostics.
    #[must_use]
    pub const fn as_millis(self) -> u64 {
        self.0
    }
}

/// Injected monotone time source used to timestamp job events deterministically.
pub trait JobClock: Send + Sync + 'static {
    /// Returns the current monotone scheduler timestamp.
    fn now(&self) -> JobTimestamp;
}
