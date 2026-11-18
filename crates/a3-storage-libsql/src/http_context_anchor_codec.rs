//! Exact codec for the existing immutable ContextIntent V1 anchor; no live authority.
use crate::http_correction_plan_repository as parent_store;
use a3_application::{EditPhaseReservationFailure as E, HttpContextIntentAnchor};
use a3_domain::*;

const MIN_PARENT_BYTES: usize = 526;
const MAX_PARENT_BYTES: usize = 133000;
const MAX_ANCHOR_BYTES: usize = 137096;
// Version + parent length + seven 32-byte values + five u64 values + three ranges.
const WRAPPER_BYTES: usize = 1 + 4 + 7 * 32 + 5 * 8 + 3 * 24;

// Keep the existing codec1 representation byte-for-byte, including its existing bounds.
pub(crate) fn encode(a: &HttpContextIntentAnchor) -> Result<Vec<u8>, E> {
    let parent = parent_store::anchor_bytes(&a.parent);
    if parent.len() > MAX_PARENT_BYTES {
        return Err(E::InvalidStoredData);
    }
    let mut bytes = vec![1];
    bytes.extend_from_slice(
        &u32::try_from(parent.len())
            .map_err(|_| E::InvalidStoredData)?
            .to_le_bytes(),
    );
    bytes.extend_from_slice(&parent);
    for value in [
        a.parent_job.as_bytes(),
        a.parent_proposal_digest.as_bytes(),
        a.read_key.as_bytes(),
        a.read_tool.as_bytes(),
        a.read_event.as_bytes(),
        a.primary_text.as_bytes(),
        a.context_text.as_bytes(),
    ] {
        bytes.extend_from_slice(value)
    }
    for value in [
        u64::from(a.read_attempt.get()),
        a.read_started_at.unix_millis(),
        a.read_tail.get(),
        a.read_at.unix_millis(),
        u64::from(a.focus.get()),
    ] {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    for range in [a.primary_range, a.context_range, a.definition_range] {
        bytes.extend_from_slice(&range_bytes(range));
    }
    if bytes.len() > MAX_ANCHOR_BYTES {
        return Err(E::InvalidStoredData);
    }
    Ok(bytes)
}

/// Reconstructs descriptive metadata, binding the nested parent to the supplied Goal.
/// Storage must still authenticate the parent, actual read and accepted reservation;
/// application code must replay current complete sources before consuming the intent.
pub(crate) fn decode(bytes: &[u8], goal: &GoalContract) -> Result<HttpContextIntentAnchor, E> {
    if !(MIN_PARENT_BYTES + WRAPPER_BYTES..=MAX_ANCHOR_BYTES).contains(&bytes.len()) {
        return Err(E::InvalidStoredData);
    }
    let mut reader = Reader { bytes, at: 0 };
    if reader.byte()? != 1 {
        return Err(E::InvalidStoredData);
    }
    let parent_len = usize::try_from(reader.u32()?).map_err(|_| E::InvalidStoredData)?;
    if !(MIN_PARENT_BYTES..=MAX_PARENT_BYTES).contains(&parent_len)
        || parent_len.checked_add(WRAPPER_BYTES) != Some(bytes.len())
    {
        return Err(E::InvalidStoredData);
    }
    let parent = parent_store::decode_retained_anchor(reader.take(parent_len)?, goal)?;
    let parent_job = ContentHash::from_bytes(reader.fixed()?);
    let parent_proposal_digest = ContentHash::from_bytes(reader.fixed()?);
    let read_key = ContentHash::from_bytes(reader.fixed()?);
    let read_tool = ToolRunId::from_bytes(reader.fixed()?);
    let read_event = RunEventId::from_bytes(reader.fixed()?);
    let primary_text = ContentHash::from_bytes(reader.fixed()?);
    let context_text = ContentHash::from_bytes(reader.fixed()?);
    let read_attempt =
        AgentToolAttemptNumber::new(reader.narrow_u32()?).map_err(|_| E::InvalidStoredData)?;
    let read_started_at =
        AgentRunTimestamp::from_unix_millis(reader.u64()?).map_err(|_| E::InvalidStoredData)?;
    let read_tail = RunEventSequence::new(reader.u64()?).map_err(|_| E::InvalidStoredData)?;
    let read_at =
        AgentRunTimestamp::from_unix_millis(reader.u64()?).map_err(|_| E::InvalidStoredData)?;
    let focus = AgentFileStartLine::new(reader.narrow_u32()?).map_err(|_| E::InvalidStoredData)?;
    let primary_range = reader.range()?;
    let context_range = reader.range()?;
    let definition_range = reader.range()?;
    if reader.at != bytes.len()
        || primary_range != parent.source_range
        || primary_text != parent.source_text
        || primary_range.is_empty()
        || context_range.is_empty()
        || definition_range.is_empty()
        || primary_range.len() > 8192
        || context_range.len() > 8192
        || !primary_range.contains(definition_range)
        || definition_range.start_position().row().checked_add(1) != Some(focus.get())
        || (context_range.start_byte() < primary_range.end_byte()
            && primary_range.start_byte() < context_range.end_byte())
        || read_started_at > read_at
        || parent.sequence.get().checked_add(1) != Some(read_tail.get())
        || parent
            .source_offer
            .as_ref()
            .is_none_or(|offer| !context_range.contains(offer.range()))
    {
        return Err(E::InvalidStoredData);
    }
    let anchor = HttpContextIntentAnchor {
        parent,
        parent_job,
        parent_proposal_digest,
        read_key,
        read_tool,
        read_attempt,
        read_started_at,
        read_event,
        read_tail,
        read_at,
        primary_range,
        primary_text,
        context_range,
        context_text,
        definition_range,
        focus,
    };
    if encode(&anchor)? != bytes {
        return Err(E::InvalidStoredData);
    }
    Ok(anchor)
}

fn range_bytes(range: SourceRange) -> Vec<u8> {
    [
        range.start_byte(),
        range.end_byte(),
        range.start_position().row(),
        range.start_position().column(),
        range.end_position().row(),
        range.end_position().column(),
    ]
    .into_iter()
    .flat_map(u32::to_le_bytes)
    .collect()
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
    fn range(&mut self) -> Result<SourceRange, E> {
        let start = usize::try_from(self.u32()?).map_err(|_| E::InvalidStoredData)?;
        let end = usize::try_from(self.u32()?).map_err(|_| E::InvalidStoredData)?;
        SourceRange::new(
            start,
            end,
            SourcePosition::new(self.u32()?, self.u32()?),
            SourcePosition::new(self.u32()?, self.u32()?),
        )
        .map_err(|_| E::InvalidStoredData)
    }
}
