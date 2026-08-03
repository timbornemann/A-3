use a3_application::{JobClock, JobTimestamp};
use std::time::Instant;

pub(crate) struct SystemJobClock {
    origin: Instant,
}

impl SystemJobClock {
    pub(crate) fn new() -> Self {
        Self {
            origin: Instant::now(),
        }
    }
}

impl JobClock for SystemJobClock {
    fn now(&self) -> JobTimestamp {
        let elapsed = self.origin.elapsed();
        let bounded = elapsed
            .as_secs()
            .saturating_mul(1_000)
            .saturating_add(u64::from(elapsed.subsec_millis()));
        JobTimestamp::from_millis(bounded)
    }
}

#[cfg(test)]
mod tests {
    use super::SystemJobClock;
    use a3_application::JobClock;

    #[test]
    fn system_clock_is_monotone() {
        let clock = SystemJobClock::new();

        assert!(clock.now() <= clock.now());
    }
}
