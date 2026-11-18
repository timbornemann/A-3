use super::{Failure, Ownership, ProcessRunControl, finish_observation, observe_until};
use a3_application::ProcessRunFailure;
use std::{
    cell::Cell,
    time::{Duration, Instant},
};

#[derive(Debug)]
struct Control(bool);
impl ProcessRunControl for Control {
    fn is_cancelled(&self) -> bool {
        self.0
    }
    fn wait_cancelled_timeout(&self, _: Duration) -> bool {
        self.0
    }
}

#[test]
fn expired_effect_deadline_never_begins_an_observation() {
    let called = Cell::new(false);
    let observed = observe_until(Instant::now(), &Control(false), |_| {
        called.set(true);
        Ok::<_, ProcessRunFailure>(Ownership::Owned)
    });
    assert_eq!(observed, Err(Failure::TimedOut));
    assert!(!called.get());
}

#[test]
fn observation_error_before_deadline_remains_unavailable() {
    let end = Instant::now() + Duration::from_secs(60);
    assert_eq!(
        observe_until(end, &Control(false), |_| Err::<Ownership, _>(
            ProcessRunFailure::OutputUnavailable
        )),
        Err(Failure::Unavailable)
    );
    assert_eq!(
        finish_observation(Ok(Ownership::Owned), end, &Control(false)),
        Ok(Ownership::Owned)
    );
}

#[test]
fn elapsed_effect_deadline_rejects_completed_error_and_owned_result() {
    let end = Instant::now();
    assert_eq!(
        finish_observation::<Ownership>(Err(Failure::Unavailable), end, &Control(false)),
        Err(Failure::TimedOut)
    );
    assert_eq!(
        finish_observation(Ok(Ownership::Owned), end, &Control(false)),
        Err(Failure::TimedOut)
    );
}

#[test]
fn cancellation_precedes_deadline_and_completed_observation() {
    let end = Instant::now();
    assert_eq!(
        finish_observation::<Ownership>(Err(Failure::Unavailable), end, &Control(true)),
        Err(Failure::Cancelled)
    );
    assert_eq!(
        finish_observation(Ok(Ownership::Owned), end, &Control(true)),
        Err(Failure::Cancelled)
    );
    let called = Cell::new(false);
    assert_eq!(
        observe_until(end + Duration::from_secs(60), &Control(true), |_| {
            called.set(true);
            Ok::<_, ProcessRunFailure>(Ownership::Owned)
        }),
        Err(Failure::Cancelled)
    );
    assert!(!called.get());
}
