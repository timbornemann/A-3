use a3_domain::{AgentRunId, MutationActionFingerprint, WorktreeId};
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt;
use std::sync::{Mutex, MutexGuard};

/// Stable terminal class used to compare failed attempts without retaining raw output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum MutationFailureClass {
    /// Another writer changed the expected snapshot, path, or durable CAS anchor.
    Conflict,
    /// Central policy or a capability boundary denied the action.
    Denied,
    /// A bounded operation exceeded its deadline.
    TimedOut,
    /// Cooperative cancellation stopped the operation.
    Cancelled,
    /// A required local tool or adapter was unavailable.
    ToolUnavailable,
    /// Deterministic verification rejected the produced result.
    VerificationFailed,
    /// Changed paths could not be republished into a current index.
    IndexRefreshFailed,
    /// Post-mutation context did not bind the new published snapshot.
    ContextStale,
}

/// Deterministic controller response to a content-identical failure streak.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MutationProgressDecision {
    /// One first failure may return through the normal Verify-to-Execute retry path.
    RetryAllowed,
    /// A second identical failure must leave Execute through the Replan path.
    ReplanRequired,
    /// A third identical failure must stop the run instead of cycling again.
    StopRequired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct MutationOwner {
    run_id: AgentRunId,
    worktree_id: WorktreeId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FailureStreak {
    fingerprint: MutationActionFingerprint,
    class: MutationFailureClass,
    count: u8,
}

#[derive(Debug, Default)]
struct MutationCoordinatorState {
    active_worktrees: BTreeSet<WorktreeId>,
    failure_streaks: BTreeMap<MutationOwner, FailureStreak>,
}

/// Composition-root-owned serialization and progress state for every mutating action type.
///
/// No process-wide singleton exists: one injected instance owns one application runtime, and the
/// lease holds no mutex guard while a long operation awaits.
#[derive(Debug, Default)]
pub struct WorktreeMutationCoordinator {
    state: Mutex<MutationCoordinatorState>,
}

impl WorktreeMutationCoordinator {
    /// Creates a coordinator without active work or failure history.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            state: Mutex::new(MutationCoordinatorState {
                active_worktrees: BTreeSet::new(),
                failure_streaks: BTreeMap::new(),
            }),
        }
    }

    /// Acquires the sole controller-wide mutation capability for one worktree without waiting.
    pub fn try_acquire(
        &self,
        run_id: AgentRunId,
        worktree_id: WorktreeId,
        fingerprint: MutationActionFingerprint,
    ) -> Result<WorktreeMutationLease<'_>, WorktreeMutationBusy> {
        let mut state = lock_recovering_poison(&self.state);
        if !state.active_worktrees.insert(worktree_id) {
            return Err(WorktreeMutationBusy { worktree_id });
        }
        Ok(WorktreeMutationLease {
            coordinator: self,
            owner: MutationOwner {
                run_id,
                worktree_id,
            },
            fingerprint,
        })
    }

    fn record_failure(
        &self,
        owner: MutationOwner,
        fingerprint: MutationActionFingerprint,
        class: MutationFailureClass,
    ) -> MutationProgressDecision {
        let mut state = lock_recovering_poison(&self.state);
        let streak = state.failure_streaks.entry(owner).or_insert(FailureStreak {
            fingerprint,
            class,
            count: 0,
        });
        if streak.fingerprint != fingerprint || streak.class != class {
            *streak = FailureStreak {
                fingerprint,
                class,
                count: 0,
            };
        }
        streak.count = streak.count.saturating_add(1);
        match streak.count {
            1 => MutationProgressDecision::RetryAllowed,
            2 => MutationProgressDecision::ReplanRequired,
            _ => MutationProgressDecision::StopRequired,
        }
    }

    fn record_success(&self, owner: MutationOwner) {
        lock_recovering_poison(&self.state)
            .failure_streaks
            .remove(&owner);
    }

    fn release(&self, worktree_id: WorktreeId) {
        lock_recovering_poison(&self.state)
            .active_worktrees
            .remove(&worktree_id);
    }
}

/// Another patch or process already owns the worktree mutation boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorktreeMutationBusy {
    worktree_id: WorktreeId,
}

impl WorktreeMutationBusy {
    /// Returns the exact worktree whose mutation boundary is occupied.
    #[must_use]
    pub const fn worktree_id(self) -> WorktreeId {
        self.worktree_id
    }
}

impl fmt::Display for WorktreeMutationBusy {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("worktree already has an active mutation")
    }
}

impl Error for WorktreeMutationBusy {}

/// Sole non-cloneable capability for one worktree mutation and its progress observation.
pub struct WorktreeMutationLease<'a> {
    coordinator: &'a WorktreeMutationCoordinator,
    owner: MutationOwner,
    fingerprint: MutationActionFingerprint,
}

impl WorktreeMutationLease<'_> {
    /// Records one terminal failure and returns the mandatory retry, replan, or stop response.
    #[must_use]
    pub fn record_failure(&self, class: MutationFailureClass) -> MutationProgressDecision {
        self.coordinator
            .record_failure(self.owner, self.fingerprint, class)
    }

    /// Clears any prior identical failure streak after a complete successful action.
    pub fn record_success(&self) {
        self.coordinator.record_success(self.owner);
    }

    /// Returns the exact content-free action identity protected by this lease.
    #[must_use]
    pub const fn fingerprint(&self) -> MutationActionFingerprint {
        self.fingerprint
    }
}

impl fmt::Debug for WorktreeMutationLease<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("WorktreeMutationLease")
            .field("owner", &self.owner)
            .field("fingerprint", &self.fingerprint)
            .finish()
    }
}

impl Drop for WorktreeMutationLease<'_> {
    fn drop(&mut self) {
        self.coordinator.release(self.owner.worktree_id);
    }
}

fn lock_recovering_poison<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;
    use a3_domain::{AgentAction, AgentRunAction, DiscoveredCommandId, TaskStepId};

    fn action(value: u8) -> AgentAction {
        AgentAction::Run(AgentRunAction::new(
            TaskStepId::from_bytes([value; 32]),
            DiscoveredCommandId::from_bytes([value.saturating_add(1); 32]),
        ))
    }

    #[test]
    fn one_runtime_lease_serializes_all_mutation_types_per_worktree() -> Result<(), Box<dyn Error>>
    {
        let coordinator = WorktreeMutationCoordinator::new();
        let run_id = AgentRunId::from_bytes([1; 32]);
        let worktree = WorktreeId::from_bytes([2; 32]);
        let fingerprint = MutationActionFingerprint::from_action(&action(3))?;
        let lease = coordinator.try_acquire(run_id, worktree, fingerprint)?;

        assert!(
            coordinator
                .try_acquire(run_id, worktree, fingerprint)
                .is_err()
        );
        let other =
            coordinator.try_acquire(run_id, WorktreeId::from_bytes([4; 32]), fingerprint)?;
        drop(other);
        drop(lease);
        assert!(
            coordinator
                .try_acquire(run_id, worktree, fingerprint)
                .is_ok()
        );
        Ok(())
    }

    #[test]
    fn identical_failure_replans_then_stops_while_changed_action_resets()
    -> Result<(), Box<dyn Error>> {
        let coordinator = WorktreeMutationCoordinator::new();
        let run_id = AgentRunId::from_bytes([5; 32]);
        let worktree = WorktreeId::from_bytes([6; 32]);
        let first = MutationActionFingerprint::from_action(&action(7))?;

        for expected in [
            MutationProgressDecision::RetryAllowed,
            MutationProgressDecision::ReplanRequired,
            MutationProgressDecision::StopRequired,
        ] {
            let lease = coordinator.try_acquire(run_id, worktree, first)?;
            assert_eq!(
                lease.record_failure(MutationFailureClass::VerificationFailed),
                expected
            );
            drop(lease);
        }

        let changed = MutationActionFingerprint::from_action(&action(8))?;
        let lease = coordinator.try_acquire(run_id, worktree, changed)?;
        assert_eq!(
            lease.record_failure(MutationFailureClass::VerificationFailed),
            MutationProgressDecision::RetryAllowed
        );
        lease.record_success();
        drop(lease);
        let lease = coordinator.try_acquire(run_id, worktree, changed)?;
        assert_eq!(
            lease.record_failure(MutationFailureClass::VerificationFailed),
            MutationProgressDecision::RetryAllowed
        );
        Ok(())
    }

    #[test]
    fn read_only_actions_cannot_acquire_a_mutation_fingerprint() {
        assert!(
            MutationActionFingerprint::from_action(&AgentAction::Finish(
                a3_domain::AgentFinishAction
            ))
            .is_err()
        );
    }
}
