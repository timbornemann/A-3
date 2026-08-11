use crate::{
    CreateGoalContract, CreateGoalContractFailure, GoalContractStore, GoalContractStoreFailure,
};
use a3_domain::{
    AcceptanceCriterion, AcceptanceCriterionId, AcceptanceCriterionRequirement,
    AcceptanceCriterionStatement, GoalConstraint, GoalContract, GoalContractDraft,
    GoalContractDraftError, GoalContractRevision, GoalContractRevisionFailure,
    GoalContractTimestamp, GoalObjective, GoalRevisionReason, NonGoal, ProjectIdentity,
    SuccessVerification, TaskId, UserDecision,
};
use std::collections::BTreeSet;
use std::error::Error;
use std::fmt;
use std::sync::Arc;

/// One generated opaque 32-byte identity before it receives a domain-specific type.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AgentGoalGeneratedIdentity([u8; 32]);

impl AgentGoalGeneratedIdentity {
    /// Wraps bytes supplied by an injected operating-system identity source.
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    const fn into_bytes(self) -> [u8; 32] {
        self.0
    }
}

/// Stable failure classes for Core-owned Goal Contract identity and wall-clock metadata.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AgentGoalMetadataFailure {
    /// The operating-system identity source could not produce a value.
    IdentityUnavailable,
    /// The wall clock could not be represented by the durable Goal Contract timestamp.
    ClockUnavailable,
}

impl fmt::Display for AgentGoalMetadataFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::IdentityUnavailable => "Goal Contract identity source is unavailable",
            Self::ClockUnavailable => "Goal Contract clock is unavailable",
        })
    }
}

impl Error for AgentGoalMetadataFailure {}

/// Injected Core-only source for task identities, criterion identities, and durable wall time.
pub trait AgentGoalMetadataSource: fmt::Debug + Send + Sync {
    /// Generates a fresh opaque identity that never crosses IPC before persistence.
    fn next_identity(&self) -> Result<AgentGoalGeneratedIdentity, AgentGoalMetadataFailure>;

    /// Returns the current durable Goal Contract wall-clock timestamp.
    fn now(&self) -> Result<GoalContractTimestamp, AgentGoalMetadataFailure>;
}

/// One criterion proposed by the user, optionally retaining a current durable identity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AgentGoalCriterionDraft {
    criterion_id: Option<AcceptanceCriterionId>,
    statement: AcceptanceCriterionStatement,
    requirement: AcceptanceCriterionRequirement,
}

impl AgentGoalCriterionDraft {
    /// Creates a bounded criterion input. `None` requests a Core-generated identity.
    #[must_use]
    pub const fn new(
        criterion_id: Option<AcceptanceCriterionId>,
        statement: AcceptanceCriterionStatement,
        requirement: AcceptanceCriterionRequirement,
    ) -> Self {
        Self {
            criterion_id,
            statement,
            requirement,
        }
    }

    /// Returns the retained current identity, or `None` when Core must allocate one.
    #[must_use]
    pub const fn criterion_id(&self) -> Option<AcceptanceCriterionId> {
        self.criterion_id
    }

    /// Returns the bounded criterion statement.
    #[must_use]
    pub const fn statement(&self) -> &AcceptanceCriterionStatement {
        &self.statement
    }

    /// Returns whether the criterion gates task completion.
    #[must_use]
    pub const fn requirement(&self) -> AcceptanceCriterionRequirement {
        self.requirement
    }
}

/// Complete validated textual content proposed for one Agent-workspace Goal Contract revision.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AgentGoalDraft {
    objective: GoalObjective,
    acceptance_criteria: Vec<AgentGoalCriterionDraft>,
    constraints: Vec<GoalConstraint>,
    non_goals: Vec<NonGoal>,
    user_decisions: Vec<UserDecision>,
    success_verification: SuccessVerification,
}

impl AgentGoalDraft {
    /// Collects already bounded field values; aggregate cardinality is checked during execution.
    #[must_use]
    pub fn new(
        objective: GoalObjective,
        acceptance_criteria: Vec<AgentGoalCriterionDraft>,
        constraints: Vec<GoalConstraint>,
        non_goals: Vec<NonGoal>,
        user_decisions: Vec<UserDecision>,
        success_verification: SuccessVerification,
    ) -> Self {
        Self {
            objective,
            acceptance_criteria,
            constraints,
            non_goals,
            user_decisions,
            success_verification,
        }
    }

    /// Returns the proposed acceptance criteria in user-defined order.
    #[must_use]
    pub fn acceptance_criteria(&self) -> &[AgentGoalCriterionDraft] {
        &self.acceptance_criteria
    }
}

/// Creates a durable task and initial Goal Contract from Core-owned metadata.
#[derive(Debug, Clone)]
pub struct CreateAgentGoal {
    store: Arc<dyn GoalContractStore>,
    metadata: Arc<dyn AgentGoalMetadataSource>,
}

impl CreateAgentGoal {
    /// Creates the use case from its persistence and operating-system metadata boundaries.
    #[must_use]
    pub const fn new(
        store: Arc<dyn GoalContractStore>,
        metadata: Arc<dyn AgentGoalMetadataSource>,
    ) -> Self {
        Self { store, metadata }
    }

    /// Generates all identities and persists revision one atomically with its task.
    pub async fn execute(
        &self,
        project: &ProjectIdentity,
        draft: AgentGoalDraft,
    ) -> Result<GoalContract, CreateAgentGoalFailure> {
        if draft
            .acceptance_criteria
            .iter()
            .any(|criterion| criterion.criterion_id.is_some())
        {
            return Err(CreateAgentGoalFailure::ExistingCriterionIdentity);
        }
        let task_id = TaskId::from_bytes(
            self.metadata
                .next_identity()
                .map_err(CreateAgentGoalFailure::Metadata)?
                .into_bytes(),
        );
        let draft = materialize_draft(draft, None, self.metadata.as_ref())
            .map_err(CreateAgentGoalFailure::Draft)?;
        let created_at = self
            .metadata
            .now()
            .map_err(CreateAgentGoalFailure::Metadata)?;
        let contract = GoalContract::initial(task_id, draft, created_at);
        CreateGoalContract::new(self.store.as_ref())
            .execute(project, &contract)
            .await
            .map_err(CreateAgentGoalFailure::Create)?;
        Ok(contract)
    }
}

/// Initial Goal Contract creation failed safely before any run could begin.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum CreateAgentGoalFailure {
    /// Initial forms may not claim an already durable criterion identity.
    ExistingCriterionIdentity,
    /// Core-owned identity or time metadata was unavailable.
    Metadata(AgentGoalMetadataFailure),
    /// Criterion identities or aggregate content violated the Goal Contract contract.
    Draft(AgentGoalDraftFailure),
    /// Atomic task-and-goal persistence failed.
    Create(CreateGoalContractFailure),
}

impl fmt::Display for CreateAgentGoalFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ExistingCriterionIdentity => formatter
                .write_str("initial Goal Contract criteria cannot supply durable identities"),
            Self::Metadata(error) => write!(formatter, "Goal Contract metadata failed: {error}"),
            Self::Draft(error) => write!(formatter, "Goal Contract draft failed: {error}"),
            Self::Create(error) => write!(formatter, "Goal Contract creation failed: {error}"),
        }
    }
}

impl Error for CreateAgentGoalFailure {}

/// Appends one material successor revision without allowing stale editors to overwrite changes.
#[derive(Debug, Clone)]
pub struct ReviseAgentGoal {
    store: Arc<dyn GoalContractStore>,
    metadata: Arc<dyn AgentGoalMetadataSource>,
}

impl ReviseAgentGoal {
    /// Creates the use case from its persistence and operating-system metadata boundaries.
    #[must_use]
    pub const fn new(
        store: Arc<dyn GoalContractStore>,
        metadata: Arc<dyn AgentGoalMetadataSource>,
    ) -> Self {
        Self { store, metadata }
    }

    /// Revalidates current criterion identities and compare-and-appends the immediate successor.
    pub async fn execute(
        &self,
        project: &ProjectIdentity,
        task_id: TaskId,
        expected_revision: GoalContractRevision,
        draft: AgentGoalDraft,
        reason: GoalRevisionReason,
    ) -> Result<GoalContract, ReviseAgentGoalFailure> {
        let current = self
            .store
            .load_current_goal_contract(project, task_id)
            .await
            .map_err(ReviseAgentGoalFailure::Store)?
            .ok_or(ReviseAgentGoalFailure::TaskNotFound)?;
        if current.revision() != expected_revision {
            return Err(ReviseAgentGoalFailure::RevisionConflict);
        }
        let current_ids = current
            .draft()
            .acceptance_criteria()
            .iter()
            .map(AcceptanceCriterion::id)
            .collect::<BTreeSet<_>>();
        let draft = materialize_draft(draft, Some(&current_ids), self.metadata.as_ref())
            .map_err(ReviseAgentGoalFailure::Draft)?;
        let created_at = self
            .metadata
            .now()
            .map_err(ReviseAgentGoalFailure::Metadata)?;
        let revised = current
            .revise(draft, reason, created_at)
            .map_err(ReviseAgentGoalFailure::InvalidRevision)?;
        self.store
            .append_goal_contract_revision(project, &revised)
            .await
            .map_err(ReviseAgentGoalFailure::Store)?;
        Ok(revised)
    }
}

/// Goal Contract revision failed without overwriting the current durable revision.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum ReviseAgentGoalFailure {
    /// The task is absent from the active worktree.
    TaskNotFound,
    /// The editor or persistence writer no longer targets the current revision.
    RevisionConflict,
    /// Core-owned identity or time metadata was unavailable.
    Metadata(AgentGoalMetadataFailure),
    /// Criterion identities or aggregate content violated the Goal Contract contract.
    Draft(AgentGoalDraftFailure),
    /// The proposal was not a valid material immediate successor.
    InvalidRevision(GoalContractRevisionFailure),
    /// Durable loading or compare-and-append failed.
    Store(GoalContractStoreFailure),
}

impl fmt::Display for ReviseAgentGoalFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TaskNotFound => formatter.write_str("Goal Contract task was not found"),
            Self::RevisionConflict => formatter.write_str("Goal Contract editor revision is stale"),
            Self::Metadata(error) => write!(formatter, "Goal Contract metadata failed: {error}"),
            Self::Draft(error) => write!(formatter, "Goal Contract draft failed: {error}"),
            Self::InvalidRevision(error) => {
                write!(formatter, "Goal Contract revision failed: {error}")
            }
            Self::Store(error) => write!(formatter, "Goal Contract persistence failed: {error}"),
        }
    }
}

impl Error for ReviseAgentGoalFailure {}

/// Reads the current immutable Goal Contract without exposing persistence details.
#[derive(Debug, Clone)]
pub struct GetAgentGoal {
    store: Arc<dyn GoalContractStore>,
}

impl GetAgentGoal {
    /// Creates the read use case from its narrow store.
    #[must_use]
    pub const fn new(store: Arc<dyn GoalContractStore>) -> Self {
        Self { store }
    }

    /// Loads the current revision for one task in the active worktree.
    pub async fn execute(
        &self,
        project: &ProjectIdentity,
        task_id: TaskId,
    ) -> Result<Option<GoalContract>, GoalContractStoreFailure> {
        self.store
            .load_current_goal_contract(project, task_id)
            .await
    }
}

/// Invalid generated identity or final Goal Contract aggregate content.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum AgentGoalDraftFailure {
    /// A revised form claimed a criterion identity absent from the current revision.
    UnknownCriterionIdentity,
    /// The injected identity source repeated an identity within the same revision.
    IdentityCollision,
    /// The final domain aggregate rejected collection count or uniqueness.
    InvalidAggregate(GoalContractDraftError),
    /// Core-owned identity generation failed.
    Metadata(AgentGoalMetadataFailure),
}

impl fmt::Display for AgentGoalDraftFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownCriterionIdentity => {
                formatter.write_str("Goal Contract criterion identity is not current")
            }
            Self::IdentityCollision => {
                formatter.write_str("Goal Contract identity source repeated an identity")
            }
            Self::InvalidAggregate(error) => write!(formatter, "{error}"),
            Self::Metadata(error) => write!(formatter, "{error}"),
        }
    }
}

impl Error for AgentGoalDraftFailure {}

fn materialize_draft(
    draft: AgentGoalDraft,
    current_ids: Option<&BTreeSet<AcceptanceCriterionId>>,
    metadata: &dyn AgentGoalMetadataSource,
) -> Result<GoalContractDraft, AgentGoalDraftFailure> {
    let mut used_ids = BTreeSet::new();
    let mut criteria = Vec::with_capacity(draft.acceptance_criteria.len());
    for criterion in draft.acceptance_criteria {
        let id = match criterion.criterion_id {
            Some(id) => {
                if current_ids.is_none_or(|ids| !ids.contains(&id)) {
                    return Err(AgentGoalDraftFailure::UnknownCriterionIdentity);
                }
                id
            }
            None => AcceptanceCriterionId::from_bytes(
                metadata
                    .next_identity()
                    .map_err(AgentGoalDraftFailure::Metadata)?
                    .into_bytes(),
            ),
        };
        if !used_ids.insert(id) {
            return Err(AgentGoalDraftFailure::IdentityCollision);
        }
        criteria.push(AcceptanceCriterion::with_requirement(
            id,
            criterion.statement,
            criterion.requirement,
        ));
    }
    GoalContractDraft::new(
        draft.objective,
        criteria,
        draft.constraints,
        draft.non_goals,
        draft.user_decisions,
        draft.success_verification,
    )
    .map_err(AgentGoalDraftFailure::InvalidAggregate)
}

#[cfg(test)]
mod tests {
    use super::{
        AgentGoalCriterionDraft, AgentGoalDraft, AgentGoalGeneratedIdentity,
        AgentGoalMetadataFailure, AgentGoalMetadataSource, CreateAgentGoal, CreateAgentGoalFailure,
        ReviseAgentGoal, ReviseAgentGoalFailure,
    };
    use crate::{GoalContractStore, GoalContractStoreFailure, GoalContractStoreFuture};
    use a3_domain::{
        AcceptanceCriterionRequirement, AcceptanceCriterionStatement, CanonicalDirectory, GitHead,
        GitReferenceName, GoalConstraint, GoalContract, GoalContractRevision,
        GoalContractTimestamp, GoalObjective, GoalRevisionReason, NonGoal, ProjectIdentity,
        RepositoryId, RepositoryIdentity, SuccessVerification, TaskId, UserDecision,
        WorktreeAnchorId, WorktreeId, WorktreeIdentity,
    };
    use futures::executor::block_on;
    use std::error::Error;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicU8, Ordering};

    #[derive(Debug)]
    struct Metadata {
        next: AtomicU8,
        now: u64,
    }

    impl AgentGoalMetadataSource for Metadata {
        fn next_identity(&self) -> Result<AgentGoalGeneratedIdentity, AgentGoalMetadataFailure> {
            let value = self.next.fetch_add(1, Ordering::Relaxed);
            Ok(AgentGoalGeneratedIdentity::from_bytes([value; 32]))
        }

        fn now(&self) -> Result<GoalContractTimestamp, AgentGoalMetadataFailure> {
            GoalContractTimestamp::from_unix_millis(self.now)
                .map_err(|_| AgentGoalMetadataFailure::ClockUnavailable)
        }
    }

    #[derive(Debug, Default)]
    struct Store {
        current: Mutex<Option<GoalContract>>,
    }

    impl GoalContractStore for Store {
        fn create_goal_contract<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
            contract: &'a GoalContract,
        ) -> GoalContractStoreFuture<'a, ()> {
            Box::pin(async move {
                let mut current = self
                    .current
                    .lock()
                    .map_err(|_| GoalContractStoreFailure::Unavailable)?;
                if current.is_some() {
                    return Err(GoalContractStoreFailure::TaskAlreadyExists);
                }
                *current = Some(contract.clone());
                Ok(())
            })
        }

        fn append_goal_contract_revision<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
            contract: &'a GoalContract,
        ) -> GoalContractStoreFuture<'a, ()> {
            Box::pin(async move {
                let mut current = self
                    .current
                    .lock()
                    .map_err(|_| GoalContractStoreFailure::Unavailable)?;
                let stored = current
                    .as_ref()
                    .ok_or(GoalContractStoreFailure::TaskNotFound)?;
                if contract.previous_revision() != Some(stored.revision()) {
                    return Err(GoalContractStoreFailure::RevisionConflict);
                }
                *current = Some(contract.clone());
                Ok(())
            })
        }

        fn load_current_goal_contract<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
            _task_id: TaskId,
        ) -> GoalContractStoreFuture<'a, Option<GoalContract>> {
            Box::pin(async move {
                self.current
                    .lock()
                    .map(|current| current.clone())
                    .map_err(|_| GoalContractStoreFailure::Unavailable)
            })
        }

        fn load_goal_contract_revision<'a>(
            &'a self,
            project: &'a ProjectIdentity,
            task_id: TaskId,
            _revision: GoalContractRevision,
        ) -> GoalContractStoreFuture<'a, Option<GoalContract>> {
            self.load_current_goal_contract(project, task_id)
        }
    }

    #[test]
    fn create_generates_core_owned_ids_and_revision_one() -> Result<(), Box<dyn Error>> {
        let store = std::sync::Arc::new(Store::default());
        let metadata = std::sync::Arc::new(Metadata {
            next: AtomicU8::new(1),
            now: 10,
        });
        let created = block_on(
            CreateAgentGoal::new(store, metadata)
                .execute(&project()?, draft(None, "initial goal")?),
        )?;

        assert_eq!(created.task_id(), TaskId::from_bytes([1; 32]));
        assert_eq!(
            created.draft().acceptance_criteria()[0].id(),
            a3_domain::AcceptanceCriterionId::from_bytes([2; 32])
        );
        assert_eq!(created.revision(), GoalContractRevision::INITIAL);
        Ok(())
    }

    #[test]
    fn revise_retains_known_ids_and_rejects_stale_or_invented_ones() -> Result<(), Box<dyn Error>> {
        let store = std::sync::Arc::new(Store::default());
        let create_metadata = std::sync::Arc::new(Metadata {
            next: AtomicU8::new(10),
            now: 10,
        });
        let current = block_on(
            CreateAgentGoal::new(store.clone(), create_metadata)
                .execute(&project()?, draft(None, "initial goal")?),
        )?;
        let criterion_id = current.draft().acceptance_criteria()[0].id();
        let revise_metadata = std::sync::Arc::new(Metadata {
            next: AtomicU8::new(20),
            now: 11,
        });
        let revised = block_on(
            ReviseAgentGoal::new(store.clone(), revise_metadata.clone()).execute(
                &project()?,
                current.task_id(),
                current.revision(),
                draft(Some(criterion_id), "revised goal")?,
                GoalRevisionReason::try_from_string("user clarified scope".to_owned())?,
            ),
        )?;
        assert_eq!(revised.draft().acceptance_criteria()[0].id(), criterion_id);
        assert_eq!(revised.revision().get(), 2);

        let stale = block_on(
            ReviseAgentGoal::new(store.clone(), revise_metadata.clone()).execute(
                &project()?,
                revised.task_id(),
                GoalContractRevision::INITIAL,
                draft(Some(criterion_id), "another revision")?,
                GoalRevisionReason::try_from_string("stale editor".to_owned())?,
            ),
        );
        assert_eq!(stale, Err(ReviseAgentGoalFailure::RevisionConflict));

        let invented = block_on(ReviseAgentGoal::new(store, revise_metadata).execute(
            &project()?,
            revised.task_id(),
            revised.revision(),
            draft(
                Some(a3_domain::AcceptanceCriterionId::from_bytes([99; 32])),
                "invented criterion",
            )?,
            GoalRevisionReason::try_from_string("invented identity".to_owned())?,
        ));
        assert!(matches!(
            invented,
            Err(ReviseAgentGoalFailure::Draft(
                super::AgentGoalDraftFailure::UnknownCriterionIdentity
            ))
        ));
        Ok(())
    }

    #[test]
    fn create_rejects_webview_supplied_durable_criterion_ids() -> Result<(), Box<dyn Error>> {
        let result = block_on(
            CreateAgentGoal::new(
                std::sync::Arc::new(Store::default()),
                std::sync::Arc::new(Metadata {
                    next: AtomicU8::new(1),
                    now: 10,
                }),
            )
            .execute(
                &project()?,
                draft(
                    Some(a3_domain::AcceptanceCriterionId::from_bytes([3; 32])),
                    "goal",
                )?,
            ),
        );
        assert_eq!(
            result,
            Err(CreateAgentGoalFailure::ExistingCriterionIdentity)
        );
        Ok(())
    }

    fn draft(
        criterion_id: Option<a3_domain::AcceptanceCriterionId>,
        objective: &str,
    ) -> Result<AgentGoalDraft, Box<dyn Error>> {
        Ok(AgentGoalDraft::new(
            GoalObjective::try_from_string(objective.to_owned())?,
            vec![AgentGoalCriterionDraft::new(
                criterion_id,
                AcceptanceCriterionStatement::try_from_string("tests pass".to_owned())?,
                AcceptanceCriterionRequirement::Must,
            )],
            vec![GoalConstraint::try_from_string("remain local".to_owned())?],
            vec![NonGoal::try_from_string("no network".to_owned())?],
            vec![UserDecision::try_from_string(
                "use durable state".to_owned(),
            )?],
            SuccessVerification::try_from_string("run the tests".to_owned())?,
        ))
    }

    fn project() -> Result<ProjectIdentity, Box<dyn Error>> {
        let root = std::env::current_dir()?.canonicalize()?;
        let repository_id = RepositoryId::from_bytes([7; 32]);
        Ok(ProjectIdentity::new(
            RepositoryIdentity::new(
                repository_id,
                CanonicalDirectory::from_canonicalized(root.clone())?,
                None,
            ),
            WorktreeIdentity::new(
                WorktreeId::from_bytes([8; 32]),
                WorktreeAnchorId::from_bytes([9; 32]),
                repository_id,
                CanonicalDirectory::from_canonicalized(root)?,
            ),
            GitHead::Unborn {
                reference: GitReferenceName::try_from_full_name("refs/heads/main".to_owned())?,
            },
        )?)
    }
}
