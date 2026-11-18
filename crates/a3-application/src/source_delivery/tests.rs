use super::*;
use crate::AgentSourcePage;
use a3_domain::{
    AgentFileStartLine, RepositoryPath, SourcePosition, SourceRange, SymbolId, TaskId,
};

type TestResult<T = ()> = Result<T, Box<dyn Error>>;

fn scope() -> TestResult<SourceDeliveryScope> {
    Ok(SourceDeliveryScope {
        kind: SourceDeliveryScopeKind::Edit,
        worktree: WorktreeId::from_bytes([1; 32]),
        goal: GoalContractReference::initial(TaskId::from_bytes([2; 32])),
        ledger: TaskLedgerRevision::INITIAL,
        run: AgentRunId::from_bytes([3; 32]),
        step: TaskStepId::from_bytes([4; 32]),
        attempt: TaskStepAttemptNumber::new(1)?,
        verification: VerificationSpecId::from_bytes([5; 32]),
        snapshot: SnapshotId::from_bytes([6; 32]),
    })
}

fn file(path: &[u8], hash: u8) -> TestResult<FileRevision> {
    Ok(FileRevision::new(
        RepositoryPath::try_from_bytes(path.to_vec())?,
        ContentHash::from_bytes([hash; 32]),
    ))
}

fn wire(
    scope: SourceDeliveryScope,
    file: &FileRevision,
    start: u32,
    end: u32,
) -> TestResult<SourceWireDelivery> {
    let range = SourceRange::new(
        usize::try_from(start)?,
        usize::try_from(end)?,
        SourcePosition::new(start, 0),
        SourcePosition::new(end, 0),
    )?;
    let page = AgentSourcePage::new(
        file.clone(),
        range,
        AgentFileStartLine::new(start + 1)?,
        "\n".repeat(usize::try_from(range.len())?),
        if range.is_empty() {
            None
        } else {
            Some(AgentFileStartLine::new(end + 1)?)
        },
        !range.is_empty(),
    )?;
    Ok(SourceWireDelivery::from_observed_dispatch(
        ContextOriginalSource::from_packed_page(scope.snapshot, &page),
        ContentHash::from_bytes([7; 32]),
    ))
}

fn deliver(
    progress: &mut SourceDeliveryProgress,
    scope: SourceDeliveryScope,
    revision: &FileRevision,
    start: u32,
    end: u32,
) -> TestResult<SourceDeliveryOutcome> {
    let ordinal = progress.begin(scope, SourceAcquisition::Read(revision.clone()))?;
    let delivery = wire(scope, revision, start, end)?;
    Ok(progress.finish(scope, ordinal, Some(delivery))?.outcome())
}

#[test]
fn union_counts_only_disjoint_bytes_and_partial_overlap() -> TestResult {
    let scope = scope()?;
    let mut progress = SourceDeliveryProgress::new(scope);
    let revision = file(b"one.py", 8)?;
    for (start, end, expected) in [
        (0, 10, SourceDeliveryOutcome::NewBytes(10)),
        (10, 20, SourceDeliveryOutcome::NewBytes(10)),
        (5, 15, SourceDeliveryOutcome::AlreadyDelivered),
        (15, 25, SourceDeliveryOutcome::NewBytes(5)),
        (30, 40, SourceDeliveryOutcome::NewBytes(10)),
        (22, 35, SourceDeliveryOutcome::NewBytes(5)),
        (0, 40, SourceDeliveryOutcome::AlreadyDelivered),
    ] {
        assert_eq!(
            deliver(&mut progress, scope, &revision, start, end)?,
            expected
        );
    }
    let same_hash_other_path = file(b"two.py", 8)?;
    assert_eq!(
        deliver(&mut progress, scope, &same_hash_other_path, 0, 10)?,
        SourceDeliveryOutcome::NewBytes(10)
    );
    Ok(())
}

#[test]
fn unsent_and_empty_sources_spend_acquisitions_without_progress() -> TestResult {
    let scope = scope()?;
    let mut progress = SourceDeliveryProgress::new(scope);
    let revision = file(b"one.py", 8)?;
    let acquire = SourceAcquisition::Read(revision.clone());
    // A reader or packed context has no wire observation to supply.
    let first = progress.begin(scope, acquire.clone())?;
    let receipt = progress.finish(scope, first, None)?;
    assert_eq!(receipt.outcome(), SourceDeliveryOutcome::NotDelivered);
    assert_eq!(receipt.delivery(), None);
    assert_eq!(
        deliver(&mut progress, scope, &revision, 10, 10)?,
        SourceDeliveryOutcome::EmptyRange
    );
    assert_eq!(
        progress.begin(scope, acquire),
        Err(SourceDeliveryFailure::NoveltyLimit)
    );
    assert_eq!(progress.receipts().len(), 2);
    Ok(())
}

#[test]
fn new_bytes_reset_the_streak_but_the_ninth_acquisition_is_denied() -> TestResult {
    let scope = scope()?;
    let mut progress = SourceDeliveryProgress::new(scope);
    let revision = file(b"one.py", 8)?;
    let first = progress.begin(scope, SourceAcquisition::Read(revision.clone()))?;
    progress.finish(scope, first, None)?;
    for start in 0..7 {
        assert_eq!(
            deliver(&mut progress, scope, &revision, start, start + 1)?,
            SourceDeliveryOutcome::NewBytes(1)
        );
    }
    assert_eq!(progress.receipts().len(), 8);
    assert_eq!(
        progress.begin(scope, SourceAcquisition::Read(revision)),
        Err(SourceDeliveryFailure::AcquisitionLimit)
    );
    Ok(())
}

#[test]
fn same_gap_cannot_relocalize_twice_but_distinct_endpoints_can() -> TestResult {
    let scope = scope()?;
    let mut progress = SourceDeliveryProgress::new(scope);
    let revision = file(b"caller.py", 8)?;
    let gap = |need, id| SourceAcquisition::Relocalize {
        need,
        target: GraphEndpoint::Symbol(SymbolId::from_bytes([id; 32])),
    };
    let caller = gap(EditEvidenceNeed::Caller, 9);
    let first = progress.begin(scope, caller.clone())?;
    let receipt = progress.finish(scope, first, Some(wire(scope, &revision, 0, 3)?))?;
    assert_eq!(receipt.admission().acquisition(), &caller);
    assert_eq!(receipt.outcome(), SourceDeliveryOutcome::NewBytes(3));
    assert_eq!(
        progress.begin(scope, caller),
        Err(SourceDeliveryFailure::GapAlreadyRelocalized)
    );
    let second = progress.begin(scope, gap(EditEvidenceNeed::Caller, 10))?;
    progress.finish(scope, second, Some(wire(scope, &revision, 3, 5)?))?;
    let third = progress.begin(scope, gap(EditEvidenceNeed::Test, 9))?;
    assert_eq!(third.get(), 3);
    Ok(())
}

#[test]
fn stale_snapshot_or_wrong_revision_cannot_credit_the_charged_acquisition() -> TestResult {
    let scope = scope()?;
    let mut stale = scope;
    stale.snapshot = SnapshotId::from_bytes([19; 32]);
    let mut progress = SourceDeliveryProgress::new(scope);
    let revision = file(b"one.py", 8)?;
    let changed = file(b"one.py", 9)?;
    let other = file(b"other.py", 8)?;
    let acquire = SourceAcquisition::Read(revision.clone());
    assert_eq!(
        progress.begin(stale, acquire.clone()),
        Err(SourceDeliveryFailure::StaleScope)
    );
    let ordinal = progress.begin(scope, acquire.clone())?;
    assert_eq!(
        progress.finish(scope, ordinal, Some(wire(stale, &revision, 0, 10)?)),
        Err(SourceDeliveryFailure::StaleDelivery)
    );
    for wrong in [changed.clone(), other] {
        assert_eq!(
            progress.finish(scope, ordinal, Some(wire(scope, &wrong, 0, 10)?)),
            Err(SourceDeliveryFailure::RevisionMismatch)
        );
    }
    assert!(progress.receipts().is_empty());
    assert_eq!(
        progress.pending().map(SourceAcquisitionAdmission::ordinal),
        Some(ordinal)
    );
    assert_eq!(
        progress.begin(scope, acquire),
        Err(SourceDeliveryFailure::AcquisitionPending)
    );
    progress.finish(scope, ordinal, Some(wire(scope, &revision, 0, 10)?))?;
    assert_eq!(
        progress.begin(scope, SourceAcquisition::Read(changed)),
        Err(SourceDeliveryFailure::RevisionMismatch)
    );
    Ok(())
}

#[test]
fn receipt_replay_preserves_pending_owner_and_rederives_novelty() -> TestResult {
    let scope = scope()?;
    let mut original = SourceDeliveryProgress::new(scope);
    let revision = file(b"one.py", 8)?;
    deliver(&mut original, scope, &revision, 0, 4)?;
    let second = original.begin(scope, SourceAcquisition::Read(revision.clone()))?;
    let mut reopened =
        SourceDeliveryProgress::replay(scope, original.receipts(), original.pending().cloned())?;
    assert_eq!(reopened.receipts(), original.receipts());
    assert_eq!(reopened.pending(), original.pending());
    let mut foreign_pending_owner = scope;
    foreign_pending_owner.attempt = TaskStepAttemptNumber::new(2)?;
    assert!(matches!(
        SourceDeliveryProgress::replay(foreign_pending_owner, &[], original.pending().cloned()),
        Err(SourceDeliveryFailure::StaleScope)
    ));
    assert_eq!(
        reopened.begin(scope, SourceAcquisition::Read(revision.clone())),
        Err(SourceDeliveryFailure::AcquisitionPending)
    );
    let first = reopened.receipts()[0].admission().ordinal();
    assert_eq!(
        reopened.finish(scope, first, None),
        Err(SourceDeliveryFailure::ReceiptMismatch)
    );
    let result = reopened.finish(scope, second, Some(wire(scope, &revision, 2, 6)?))?;
    assert_eq!(result.outcome(), SourceDeliveryOutcome::NewBytes(2));
    assert_eq!(result.admission().scope(), scope);
    assert_eq!(
        result
            .delivery()
            .ok_or("wire observation missing")?
            .wire_digest(),
        ContentHash::from_bytes([7; 32])
    );

    let mut tampered = reopened.receipts().to_vec();
    tampered[1].outcome = SourceDeliveryOutcome::NewBytes(4);
    assert!(matches!(
        SourceDeliveryProgress::replay(scope, &tampered, None),
        Err(SourceDeliveryFailure::ReceiptMismatch)
    ));
    let mut foreign_scope = scope;
    foreign_scope.step = TaskStepId::from_bytes([41; 32]);
    assert!(matches!(
        SourceDeliveryProgress::replay(foreign_scope, reopened.receipts(), None),
        Err(SourceDeliveryFailure::StaleScope)
    ));
    Ok(())
}
