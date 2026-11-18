//! Exact content-free V1 anchor; rows and current source remain separate authorities.
use a3_application::{
    ClosedEditNoChangeOwner, ClosedEditWindow, EditPhaseCallKind, EditPhaseReservationFailure as E,
    HttpNoChangeReplanAnchor, TaskLedgerStoreVersion,
};
use a3_domain::*;

pub(super) fn encode(a: &HttpNoChangeReplanAnchor) -> Result<Vec<u8>, E> {
    let o = &a.closed_owner;
    let mut b = vec![1];
    for id in [
        o.worktree.as_bytes(),
        o.goal.task_id().as_bytes(),
        o.original_goal.as_bytes(),
        o.run.as_bytes(),
        o.step.as_bytes(),
        o.verification.as_bytes(),
        o.index.as_bytes(),
        o.snapshot.as_bytes(),
        o.profile.id().as_bytes(),
        a.window.revision().content_hash().as_bytes(),
        a.window.digest().as_bytes(),
        a.closure.as_bytes(),
        a.parent_job.as_bytes(),
        a.producer_job.as_bytes(),
        a.diagnostic.as_bytes(),
        a.old_job.as_bytes(),
        a.old_source_read.as_bytes(),
    ] {
        b.extend_from_slice(id);
    }
    for n in [
        u64::from(o.goal.revision().get()),
        u64::from(o.ledger.get()),
        o.version.get(),
        o.sequence.get(),
        u64::from(o.attempt.get()),
        u64::from(o.profile.version().get()),
        a.closed_sequence.get(),
        a.closed_version.get(),
    ] {
        b.extend_from_slice(&n.to_le_bytes());
    }
    let r = a.window.range();
    for n in [
        r.start_byte(),
        r.end_byte(),
        r.start_position().row(),
        r.start_position().column(),
        r.end_position().row(),
        r.end_position().column(),
    ] {
        b.extend_from_slice(&n.to_le_bytes());
    }
    b.push(u8::from(
        a.old_call_kind == EditPhaseCallKind::StructuralRepair,
    ));
    let path = a.window.revision().path().as_bytes();
    b.extend_from_slice(
        &u32::try_from(path.len())
            .map_err(|_| E::InvalidStoredData)?
            .to_le_bytes(),
    );
    b.extend_from_slice(path);
    if !(512..=133000).contains(&b.len()) {
        return Err(E::InvalidStoredData);
    }
    Ok(b)
}
pub(super) fn decode(bytes: &[u8], goal: &GoalContract) -> Result<HttpNoChangeReplanAnchor, E> {
    if !(512..=133000).contains(&bytes.len()) {
        return Err(E::InvalidStoredData);
    }
    let mut r = Reader { bytes, at: 0 };
    if r.byte()? != 1 {
        return Err(E::InvalidStoredData);
    }
    let worktree = WorktreeId::from_bytes(r.fixed()?);
    let task = TaskId::from_bytes(r.fixed()?);
    let original_goal = GoalOriginalDigest::from_bytes(r.fixed()?);
    let run = AgentRunId::from_bytes(r.fixed()?);
    let step = TaskStepId::from_bytes(r.fixed()?);
    let verification = VerificationSpecId::from_bytes(r.fixed()?);
    let index = IndexRunId::from_bytes(r.fixed()?);
    let snapshot = SnapshotId::from_bytes(r.fixed()?);
    let profile_id = ModelProfileId::from_bytes(r.fixed()?);
    let content = ContentHash::from_bytes(r.fixed()?);
    let digest = ContentHash::from_bytes(r.fixed()?);
    let closure = RunEventId::from_bytes(r.fixed()?);
    let parent_job = ContentHash::from_bytes(r.fixed()?);
    let producer_job = ContentHash::from_bytes(r.fixed()?);
    let diagnostic = ContentHash::from_bytes(r.fixed()?);
    let old_job = ContentHash::from_bytes(r.fixed()?);
    let old_source_read = ContentHash::from_bytes(r.fixed()?);
    let goal_revision = r.narrow()?;
    let ledger = TaskLedgerRevision::new(r.narrow()?).map_err(|_| E::InvalidStoredData)?;
    let version = TaskLedgerStoreVersion::new(r.u64()?).map_err(|_| E::InvalidStoredData)?;
    let sequence = RunEventSequence::new(r.u64()?).map_err(|_| E::InvalidStoredData)?;
    let attempt = TaskStepAttemptNumber::new(r.narrow()?).map_err(|_| E::InvalidStoredData)?;
    let profile_version =
        ModelProfileVersion::from_u16(u16::try_from(r.u64()?).map_err(|_| E::InvalidStoredData)?)
            .map_err(|_| E::InvalidStoredData)?;
    let closed_sequence = RunEventSequence::new(r.u64()?).map_err(|_| E::InvalidStoredData)?;
    let closed_version = TaskLedgerStoreVersion::new(r.u64()?).map_err(|_| E::InvalidStoredData)?;
    let range = SourceRange::new(
        r.u32()? as usize,
        r.u32()? as usize,
        SourcePosition::new(r.u32()?, r.u32()?),
        SourcePosition::new(r.u32()?, r.u32()?),
    )
    .map_err(|_| E::InvalidStoredData)?;
    let old_call_kind = match r.byte()? {
        0 => EditPhaseCallKind::Initial,
        1 => EditPhaseCallKind::StructuralRepair,
        _ => return Err(E::InvalidStoredData),
    };
    let len = usize::try_from(r.u32()?).map_err(|_| E::InvalidStoredData)?;
    let path =
        RepositoryPath::try_from_bytes(r.take(len)?.to_vec()).map_err(|_| E::InvalidStoredData)?;
    if r.at != bytes.len()
        || task != goal.task_id()
        || goal_revision != goal.reference().revision().get()
        || goal.original().map(|v| v.digest()) != Some(original_goal)
        || sequence.get().checked_add(1) != Some(closed_sequence.get())
        || version.get().checked_add(1) != Some(closed_version.get())
    {
        return Err(E::InvalidStoredData);
    }
    let window = ClosedEditWindow::from_recorded(
        EvidenceRef::new(FileRevision::new(path, content), range),
        digest,
    )?;
    let a = HttpNoChangeReplanAnchor {
        closed_owner: ClosedEditNoChangeOwner {
            worktree,
            goal: goal.reference(),
            original_goal,
            run,
            ledger,
            version,
            sequence,
            step,
            attempt,
            verification,
            index,
            snapshot,
            profile: ModelProfileReference::new(profile_id, profile_version),
        },
        window,
        closure,
        closed_sequence,
        closed_version,
        parent_job,
        producer_job,
        diagnostic,
        old_job,
        old_source_read,
        old_call_kind,
    };
    if encode(&a)? != bytes {
        return Err(E::InvalidStoredData);
    }
    Ok(a)
}
struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}
impl<'a> Reader<'a> {
    fn take(&mut self, len: usize) -> Result<&'a [u8], E> {
        let end = self.at.checked_add(len).ok_or(E::InvalidStoredData)?;
        let b = self.bytes.get(self.at..end).ok_or(E::InvalidStoredData)?;
        self.at = end;
        Ok(b)
    }
    fn fixed<const N: usize>(&mut self) -> Result<[u8; N], E> {
        self.take(N)?.try_into().map_err(|_| E::InvalidStoredData)
    }
    fn byte(&mut self) -> Result<u8, E> {
        Ok(u8::from_le_bytes(self.fixed()?))
    }
    fn u32(&mut self) -> Result<u32, E> {
        Ok(u32::from_le_bytes(self.fixed()?))
    }
    fn u64(&mut self) -> Result<u64, E> {
        Ok(u64::from_le_bytes(self.fixed()?))
    }
    fn narrow(&mut self) -> Result<u32, E> {
        u32::try_from(self.u64()?).map_err(|_| E::InvalidStoredData)
    }
}
