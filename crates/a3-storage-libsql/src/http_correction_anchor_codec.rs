//! Exact codec for the existing immutable HTTP Plan anchor; no live or execution authority.
use a3_application::{
    EditPhaseReservationFailure as E, HttpCorrectionPlanAnchor, HttpCorrectionPlanSourceOffer,
    TaskLedgerStoreVersion,
};
use a3_domain::*;

pub(super) fn encode(a: &HttpCorrectionPlanAnchor) -> Vec<u8> {
    let mut b = vec![2];
    for id in [
        a.worktree.as_bytes(),
        a.goal.task_id().as_bytes(),
        a.original.as_bytes(),
        a.run.as_bytes(),
        a.step.as_bytes(),
        a.verification.as_bytes(),
        a.index.as_bytes(),
        a.snapshot.as_bytes(),
        a.profile.id().as_bytes(),
        a.diagnostic.as_bytes(),
        a.tool.as_bytes(),
        a.source.content_hash().as_bytes(),
        a.source_text.as_bytes(),
    ] {
        b.extend_from_slice(id);
    }
    for n in [
        u64::from(a.goal.revision().get()),
        u64::from(a.ledger.get()),
        a.version.get(),
        a.sequence.get(),
        u64::from(a.failed_attempt.get()),
        u64::from(a.profile.version().get()),
        u64::from(a.tool_attempt.get()),
        u64::from(a.source_range.start_byte()),
        u64::from(a.source_range.end_byte()),
        u64::from(a.source_range.start_position().row()),
        u64::from(a.source_range.start_position().column()),
        u64::from(a.source_range.end_position().row()),
        u64::from(a.source_range.end_position().column()),
    ] {
        b.extend_from_slice(&n.to_le_bytes());
    }
    b.extend_from_slice(&(a.source.path().as_bytes().len() as u32).to_le_bytes());
    b.push(u8::from(a.source_offer.is_some()));
    if let Some(offer) = &a.source_offer {
        b.extend_from_slice(offer.digest().as_bytes());
        let r = offer.range();
        for n in [
            r.start_byte(),
            r.end_byte(),
            r.start_position().row(),
            r.start_position().column(),
            r.end_position().row(),
            r.end_position().column(),
            offer.inspection().start_line().get(),
            u32::from(offer.inspection().line_count().get()),
        ] {
            b.extend_from_slice(&n.to_le_bytes());
        }
    }
    b.extend_from_slice(a.source.path().as_bytes());
    b
}

pub(super) fn decode(bytes: &[u8], goal: &GoalContract) -> Result<HttpCorrectionPlanAnchor, E> {
    if !(526..=133000).contains(&bytes.len()) {
        return Err(E::InvalidStoredData);
    }
    let mut r = Reader { bytes, at: 0 };
    if r.byte()? != 2 {
        return Err(E::InvalidStoredData);
    }
    let worktree = WorktreeId::from_bytes(r.fixed()?);
    let task = TaskId::from_bytes(r.fixed()?);
    let original = GoalOriginalDigest::from_bytes(r.fixed()?);
    let run = AgentRunId::from_bytes(r.fixed()?);
    let step = TaskStepId::from_bytes(r.fixed()?);
    let verification = VerificationSpecId::from_bytes(r.fixed()?);
    let index = IndexRunId::from_bytes(r.fixed()?);
    let snapshot = SnapshotId::from_bytes(r.fixed()?);
    let profile_id = ModelProfileId::from_bytes(r.fixed()?);
    let diagnostic = ContentHash::from_bytes(r.fixed()?);
    let tool = ToolRunId::from_bytes(r.fixed()?);
    let source_hash = ContentHash::from_bytes(r.fixed()?);
    let source_text = ContentHash::from_bytes(r.fixed()?);
    let goal_revision = r.narrow_u32()?;
    let ledger = TaskLedgerRevision::new(r.narrow_u32()?).map_err(|_| E::InvalidStoredData)?;
    let version = TaskLedgerStoreVersion::new(r.u64()?).map_err(|_| E::InvalidStoredData)?;
    let sequence = RunEventSequence::new(r.u64()?).map_err(|_| E::InvalidStoredData)?;
    let failed_attempt =
        TaskStepAttemptNumber::new(r.narrow_u32()?).map_err(|_| E::InvalidStoredData)?;
    let profile_version =
        ModelProfileVersion::from_u16(u16::try_from(r.u64()?).map_err(|_| E::InvalidStoredData)?)
            .map_err(|_| E::InvalidStoredData)?;
    let tool_attempt =
        AgentToolAttemptNumber::new(r.narrow_u32()?).map_err(|_| E::InvalidStoredData)?;
    let start = usize::try_from(r.u64()?).map_err(|_| E::InvalidStoredData)?;
    let end = usize::try_from(r.u64()?).map_err(|_| E::InvalidStoredData)?;
    let source_range = SourceRange::new(
        start,
        end,
        SourcePosition::new(r.narrow_u32()?, r.narrow_u32()?),
        SourcePosition::new(r.narrow_u32()?, r.narrow_u32()?),
    )
    .map_err(|_| E::InvalidStoredData)?;
    let path_len = usize::try_from(r.u32()?).map_err(|_| E::InvalidStoredData)?;
    let offer = match r.byte()? {
        0 => None,
        1 => {
            let digest = ContentHash::from_bytes(r.fixed()?);
            let start = usize::try_from(r.u32()?).map_err(|_| E::InvalidStoredData)?;
            let end = usize::try_from(r.u32()?).map_err(|_| E::InvalidStoredData)?;
            let range = SourceRange::new(
                start,
                end,
                SourcePosition::new(r.u32()?, r.u32()?),
                SourcePosition::new(r.u32()?, r.u32()?),
            )
            .map_err(|_| E::InvalidStoredData)?;
            let line = AgentFileStartLine::new(r.u32()?).map_err(|_| E::InvalidStoredData)?;
            let count =
                AgentFileLineCount::new(u16::try_from(r.u32()?).map_err(|_| E::InvalidStoredData)?)
                    .map_err(|_| E::InvalidStoredData)?;
            Some((digest, range, line, count))
        }
        _ => return Err(E::InvalidStoredData),
    };
    let path = RepositoryPath::try_from_bytes(r.take(path_len)?.to_vec())
        .map_err(|_| E::InvalidStoredData)?;
    if r.at != bytes.len() || source_range.is_empty() || source_range.len() > 8192 {
        return Err(E::InvalidStoredData);
    }
    if goal.task_id() != task
        || goal.reference().revision().get() != goal_revision
        || goal.original().map(|v| v.digest()) != Some(original)
    {
        return Err(E::StaleAnchor);
    }
    let source = FileRevision::new(path.clone(), source_hash);
    let source_offer = offer
        .map(|(digest, range, line, count)| {
            HttpCorrectionPlanSourceOffer::reconstruct_descriptor(
                worktree,
                index,
                snapshot,
                EvidenceRef::new(source.clone(), range),
                AgentFileInspection::new(path, line, count),
                digest,
            )
            .map_err(|_| E::InvalidStoredData)
        })
        .transpose()?;
    if source_offer
        .as_ref()
        .is_some_and(|offer| source_range.contains(offer.range()))
    {
        return Err(E::InvalidStoredData);
    }
    let anchor = HttpCorrectionPlanAnchor {
        worktree,
        goal: goal.reference(),
        original,
        run,
        ledger,
        version,
        sequence,
        step,
        failed_attempt,
        verification,
        index,
        snapshot,
        profile: ModelProfileReference::new(profile_id, profile_version),
        diagnostic,
        tool,
        tool_attempt,
        source,
        source_range,
        source_text,
        source_offer,
    };
    if encode(&anchor) != bytes {
        return Err(E::InvalidStoredData);
    }
    Ok(anchor)
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}
impl<'a> Reader<'a> {
    fn take(&mut self, len: usize) -> Result<&'a [u8], E> {
        let end = self.at.checked_add(len).ok_or(E::InvalidStoredData)?;
        let value = self.bytes.get(self.at..end).ok_or(E::InvalidStoredData)?;
        self.at = end;
        Ok(value)
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
    fn narrow_u32(&mut self) -> Result<u32, E> {
        u32::try_from(self.u64()?).map_err(|_| E::InvalidStoredData)
    }
}
