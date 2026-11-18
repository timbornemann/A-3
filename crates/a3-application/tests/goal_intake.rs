//! Use-case boundary controls only. Real transaction/reopen proofs belong to the adapter suite.
use a3_application::{
    CaptureGoalIntake, CaptureGoalIntakeFailure, GoalContractStore, GoalContractStoreFailure,
    GoalContractStoreFuture, GoalIntakeLoadResult, GoalIntakeStoreVersion, PromoteGoalIntake,
    PromoteGoalIntakeFailure, StoredPendingGoalIntake,
};
use a3_domain::{
    AcceptanceCriterion, AcceptanceCriterionId, AcceptanceCriterionStatement, CanonicalDirectory,
    CriterionEvidenceRequirement, CriterionOrigin, DiscoveredCommandId, GitHead, GitReferenceName,
    GoalConstraint, GoalContract, GoalContractDraft, GoalContractReference, GoalContractRevision,
    GoalContractTimestamp, GoalIntakeError, GoalObjective, GoalOriginalRequest,
    MinimumTestCaseCount, PendingGoalIntake, ProjectIdentity, RepositoryId, RepositoryIdentity,
    SuccessVerification, TaskId, TestCaseSelector, VerificationScope, WorktreeAnchorId, WorktreeId,
    WorktreeIdentity,
};
use futures::executor::block_on;
use std::error::Error;
use std::sync::{Mutex, MutexGuard};

type TestResult<T = ()> = Result<T, Box<dyn Error>>;

#[test]
fn pending_discovery_pages_bound_lookahead_and_reject_unordered_or_replayed_ids() -> TestResult {
    use a3_application::{PENDING_GOAL_INTAKE_PAGE_SIZE, PendingGoalIntakePage};
    let ids: Vec<_> = (1..=33).map(|id| TaskId::from_bytes([id; 32])).collect();
    let page = PendingGoalIntakePage::from_lookahead(None, ids.clone())?;
    assert_eq!(page.task_ids(), &ids[..PENDING_GOAL_INTAKE_PAGE_SIZE]);
    assert_eq!(page.next_after(), Some(ids[31]));
    let last = PendingGoalIntakePage::from_lookahead(page.next_after(), vec![ids[32]])?;
    assert_eq!(last.task_ids(), &ids[32..]);
    assert_eq!(last.next_after(), None);
    assert!(PendingGoalIntakePage::from_lookahead(None, vec![ids[0]; 2]).is_err());
    assert!(PendingGoalIntakePage::from_lookahead(None, vec![ids[1], ids[0]]).is_err());
    assert!(PendingGoalIntakePage::from_lookahead(Some(ids[0]), vec![ids[0]]).is_err());
    assert!(PendingGoalIntakePage::from_lookahead(None, vec![ids[0]; 34]).is_err());
    let empty = PendingGoalIntakePage::from_lookahead(None, Vec::new())?;
    assert!(empty.task_ids().is_empty());
    assert_eq!(empty.next_after(), None);
    Ok(())
}

const TEXT: &str = "R: implement behavior\r\nPreserve protected files\r\n";

#[derive(Debug, Default)]
struct State {
    loaded: Option<GoalIntakeLoadResult>,
    materialized: Option<GoalContract>,
    captures: usize,
    reads: usize,
    promotions: usize,
    review: Option<a3_domain::GoalIntakeReview>,
    review_reads: usize,
    review_commits: usize,
    review_failure: Option<GoalContractStoreFailure>,
    read_failure: Option<GoalContractStoreFailure>,
    promotion_failure: Option<GoalContractStoreFailure>,
}
#[derive(Debug, Default)]
struct RecordingStore(Mutex<State>);
impl RecordingStore {
    fn state(&self) -> TestResult<MutexGuard<'_, State>> {
        self.0.lock().map_err(|_| "test store poisoned".into())
    }
    fn pending(value: PendingGoalIntake) -> Self {
        Self(Mutex::new(State {
            loaded: Some(GoalIntakeLoadResult::Pending(StoredPendingGoalIntake::new(
                value,
                GoalIntakeStoreVersion::INITIAL,
            ))),
            ..State::default()
        }))
    }
}
macro_rules! baseline_methods {
    () => {
        fn create_goal_contract<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
            _contract: &'a GoalContract,
        ) -> GoalContractStoreFuture<'a, ()> {
            Box::pin(async { Err(GoalContractStoreFailure::UnsupportedSchema) })
        }
        fn append_goal_contract_revision<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
            _contract: &'a GoalContract,
        ) -> GoalContractStoreFuture<'a, ()> {
            Box::pin(async { Err(GoalContractStoreFailure::UnsupportedSchema) })
        }
        fn load_current_goal_contract<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
            _task: TaskId,
        ) -> GoalContractStoreFuture<'a, Option<GoalContract>> {
            Box::pin(async { Err(GoalContractStoreFailure::UnsupportedSchema) })
        }
        fn load_goal_contract_revision<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
            _task: TaskId,
            _revision: GoalContractRevision,
        ) -> GoalContractStoreFuture<'a, Option<GoalContract>> {
            Box::pin(async { Err(GoalContractStoreFailure::UnsupportedSchema) })
        }
    };
}
impl GoalContractStore for RecordingStore {
    baseline_methods!();
    fn load_goal_intake_review<'a>(
        &'a self,
        _: &'a ProjectIdentity,
        _: TaskId,
    ) -> GoalContractStoreFuture<'a, Option<a3_domain::GoalIntakeReview>> {
        Box::pin(async move {
            let mut s = self
                .0
                .lock()
                .map_err(|_| GoalContractStoreFailure::Unavailable)?;
            s.review_reads += 1;
            if let Some(error) = s.review_failure {
                return Err(error);
            }
            Ok(s.review.clone())
        })
    }
    fn append_goal_intake_review<'a>(
        &'a self,
        _: &'a ProjectIdentity,
        task: TaskId,
        capture: GoalIntakeStoreVersion,
        expected: Option<a3_domain::GoalIntakeReviewRevision>,
        review: &'a a3_domain::GoalIntakeReview,
    ) -> GoalContractStoreFuture<'a, a3_domain::GoalIntakeReviewRevision> {
        Box::pin(async move {
            let mut s = self
                .0
                .lock()
                .map_err(|_| GoalContractStoreFailure::Unavailable)?;
            if let Some(error) = s.review_failure {
                return Err(error);
            }
            let Some(GoalIntakeLoadResult::Pending(stored)) = &s.loaded else {
                return Err(GoalContractStoreFailure::IntakeNotPending);
            };
            if stored.pending().task_id() != task || stored.version() != capture {
                return Err(GoalContractStoreFailure::IntakeVersionConflict);
            }
            if s.review.as_ref().map(a3_domain::GoalIntakeReview::revision) != expected {
                return Err(GoalContractStoreFailure::IntakeReviewVersionConflict);
            }
            review
                .validate_capture(stored.pending())
                .map_err(|_| GoalContractStoreFailure::InvalidIntakeReview)?;
            match &s.review {
                Some(previous) => review
                    .validate_successor_of(previous)
                    .map_err(|_| GoalContractStoreFailure::InvalidIntakeReview)?,
                None => {
                    if review.revision().get() != 1 {
                        return Err(GoalContractStoreFailure::InvalidIntakeReview);
                    }
                }
            }
            s.review = Some(review.clone());
            s.review_commits += 1;
            Ok(review.revision())
        })
    }
    fn capture_goal_intake<'a>(
        &'a self,
        _project: &'a ProjectIdentity,
        pending: &'a PendingGoalIntake,
    ) -> GoalContractStoreFuture<'a, GoalIntakeStoreVersion> {
        Box::pin(async move {
            let mut state = self
                .0
                .lock()
                .map_err(|_| GoalContractStoreFailure::Unavailable)?;
            state.captures += 1;
            if state.loaded.is_some() {
                return Err(GoalContractStoreFailure::TaskAlreadyExists);
            }
            state.loaded = Some(GoalIntakeLoadResult::Pending(StoredPendingGoalIntake::new(
                pending.clone(),
                GoalIntakeStoreVersion::INITIAL,
            )));
            Ok(GoalIntakeStoreVersion::INITIAL)
        })
    }
    fn load_goal_intake<'a>(
        &'a self,
        _project: &'a ProjectIdentity,
        _task: TaskId,
    ) -> GoalContractStoreFuture<'a, Option<GoalIntakeLoadResult>> {
        Box::pin(async move {
            let mut state = self
                .0
                .lock()
                .map_err(|_| GoalContractStoreFailure::Unavailable)?;
            state.reads += 1;
            match state.read_failure {
                Some(error) => Err(error),
                None => Ok(state.loaded.clone()),
            }
        })
    }
    fn promote_goal_intake<'a>(
        &'a self,
        _project: &'a ProjectIdentity,
        task: TaskId,
        expected: GoalIntakeStoreVersion,
        goal: &'a GoalContract,
    ) -> GoalContractStoreFuture<'a, ()> {
        Box::pin(async move {
            let mut state = self
                .0
                .lock()
                .map_err(|_| GoalContractStoreFailure::Unavailable)?;
            state.promotions += 1;
            if let Some(error) = state.promotion_failure {
                return Err(error);
            }
            let Some(GoalIntakeLoadResult::Pending(stored)) = &state.loaded else {
                return Err(GoalContractStoreFailure::IntakeNotPending);
            };
            if stored.pending().task_id() != task || stored.version() != expected {
                return Err(GoalContractStoreFailure::IntakeVersionConflict);
            }
            stored
                .pending()
                .validate_promotion(goal)
                .map_err(|_| GoalContractStoreFailure::InvalidIntakePromotion)?;
            if let Some(review) = &state.review {
                review
                    .validate_promotion(stored.pending(), goal)
                    .map_err(|_| GoalContractStoreFailure::InvalidIntakePromotion)?;
            }
            state.loaded = Some(GoalIntakeLoadResult::Materialized(goal.reference()));
            state.materialized = Some(goal.clone());
            Ok(())
        })
    }
}
#[derive(Debug)]
struct BaselineOnlyStore;
impl GoalContractStore for BaselineOnlyStore {
    baseline_methods!();
}

fn at(n: u64) -> TestResult<GoalContractTimestamp> {
    Ok(GoalContractTimestamp::from_unix_millis(n)?)
}
fn task(n: u8) -> TaskId {
    TaskId::from_bytes([n; 32])
}
fn pending(n: u8) -> TestResult<PendingGoalIntake> {
    Ok(PendingGoalIntake::new(
        GoalOriginalRequest::try_from_bytes(
            GoalContractReference::initial(task(n)),
            TEXT.as_bytes().to_vec(),
        )?,
        at(1)?,
    )?)
}
fn goal(n: u8, text: &str, time: u64, end: usize, unbound: bool) -> TestResult<GoalContract> {
    let original = GoalOriginalRequest::try_from_bytes(
        GoalContractReference::initial(task(n)),
        text.as_bytes().to_vec(),
    )?;
    let criterion = AcceptanceCriterion::new(
        AcceptanceCriterionId::from_bytes([9; 32]),
        AcceptanceCriterionStatement::try_from_string(
            "requested behavior is implemented".to_owned(),
        )?,
    )
    .with_origin(CriterionOrigin::new(
        original.range(0, end)?,
        CriterionEvidenceRequirement::ExistingTest {
            command: DiscoveredCommandId::from_bytes([10; 32]),
            selector: TestCaseSelector::All,
            minimum_cases: MinimumTestCaseCount::new(1)?,
            scope: VerificationScope::Targeted,
            python_integer_translation: None,
        },
    ));
    let constraints = if unbound {
        vec![GoalConstraint::try_from_string(
            "preserve protected files".to_owned(),
        )?]
    } else {
        Vec::new()
    };
    Ok(GoalContract::initial(
        task(n),
        GoalContractDraft::new(
            GoalObjective::try_from_string("Implement requested behavior".to_owned())?,
            vec![criterion],
            constraints,
            Vec::new(),
            Vec::new(),
            SuccessVerification::try_from_string("matching test passes".to_owned())?,
        )?,
        at(time)?,
    )
    .with_original(original)?)
}
fn valid_goal() -> TestResult<GoalContract> {
    goal(1, TEXT, 1, TEXT.len(), false)
}
fn project() -> TestResult<ProjectIdentity> {
    let root = std::env::current_dir()?.canonicalize()?;
    let repository = RepositoryId::from_bytes([40; 32]);
    Ok(ProjectIdentity::new(
        RepositoryIdentity::new(
            repository,
            CanonicalDirectory::from_canonicalized(root.clone())?,
            None,
        ),
        WorktreeIdentity::new(
            WorktreeId::from_bytes([41; 32]),
            WorktreeAnchorId::from_bytes([42; 32]),
            repository,
            CanonicalDirectory::from_canonicalized(root)?,
        ),
        GitHead::Unborn {
            reference: GitReferenceName::try_from_full_name("refs/heads/main".to_owned())?,
        },
    )?)
}

#[test]
fn invalid_raw_utf8_empty_and_oversize_make_no_store_calls() -> TestResult {
    for bytes in [Vec::new(), vec![0xf0, 0x9f], vec![b'x'; 65_537]] {
        let store = RecordingStore::default();
        assert!(matches!(
            block_on(CaptureGoalIntake::new(&store).execute(&project()?, task(1), bytes, at(1)?)),
            Err(CaptureGoalIntakeFailure::InvalidOriginal(_))
        ));
        let state = store.state()?;
        assert_eq!((state.captures, state.reads, state.promotions), (0, 0, 0));
        assert!(state.loaded.is_none());
    }
    Ok(())
}

#[test]
fn capture_exact_64k_without_a_draft_and_duplicate_create_never_retries() -> TestResult {
    let mut text = "\u{20ac}".repeat(65_536 / 3);
    text.push('x');
    let store = RecordingStore::default();
    let first = block_on(CaptureGoalIntake::new(&store).execute(
        &project()?,
        task(1),
        text.as_bytes().to_vec(),
        at(1)?,
    ))?;
    assert_eq!(first.version(), GoalIntakeStoreVersion::INITIAL);
    assert_eq!(first.pending().original().as_bytes(), text.as_bytes());
    assert_eq!(first.pending().original().parts().len(), 33);
    assert_eq!(
        block_on(CaptureGoalIntake::new(&store).execute(
            &project()?,
            task(1),
            text.as_bytes().to_vec(),
            at(1)?
        )),
        Err(CaptureGoalIntakeFailure::Store(
            GoalContractStoreFailure::TaskAlreadyExists
        ))
    );
    let state = store.state()?;
    assert_eq!((state.captures, state.reads, state.promotions), (2, 0, 0));
    assert_eq!(state.loaded, Some(GoalIntakeLoadResult::Pending(first)));
    assert!(state.materialized.is_none());
    Ok(())
}

#[test]
fn invalid_promotion_is_rejected_before_any_promotion_port_call() -> TestResult {
    let changed_raw = TEXT.replace("\r\n", "\n");
    let cases = [
        (
            goal(2, TEXT, 1, TEXT.len(), false)?,
            GoalIntakeError::TaskMismatch,
        ),
        (
            goal(1, TEXT, 2, TEXT.len(), false)?,
            GoalIntakeError::CreatedAtMismatch,
        ),
        (
            goal(1, &changed_raw, 1, changed_raw.len(), false)?,
            GoalIntakeError::OriginalMismatch,
        ),
        (
            goal(1, TEXT, 1, TEXT.find('\n').ok_or("newline")? + 1, false)?,
            GoalIntakeError::IntakeIncomplete,
        ),
        (
            goal(1, TEXT, 1, TEXT.len(), true)?,
            GoalIntakeError::IntakeIncomplete,
        ),
    ];
    for (proposal, error) in cases {
        let value = pending(1)?;
        let store = RecordingStore::pending(value.clone());
        assert_eq!(
            block_on(PromoteGoalIntake::new(&store).execute(
                &project()?,
                task(1),
                GoalIntakeStoreVersion::INITIAL,
                &proposal
            )),
            Err(PromoteGoalIntakeFailure::InvalidPromotion(error))
        );
        let state = store.state()?;
        assert_eq!((state.captures, state.reads, state.promotions), (0, 1, 0));
        assert_eq!(
            state.loaded,
            Some(GoalIntakeLoadResult::Pending(StoredPendingGoalIntake::new(
                value,
                GoalIntakeStoreVersion::INITIAL
            )))
        );
    }
    Ok(())
}

#[test]
fn preflight_version_conflict_never_enters_the_promotion_port() -> TestResult {
    let store = RecordingStore::pending(pending(1)?);
    assert_eq!(
        block_on(PromoteGoalIntake::new(&store).execute(
            &project()?,
            task(1),
            GoalIntakeStoreVersion::new(2)?,
            &valid_goal()?
        )),
        Err(PromoteGoalIntakeFailure::Store(
            GoalContractStoreFailure::IntakeVersionConflict
        ))
    );
    assert_eq!(store.state()?.reads, 1);
    assert_eq!(store.state()?.promotions, 0);
    Ok(())
}

#[test]
fn read_failures_are_preserved_and_never_retried_or_promoted() -> TestResult {
    for error in [
        GoalContractStoreFailure::Unavailable,
        GoalContractStoreFailure::Corrupt,
        GoalContractStoreFailure::UnsupportedSchema,
        GoalContractStoreFailure::InvalidStoredData,
    ] {
        let store = RecordingStore::pending(pending(1)?);
        store.state()?.read_failure = Some(error);
        assert_eq!(
            block_on(PromoteGoalIntake::new(&store).execute(
                &project()?,
                task(1),
                GoalIntakeStoreVersion::INITIAL,
                &valid_goal()?
            )),
            Err(PromoteGoalIntakeFailure::Store(error))
        );
        assert_eq!(store.state()?.reads, 1);
        assert_eq!(store.state()?.promotions, 0);
    }
    Ok(())
}

#[test]
fn atomic_port_conflicts_are_returned_without_reload_or_second_write() -> TestResult {
    for error in [
        GoalContractStoreFailure::IntakeVersionConflict,
        GoalContractStoreFailure::IntakeNotPending,
        GoalContractStoreFailure::InvalidIntakePromotion,
        GoalContractStoreFailure::Unavailable,
    ] {
        let value = pending(1)?;
        let store = RecordingStore::pending(value.clone());
        store.state()?.promotion_failure = Some(error);
        assert_eq!(
            block_on(PromoteGoalIntake::new(&store).execute(
                &project()?,
                task(1),
                GoalIntakeStoreVersion::INITIAL,
                &valid_goal()?
            )),
            Err(PromoteGoalIntakeFailure::Store(error))
        );
        let state = store.state()?;
        assert_eq!((state.captures, state.reads, state.promotions), (0, 1, 1));
        assert_eq!(
            state.loaded,
            Some(GoalIntakeLoadResult::Pending(StoredPendingGoalIntake::new(
                value,
                GoalIntakeStoreVersion::INITIAL
            )))
        );
    }
    Ok(())
}

#[test]
fn promotion_keeps_same_goal_and_reentry_is_explicit_materialized_read() -> TestResult {
    let value = pending(1)?;
    let goal = valid_goal()?;
    let store = RecordingStore::pending(value.clone());
    assert_eq!(
        block_on(PromoteGoalIntake::new(&store).execute(
            &project()?,
            task(1),
            GoalIntakeStoreVersion::INITIAL,
            &goal
        ))?,
        value.reference()
    );
    assert_eq!(
        block_on(store.load_goal_intake(&project()?, task(1)))?,
        Some(GoalIntakeLoadResult::Materialized(value.reference()))
    );
    assert_eq!(store.state()?.materialized.as_ref(), Some(&goal));
    assert_eq!(goal.original(), Some(value.original()));
    assert_eq!(
        block_on(PromoteGoalIntake::new(&store).execute(
            &project()?,
            task(1),
            GoalIntakeStoreVersion::INITIAL,
            &goal
        )),
        Err(PromoteGoalIntakeFailure::Store(
            GoalContractStoreFailure::IntakeNotPending
        ))
    );
    assert_eq!(
        block_on(CaptureGoalIntake::new(&store).execute(
            &project()?,
            task(1),
            TEXT.as_bytes().to_vec(),
            at(1)?
        )),
        Err(CaptureGoalIntakeFailure::Store(
            GoalContractStoreFailure::TaskAlreadyExists
        ))
    );
    assert_eq!(store.state()?.promotions, 1);
    Ok(())
}

#[test]
fn missing_pending_and_materialized_read_are_not_promotion_success() -> TestResult {
    for loaded in [
        None,
        Some(GoalIntakeLoadResult::Materialized(
            GoalContractReference::initial(task(1)),
        )),
    ] {
        let store = RecordingStore::default();
        store.state()?.loaded = loaded;
        assert_eq!(
            block_on(PromoteGoalIntake::new(&store).execute(
                &project()?,
                task(1),
                GoalIntakeStoreVersion::INITIAL,
                &valid_goal()?
            )),
            Err(PromoteGoalIntakeFailure::Store(
                GoalContractStoreFailure::IntakeNotPending
            ))
        );
        assert_eq!(store.state()?.promotions, 0);
    }
    Ok(())
}

#[test]
fn foreign_or_later_materialized_and_foreign_pending_reads_are_invalid_stored_data() -> TestResult {
    let later = valid_goal()?.next_reference()?;
    for loaded in [
        GoalIntakeLoadResult::Materialized(GoalContractReference::initial(task(2))),
        GoalIntakeLoadResult::Materialized(later),
        GoalIntakeLoadResult::Pending(StoredPendingGoalIntake::new(
            pending(2)?,
            GoalIntakeStoreVersion::INITIAL,
        )),
    ] {
        let store = RecordingStore::default();
        store.state()?.loaded = Some(loaded);
        assert_eq!(
            block_on(PromoteGoalIntake::new(&store).execute(
                &project()?,
                task(1),
                GoalIntakeStoreVersion::INITIAL,
                &valid_goal()?
            )),
            Err(PromoteGoalIntakeFailure::Store(
                GoalContractStoreFailure::InvalidStoredData
            ))
        );
        assert_eq!(store.state()?.promotions, 0);
    }
    Ok(())
}

#[test]
fn baseline_defaults_are_closed_failures_for_intake_methods() -> TestResult {
    let store = BaselineOnlyStore;
    let project = project()?;
    let pending = pending(1)?;
    let goal = valid_goal()?;
    assert_eq!(
        block_on(store.list_pending_goal_intakes(&project, None)),
        Err(GoalContractStoreFailure::UnsupportedSchema)
    );
    assert_eq!(
        block_on(store.capture_goal_intake(&project, &pending)),
        Err(GoalContractStoreFailure::UnsupportedSchema)
    );
    assert_eq!(
        block_on(store.load_goal_intake(&project, task(1))),
        Err(GoalContractStoreFailure::UnsupportedSchema)
    );
    assert_eq!(
        block_on(store.promote_goal_intake(
            &project,
            task(1),
            GoalIntakeStoreVersion::INITIAL,
            &goal
        )),
        Err(GoalContractStoreFailure::UnsupportedSchema)
    );
    Ok(())
}

#[test]
fn stored_versions_are_nonzero_and_fit_the_persistence_integer() {
    assert!(GoalIntakeStoreVersion::new(0).is_err());
    assert!(GoalIntakeStoreVersion::new(i64::MAX as u64 + 1).is_err());
    assert_eq!(
        GoalIntakeStoreVersion::new(i64::MAX as u64).map(GoalIntakeStoreVersion::get),
        Ok(i64::MAX as u64)
    );
}

fn review_batch(
    p: &PendingGoalIntake,
    a: usize,
    b: usize,
) -> TestResult<a3_domain::GoalIntakeReviewBatchProposal> {
    Ok(a3_domain::GoalIntakeReviewBatchProposal::new(
        p.original().range(a, b)?,
        Vec::new(),
        Vec::new(),
    )?)
}
#[test]
fn small_review_cas_appends_actual_source_results_without_materializing_or_running() -> TestResult {
    let p = pending(1)?;
    let store = RecordingStore::pending(p.clone());
    let first = block_on(a3_application::RecordGoalIntakeReview::new(&store).execute(
        &project()?,
        task(1),
        GoalIntakeStoreVersion::INITIAL,
        None,
        review_batch(&p, 0, 10)?,
    ))?;
    assert_eq!(first.revision().get(), 1);
    let old = first.clone();
    let next = block_on(a3_application::RecordGoalIntakeReview::new(&store).execute(
        &project()?,
        task(1),
        GoalIntakeStoreVersion::INITIAL,
        Some(first.revision()),
        review_batch(&p, 10, TEXT.len())?,
    ))?;
    next.validate_successor_of(&old)?;
    let s = store.state()?;
    assert_eq!(s.review_commits, 2);
    assert_eq!(s.promotions, 0);
    assert!(s.materialized.is_none());
    assert_eq!(
        s.loaded,
        Some(GoalIntakeLoadResult::Pending(StoredPendingGoalIntake::new(
            p,
            GoalIntakeStoreVersion::INITIAL
        )))
    );
    Ok(())
}
#[test]
fn stale_capture_or_review_revision_and_repeated_windows_make_no_commit_or_retry() -> TestResult {
    let p = pending(1)?;
    let store = RecordingStore::pending(p.clone());
    let first = block_on(a3_application::RecordGoalIntakeReview::new(&store).execute(
        &project()?,
        task(1),
        GoalIntakeStoreVersion::INITIAL,
        None,
        review_batch(&p, 0, 10)?,
    ))?;
    for (capture, revision) in [
        (GoalIntakeStoreVersion::new(2)?, Some(first.revision())),
        (GoalIntakeStoreVersion::INITIAL, None),
    ] {
        assert!(matches!(
            block_on(a3_application::RecordGoalIntakeReview::new(&store).execute(
                &project()?,
                task(1),
                capture,
                revision,
                review_batch(&p, 10, TEXT.len())?
            )),
            Err(a3_application::RecordGoalIntakeReviewFailure::Store(
                GoalContractStoreFailure::IntakeVersionConflict
                    | GoalContractStoreFailure::IntakeReviewVersionConflict
            ))
        ));
    }
    assert!(matches!(
        block_on(a3_application::RecordGoalIntakeReview::new(&store).execute(
            &project()?,
            task(1),
            GoalIntakeStoreVersion::INITIAL,
            Some(first.revision()),
            review_batch(&p, 0, 10)?
        )),
        Err(
            a3_application::RecordGoalIntakeReviewFailure::InvalidReview(
                a3_domain::GoalIntakeReviewError::WindowAlreadyReviewed
            )
        )
    ));
    assert_eq!(store.state()?.review_commits, 1);
    assert_eq!(store.state()?.review, Some(first));
    Ok(())
}
#[test]
fn present_open_question_or_unconsidered_tail_blocks_promotion_before_port() -> TestResult {
    let p = pending(1)?;
    for open in [false, true] {
        let store = RecordingStore::pending(p.clone());
        let end = if open { TEXT.len() } else { TEXT.len() - 1 };
        let r = p.original().range(0, end)?;
        let questions = if open {
            vec![a3_domain::GoalIntakeQuestionDraft::new(
                a3_domain::GoalIntakeQuestionKind::MissingUserChoice,
                "Which behavior?".into(),
                vec![r],
            )?]
        } else {
            Vec::new()
        };
        let batch = a3_domain::GoalIntakeReviewBatchProposal::new(r, Vec::new(), questions)?;
        block_on(a3_application::RecordGoalIntakeReview::new(&store).execute(
            &project()?,
            task(1),
            GoalIntakeStoreVersion::INITIAL,
            None,
            batch,
        ))?;
        assert!(matches!(
            block_on(PromoteGoalIntake::new(&store).execute(
                &project()?,
                task(1),
                GoalIntakeStoreVersion::INITIAL,
                &valid_goal()?
            )),
            Err(PromoteGoalIntakeFailure::InvalidReview(
                a3_domain::GoalIntakeReviewError::OpenQuestions
                    | a3_domain::GoalIntakeReviewError::IncompleteReview
            ))
        ));
        assert_eq!(store.state()?.promotions, 0);
        assert!(matches!(
            store.state()?.loaded,
            Some(GoalIntakeLoadResult::Pending(_))
        ));
    }
    Ok(())
}
#[test]
fn review_constraint_cannot_be_lost_but_complete_preserved_review_survives_promotion() -> TestResult
{
    let p = pending(1)?;
    let store = RecordingStore::pending(p.clone());
    let r = p
        .original()
        .range(TEXT.find("Preserve").ok_or("constraint")?, TEXT.len())?;
    let finding = a3_domain::GoalIntakeFinding::new(
        a3_domain::GoalIntakeFindingKind::Constraint,
        r,
        "preserve protected files".into(),
    )?;
    let review = block_on(a3_application::RecordGoalIntakeReview::new(&store).execute(
        &project()?,
        task(1),
        GoalIntakeStoreVersion::INITIAL,
        None,
        a3_domain::GoalIntakeReviewBatchProposal::new(
            p.original().range(0, TEXT.len())?,
            vec![finding],
            Vec::new(),
        )?,
    ))?;
    assert!(matches!(
        block_on(PromoteGoalIntake::new(&store).execute(
            &project()?,
            task(1),
            GoalIntakeStoreVersion::INITIAL,
            &valid_goal()?
        )),
        Err(PromoteGoalIntakeFailure::InvalidReview(
            a3_domain::GoalIntakeReviewError::FindingOmitted
        ))
    ));
    assert_eq!(store.state()?.promotions, 0);
    let base = valid_goal()?;
    let d = base.draft();
    let kept = GoalContract::initial(
        task(1),
        GoalContractDraft::new(
            d.objective().clone(),
            d.acceptance_criteria().to_vec(),
            vec![
                GoalConstraint::try_from_string("preserve protected files".into())?.with_origin(r),
            ],
            Vec::new(),
            Vec::new(),
            d.success_verification().clone(),
        )?,
        at(1)?,
    )
    .with_original(p.original().clone())?;
    block_on(PromoteGoalIntake::new(&store).execute(
        &project()?,
        task(1),
        GoalIntakeStoreVersion::INITIAL,
        &kept,
    ))?;
    assert_eq!(
        block_on(store.load_goal_intake_review(&project()?, task(1)))?,
        Some(review)
    );
    assert!(matches!(
        store.state()?.loaded,
        Some(GoalIntakeLoadResult::Materialized(_))
    ));
    assert!(matches!(
        block_on(a3_application::RecordGoalIntakeReview::new(&store).execute(
            &project()?,
            task(1),
            GoalIntakeStoreVersion::INITIAL,
            None,
            review_batch(&p, 0, 1)?
        )),
        Err(a3_application::RecordGoalIntakeReviewFailure::Store(
            GoalContractStoreFailure::IntakeNotPending
        ))
    ));
    Ok(())
}
#[test]
fn unavailable_review_storage_is_not_treated_as_absent_or_promoted() -> TestResult {
    let p = pending(1)?;
    let store = RecordingStore::pending(p.clone());
    store.state()?.review_failure = Some(GoalContractStoreFailure::Unavailable);
    assert!(matches!(
        block_on(PromoteGoalIntake::new(&store).execute(
            &project()?,
            task(1),
            GoalIntakeStoreVersion::INITIAL,
            &valid_goal()?
        )),
        Err(PromoteGoalIntakeFailure::Store(
            GoalContractStoreFailure::Unavailable
        ))
    ));
    assert_eq!(store.state()?.promotions, 0);
    assert!(matches!(
        block_on(a3_application::RecordGoalIntakeReview::new(&store).execute(
            &project()?,
            task(1),
            GoalIntakeStoreVersion::INITIAL,
            None,
            review_batch(&p, 0, 1)?
        )),
        Err(a3_application::RecordGoalIntakeReviewFailure::Store(
            GoalContractStoreFailure::Unavailable
        ))
    ));
    assert_eq!(store.state()?.review_commits, 0);
    Ok(())
}
#[test]
fn foreign_review_source_is_rejected_without_any_commit() -> TestResult {
    let p = pending(1)?;
    let store = RecordingStore::pending(p);
    let foreign = pending(2)?;
    assert!(matches!(
        block_on(a3_application::RecordGoalIntakeReview::new(&store).execute(
            &project()?,
            task(1),
            GoalIntakeStoreVersion::INITIAL,
            None,
            review_batch(&foreign, 0, 1)?
        )),
        Err(
            a3_application::RecordGoalIntakeReviewFailure::InvalidReview(
                a3_domain::GoalIntakeReviewError::InvalidSource
            )
        )
    ));
    assert_eq!(store.state()?.review_commits, 0);
    Ok(())
}
#[test]
fn new_review_methods_keep_unsupported_baseline_ports_closed() -> TestResult {
    let p = pending(1)?;
    let review = a3_domain::GoalIntakeReview::begin(&p, review_batch(&p, 0, 1)?)?;
    assert_eq!(
        block_on(BaselineOnlyStore.load_goal_intake_review(&project()?, task(1))),
        Err(GoalContractStoreFailure::UnsupportedSchema)
    );
    assert_eq!(
        block_on(BaselineOnlyStore.append_goal_intake_review(
            &project()?,
            task(1),
            GoalIntakeStoreVersion::INITIAL,
            None,
            &review
        )),
        Err(GoalContractStoreFailure::UnsupportedSchema)
    );
    Ok(())
}
