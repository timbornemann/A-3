use a3_domain::{
    GoalContract, GoalContractDraft, GoalContractReference, GoalContractRevision,
    GoalContractRevisionFailure, GoalContractTimestamp, GoalOriginalBindingError,
    GoalRevisionReason, ProjectIdentity, TaskId,
};
use std::error::Error;
use std::fmt;
use std::future::Future;
use std::pin::Pin;

/// Owned future returned by the object-safe Goal Contract persistence port.
pub type GoalContractStoreFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, GoalContractStoreFailure>> + Send + 'a>>;

/// Persistence boundary for immutable task goals and their append-only revisions.
pub trait GoalContractStore: fmt::Debug + Send + Sync {
    /// Loads bounded append-only interpretation history from the Goal authority.
    /// It remains available for audit after Pending is consumed by promotion.
    fn load_goal_intake_review<'a>(
        &'a self,
        _project: &'a ProjectIdentity,
        _task_id: TaskId,
    ) -> GoalContractStoreFuture<'a, Option<a3_domain::GoalIntakeReview>> {
        Box::pin(async { Err(GoalContractStoreFailure::UnsupportedSchema) })
    }

    /// Revalidates actual Pending, both CAS dimensions and the immutable prefix,
    /// then appends exactly one bounded batch. No owner/provider work is started.
    #[allow(clippy::too_many_arguments)]
    fn append_goal_intake_review<'a>(
        &'a self,
        _project: &'a ProjectIdentity,
        _task_id: TaskId,
        _expected_capture_version: crate::GoalIntakeStoreVersion,
        _expected_review_revision: Option<a3_domain::GoalIntakeReviewRevision>,
        _review: &'a a3_domain::GoalIntakeReview,
    ) -> GoalContractStoreFuture<'a, a3_domain::GoalIntakeReviewRevision> {
        Box::pin(async { Err(GoalContractStoreFailure::UnsupportedSchema) })
    }

    /// Finds at most 32 pending task identities after a stable keyset cursor.
    /// No presentation history, original payload, executable state or auto-resume is implied.
    fn list_pending_goal_intakes<'a>(
        &'a self,
        _project: &'a ProjectIdentity,
        _after: Option<TaskId>,
    ) -> GoalContractStoreFuture<'a, crate::PendingGoalIntakePage> {
        Box::pin(async { Err(GoalContractStoreFailure::UnsupportedSchema) })
    }

    /// Captures original bytes once, before any Goal draft or intake model work exists.
    /// Baseline adapters cannot silently discard capture: unsupported is an explicit failure.
    fn capture_goal_intake<'a>(
        &'a self,
        _project: &'a ProjectIdentity,
        _pending: &'a a3_domain::PendingGoalIntake,
    ) -> GoalContractStoreFuture<'a, crate::GoalIntakeStoreVersion> {
        Box::pin(async { Err(GoalContractStoreFailure::UnsupportedSchema) })
    }

    /// Reads Pending or the actual initial Goal identity in one consistent task-bound view.
    /// Materialized does not claim a prior capture or promotion receipt.
    fn load_goal_intake<'a>(
        &'a self,
        _project: &'a ProjectIdentity,
        _task_id: TaskId,
    ) -> GoalContractStoreFuture<'a, Option<crate::GoalIntakeLoadResult>> {
        Box::pin(async { Err(GoalContractStoreFailure::UnsupportedSchema) })
    }

    /// Revalidates exact pending bytes and CAS, then consumes Pending and creates Goal 1
    /// in one transaction. Any failure preserves Pending; no retry is implied.
    fn promote_goal_intake<'a>(
        &'a self,
        _project: &'a ProjectIdentity,
        _task_id: TaskId,
        _expected_version: crate::GoalIntakeStoreVersion,
        _goal: &'a GoalContract,
    ) -> GoalContractStoreFuture<'a, ()> {
        Box::pin(async { Err(GoalContractStoreFailure::UnsupportedSchema) })
    }

    /// Creates one task together with its required initial Goal Contract revision atomically.
    fn create_goal_contract<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        contract: &'a GoalContract,
    ) -> GoalContractStoreFuture<'a, ()>;

    /// Appends exactly the immediate next Goal Contract revision atomically.
    fn append_goal_contract_revision<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        contract: &'a GoalContract,
    ) -> GoalContractStoreFuture<'a, ()>;

    /// Loads the current immutable Goal Contract for one task.
    fn load_current_goal_contract<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        task_id: TaskId,
    ) -> GoalContractStoreFuture<'a, Option<GoalContract>>;

    /// Loads one exact historical revision for audit, resume, or comparison.
    fn load_goal_contract_revision<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        task_id: TaskId,
        revision: GoalContractRevision,
    ) -> GoalContractStoreFuture<'a, Option<GoalContract>>;
}

/// Stable application classification of Goal Contract persistence failures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GoalContractStoreFailure {
    /// Another append changed the independent interpretation history.
    IntakeReviewVersionConflict,
    /// Actual sources, prefix, bounds or finding preservation rejected the review.
    InvalidIntakeReview,
    /// Local worktree storage could not be reached or written.
    Unavailable,
    /// Local worktree storage failed its integrity checks.
    Corrupt,
    /// The database schema is newer than this application build.
    UnsupportedSchema,
    /// Durable content violated the Goal Contract schema or domain invariants.
    InvalidStoredData,
    /// A task with the same identity already exists in this worktree.
    TaskAlreadyExists,
    /// The requested durable task does not exist in this worktree.
    TaskNotFound,
    /// Another writer already advanced the current Goal Contract revision.
    RevisionConflict,
    /// Pending capture no longer has the expected immutable store version.
    IntakeVersionConflict,
    /// No pending original capture can be consumed for this task.
    IntakeNotPending,
    /// The proposal does not retain the exact pending original and complete initial binding.
    InvalidIntakePromotion,
}

impl fmt::Display for GoalContractStoreFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::IntakeReviewVersionConflict => "Goal intake review revision conflicts",
            Self::InvalidIntakeReview => "Goal intake review is invalid",
            Self::Unavailable => "Goal Contract storage is unavailable",
            Self::Corrupt => "Goal Contract storage is corrupt",
            Self::UnsupportedSchema => "Goal Contract storage uses an unsupported schema",
            Self::InvalidStoredData => "Goal Contract storage contains invalid data",
            Self::TaskAlreadyExists => "Goal Contract task already exists",
            Self::TaskNotFound => "Goal Contract task was not found",
            Self::RevisionConflict => "Goal Contract revision conflicts with the current revision",
            Self::IntakeVersionConflict => "Goal intake store version conflicts with current state",
            Self::IntakeNotPending => "Goal intake is not pending",
            Self::InvalidIntakePromotion => "Goal intake promotion is invalid",
        })
    }
}

impl Error for GoalContractStoreFailure {}

/// Inbound use case creating a durable task only together with a valid initial goal.
#[derive(Debug, Clone, Copy)]
pub struct CreateGoalContract<'a> {
    store: &'a dyn GoalContractStore,
}

impl<'a> CreateGoalContract<'a> {
    /// Creates the use case from its narrow persistence capability.
    #[must_use]
    pub const fn new(store: &'a dyn GoalContractStore) -> Self {
        Self { store }
    }

    /// Atomically persists the initial revision and returns the only run-safe reference.
    pub async fn execute(
        self,
        project: &ProjectIdentity,
        contract: &GoalContract,
    ) -> Result<GoalContractReference, CreateGoalContractFailure> {
        if contract.revision() != GoalContractRevision::INITIAL
            || contract.previous_revision().is_some()
            || contract.revision_reason().is_some()
        {
            return Err(CreateGoalContractFailure::InvalidInitialRevision);
        }
        contract
            .validate_original_binding()
            .map_err(CreateGoalContractFailure::InvalidOriginalBinding)?;
        self.store
            .create_goal_contract(project, contract)
            .await
            .map_err(CreateGoalContractFailure::Store)?;
        Ok(contract.reference())
    }
}

/// Initial Goal Contract creation failed before a run could reference it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreateGoalContractFailure {
    /// Original bytes or criterion provenance do not belong to this immutable goal.
    InvalidOriginalBinding(GoalOriginalBindingError),
    /// The supplied contract was not revision one.
    InvalidInitialRevision,
    /// Persistence rejected the atomic task-and-goal creation.
    Store(GoalContractStoreFailure),
}

impl fmt::Display for CreateGoalContractFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidOriginalBinding(error) => {
                write!(formatter, "Goal Contract original binding failed: {error}")
            }
            Self::InvalidInitialRevision => {
                formatter.write_str("task creation requires the initial Goal Contract revision")
            }
            Self::Store(error) => write!(formatter, "Goal Contract creation failed: {error}"),
        }
    }
}

impl Error for CreateGoalContractFailure {}

/// Inbound use case deriving and atomically appending the next immutable goal revision.
#[derive(Debug, Clone, Copy)]
pub struct ReviseGoalContract<'a> {
    store: &'a dyn GoalContractStore,
}

impl<'a> ReviseGoalContract<'a> {
    /// Creates the use case from its narrow persistence capability.
    #[must_use]
    pub const fn new(store: &'a dyn GoalContractStore) -> Self {
        Self { store }
    }

    /// Loads the current revision, derives its immediate successor, and appends it atomically.
    pub async fn execute(
        self,
        project: &ProjectIdentity,
        task_id: TaskId,
        draft: GoalContractDraft,
        reason: GoalRevisionReason,
        created_at: GoalContractTimestamp,
    ) -> Result<GoalContract, ReviseGoalContractFailure> {
        let current = self
            .store
            .load_current_goal_contract(project, task_id)
            .await
            .map_err(ReviseGoalContractFailure::Store)?
            .ok_or(ReviseGoalContractFailure::TaskNotFound)?;
        let revised = current
            .revise(draft, reason, created_at)
            .map_err(ReviseGoalContractFailure::InvalidRevision)?;
        self.store
            .append_goal_contract_revision(project, &revised)
            .await
            .map_err(ReviseGoalContractFailure::Store)?;
        Ok(revised)
    }
}

/// Goal Contract revision failed before or during its atomic append.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviseGoalContractFailure {
    /// The requested task has no current Goal Contract in this worktree.
    TaskNotFound,
    /// Domain revision rules rejected the proposed change.
    InvalidRevision(GoalContractRevisionFailure),
    /// Persistence rejected the read or compare-and-append operation.
    Store(GoalContractStoreFailure),
}

impl fmt::Display for ReviseGoalContractFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TaskNotFound => formatter.write_str("Goal Contract task was not found"),
            Self::InvalidRevision(error) => {
                write!(formatter, "Goal Contract revision failed: {error}")
            }
            Self::Store(error) => write!(formatter, "Goal Contract persistence failed: {error}"),
        }
    }
}

impl Error for ReviseGoalContractFailure {}

#[cfg(test)]
mod tests {
    use super::{
        CreateGoalContract, CreateGoalContractFailure, GoalContractStore, GoalContractStoreFailure,
        GoalContractStoreFuture, ReviseGoalContract, ReviseGoalContractFailure,
    };
    use a3_domain::{
        AcceptanceCriterion, AcceptanceCriterionId, AcceptanceCriterionStatement,
        CanonicalDirectory, CriterionEvidenceRequirement, CriterionOrigin, GitHead,
        GitReferenceName, GoalContract, GoalContractDraft, GoalContractRevision,
        GoalContractRevisionFailure, GoalContractTimestamp, GoalObjective,
        GoalOriginalBindingError, GoalOriginalRequest, GoalRevisionReason, ProjectIdentity,
        RemoteIdentity, RepositoryId, RepositoryIdentity, SuccessVerification, TaskId,
        WorktreeAnchorId, WorktreeId, WorktreeIdentity,
    };
    use futures::executor::block_on;
    use std::error::Error;
    use std::sync::Mutex;

    #[test]
    fn task_creation_persists_a_run_safe_initial_reference() -> Result<(), Box<dyn Error>> {
        let contract = initial_contract()?;
        let store = RecordingStore::default();
        let reference = block_on(CreateGoalContract::new(&store).execute(&project()?, &contract))?;

        assert_eq!(reference, contract.reference());
        assert_eq!(
            store
                .stored
                .lock()
                .map_err(|_| TestError("store lock was poisoned"))?
                .as_ref(),
            Some(&contract)
        );
        Ok(())
    }

    #[test]
    fn creation_rejects_orphan_criterion_origins_before_the_store_is_called()
    -> Result<(), Box<dyn Error>> {
        let source = source_contract()?;
        let orphan = GoalContract::initial(
            source.task_id(),
            source.draft().clone(),
            source.created_at(),
        );
        let store = RecordingStore::default();
        assert_eq!(
            block_on(CreateGoalContract::new(&store).execute(&project()?, &orphan)),
            Err(CreateGoalContractFailure::InvalidOriginalBinding(
                GoalOriginalBindingError::MissingOriginal
            ))
        );
        assert!(
            store
                .stored
                .lock()
                .map_err(|_| TestError("store lock was poisoned"))?
                .is_none()
        );
        Ok(())
    }

    #[test]
    fn creation_preserves_an_explicit_exact_original_and_criterion_binding()
    -> Result<(), Box<dyn Error>> {
        let source = source_contract()?;
        let store = RecordingStore::default();
        let reference = block_on(CreateGoalContract::new(&store).execute(&project()?, &source))?;
        assert_eq!(reference, source.reference());
        let stored = store
            .stored
            .lock()
            .map_err(|_| TestError("store lock was poisoned"))?;
        assert_eq!(stored.as_ref(), Some(&source));
        assert_eq!(
            stored
                .as_ref()
                .and_then(GoalContract::original)
                .map(GoalOriginalRequest::as_bytes),
            Some(b"R01: retain this exact original\r\n".as_slice())
        );
        Ok(())
    }

    #[test]
    fn legacy_revision_use_case_cannot_drop_a_source_before_append() -> Result<(), Box<dyn Error>> {
        let source = source_contract()?;
        let store = RecordingStore {
            stored: Mutex::new(Some(source.clone())),
        };
        let result = block_on(ReviseGoalContract::new(&store).execute(
            &project()?,
            source.task_id(),
            revised_draft()?,
            GoalRevisionReason::try_from_string("claimed change".to_owned())?,
            GoalContractTimestamp::from_unix_millis(2)?,
        ));
        assert_eq!(
            result,
            Err(ReviseGoalContractFailure::InvalidRevision(
                GoalContractRevisionFailure::OriginalRequired
            ))
        );
        assert_eq!(
            store
                .stored
                .lock()
                .map_err(|_| TestError("store lock was poisoned"))?
                .as_ref(),
            Some(&source)
        );
        Ok(())
    }

    #[test]
    fn revision_requires_an_existing_task_and_a_material_change() -> Result<(), Box<dyn Error>> {
        let project = project()?;
        let initial = initial_contract()?;
        let missing = RecordingStore::default();
        let missing_result = block_on(ReviseGoalContract::new(&missing).execute(
            &project,
            initial.task_id(),
            revised_draft()?,
            GoalRevisionReason::try_from_string("changed".to_owned())?,
            GoalContractTimestamp::from_unix_millis(2)?,
        ));
        assert_eq!(missing_result, Err(ReviseGoalContractFailure::TaskNotFound));

        let store = RecordingStore {
            stored: Mutex::new(Some(initial.clone())),
        };
        let unchanged = block_on(ReviseGoalContract::new(&store).execute(
            &project,
            initial.task_id(),
            initial.draft().clone(),
            GoalRevisionReason::try_from_string("claimed change".to_owned())?,
            GoalContractTimestamp::from_unix_millis(2)?,
        ));
        assert!(matches!(
            unchanged,
            Err(ReviseGoalContractFailure::InvalidRevision(_))
        ));

        let revised = block_on(ReviseGoalContract::new(&store).execute(
            &project,
            initial.task_id(),
            revised_draft()?,
            GoalRevisionReason::try_from_string("user changed the goal".to_owned())?,
            GoalContractTimestamp::from_unix_millis(2)?,
        ))?;
        assert_eq!(revised.revision().get(), 2);
        assert_eq!(revised.previous_revision(), Some(initial.revision()));
        assert_eq!(
            store
                .stored
                .lock()
                .map_err(|_| TestError("store lock was poisoned"))?
                .as_ref(),
            Some(&revised)
        );
        Ok(())
    }

    #[derive(Debug, Default)]
    struct RecordingStore {
        stored: Mutex<Option<GoalContract>>,
    }

    impl GoalContractStore for RecordingStore {
        fn create_goal_contract<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
            contract: &'a GoalContract,
        ) -> GoalContractStoreFuture<'a, ()> {
            Box::pin(async move {
                let mut stored = self
                    .stored
                    .lock()
                    .map_err(|_| GoalContractStoreFailure::Unavailable)?;
                if stored.is_some() {
                    return Err(GoalContractStoreFailure::TaskAlreadyExists);
                }
                *stored = Some(contract.clone());
                Ok(())
            })
        }

        fn append_goal_contract_revision<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
            contract: &'a GoalContract,
        ) -> GoalContractStoreFuture<'a, ()> {
            Box::pin(async move {
                let mut stored = self
                    .stored
                    .lock()
                    .map_err(|_| GoalContractStoreFailure::Unavailable)?;
                let current = stored
                    .as_ref()
                    .ok_or(GoalContractStoreFailure::TaskNotFound)?;
                if contract.previous_revision() != Some(current.revision()) {
                    return Err(GoalContractStoreFailure::RevisionConflict);
                }
                *stored = Some(contract.clone());
                Ok(())
            })
        }

        fn load_current_goal_contract<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
            task_id: TaskId,
        ) -> GoalContractStoreFuture<'a, Option<GoalContract>> {
            Box::pin(async move {
                Ok(self
                    .stored
                    .lock()
                    .map_err(|_| GoalContractStoreFailure::Unavailable)?
                    .as_ref()
                    .filter(|contract| contract.task_id() == task_id)
                    .cloned())
            })
        }

        fn load_goal_contract_revision<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
            task_id: TaskId,
            revision: GoalContractRevision,
        ) -> GoalContractStoreFuture<'a, Option<GoalContract>> {
            Box::pin(async move {
                Ok(self
                    .stored
                    .lock()
                    .map_err(|_| GoalContractStoreFailure::Unavailable)?
                    .as_ref()
                    .filter(|contract| {
                        contract.task_id() == task_id && contract.revision() == revision
                    })
                    .cloned())
            })
        }
    }

    #[derive(Debug)]
    struct TestError(&'static str);

    impl std::fmt::Display for TestError {
        fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str(self.0)
        }
    }

    impl Error for TestError {}

    fn initial_contract() -> Result<GoalContract, Box<dyn Error>> {
        Ok(GoalContract::initial(
            TaskId::from_bytes([1; 32]),
            draft("initial goal")?,
            GoalContractTimestamp::from_unix_millis(1)?,
        ))
    }

    fn source_contract() -> Result<GoalContract, Box<dyn Error>> {
        let base = initial_contract()?;
        let original = GoalOriginalRequest::try_from_bytes(
            base.reference(),
            b"R01: retain this exact original\r\n".to_vec(),
        )?;
        let criterion = base
            .draft()
            .acceptance_criteria()
            .first()
            .ok_or(TestError("missing test criterion"))?
            .clone()
            .with_origin(CriterionOrigin::new(
                original.range(0, original.as_bytes().len())?,
                CriterionEvidenceRequirement::RuntimeNeedsRecipe,
            ));
        let draft = GoalContractDraft::new(
            base.draft().objective().clone(),
            vec![criterion],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            base.draft().success_verification().clone(),
        )?;
        Ok(
            GoalContract::initial(base.task_id(), draft, base.created_at())
                .with_original(original)?,
        )
    }

    fn revised_draft() -> Result<GoalContractDraft, Box<dyn Error>> {
        draft("revised goal")
    }

    fn draft(objective: &str) -> Result<GoalContractDraft, Box<dyn Error>> {
        Ok(GoalContractDraft::new(
            GoalObjective::try_from_string(objective.to_owned())?,
            vec![AcceptanceCriterion::new(
                AcceptanceCriterionId::from_bytes([2; 32]),
                AcceptanceCriterionStatement::try_from_string("tests pass".to_owned())?,
            )],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            SuccessVerification::try_from_string("run tests".to_owned())?,
        )?)
    }

    fn project() -> Result<ProjectIdentity, Box<dyn Error>> {
        let root = std::env::current_dir()?.canonicalize()?;
        let repository_id = RepositoryId::from_bytes([3; 32]);
        let repository = RepositoryIdentity::new(
            repository_id,
            CanonicalDirectory::from_canonicalized(root.clone())?,
            Some(RemoteIdentity::from_bytes([4; 32])),
        );
        let worktree = WorktreeIdentity::new(
            WorktreeId::from_bytes([5; 32]),
            WorktreeAnchorId::from_bytes([6; 32]),
            repository_id,
            CanonicalDirectory::from_canonicalized(root)?,
        );
        Ok(ProjectIdentity::new(
            repository,
            worktree,
            GitHead::Unborn {
                reference: GitReferenceName::try_from_full_name("refs/heads/main".to_owned())?,
            },
        )?)
    }
}
