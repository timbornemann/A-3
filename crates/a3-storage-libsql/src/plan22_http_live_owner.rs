//! Private test support for fresh, owned local-model correction runs.
//! Does not replace the Core's shared durable phase-budget/policy checks.
use a3_application::AgentControllerControl;
use a3_domain::{AgentRun, AgentRunBudget, AgentRunId, AgentRunTimestamp};
use serde::Deserialize;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

const OWNER_LIMIT_MS: u64 = 900_000;
const MAX_REVIEW_BYTES: usize = 8192;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LiveOwnerFailure {
    Binding,
    ClockRegressed,
    Deadline,
    Budget,
    Cancelled,
    InvalidReview,
    ReviewDenied,
}

impl std::fmt::Display for LiveOwnerFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "owned live correction: {self:?}")
    }
}
impl std::error::Error for LiveOwnerFailure {}

/// Construct exactly once in the fresh owned native worker, before probe IO.
/// Bind exactly once after fresh Run creation; cold store reopen keeps this object.
pub(super) struct LiveOwnerClock {
    started: Instant,
    prior_owner_elapsed_ms: u64,
    bound: Option<(AgentRunId, AgentRunTimestamp, AgentRunBudget)>,
    last: Option<AgentRunTimestamp>,
}

impl LiveOwnerClock {
    pub(super) fn begin(owner_started_utc_ms: u64) -> Result<Self, LiveOwnerFailure> {
        let started = Instant::now();
        let now = u64::try_from(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|_| LiveOwnerFailure::ClockRegressed)?
                .as_millis(),
        )
        .map_err(|_| LiveOwnerFailure::Binding)?;
        let elapsed = now
            .checked_sub(owner_started_utc_ms)
            .ok_or(LiveOwnerFailure::ClockRegressed)?;
        if elapsed >= OWNER_LIMIT_MS {
            return Err(LiveOwnerFailure::Deadline);
        }
        Ok(Self {
            started,
            prior_owner_elapsed_ms: elapsed,
            bound: None,
            last: None,
        })
    }

    pub(super) fn bind(&mut self, run: &AgentRun) -> Result<(), LiveOwnerFailure> {
        if self.bound.is_some() || run.budget().duration_limit().millis() != OWNER_LIMIT_MS {
            return Err(LiveOwnerFailure::Binding);
        }
        self.bound = Some((run.id(), run.created_at(), run.budget()));
        Ok(())
    }

    /// Include owner startup/probe/setup and every elapsed review or cold-reopen gap.
    /// `durable_phase_clock` is the current settled/reserved phase floor from the store.
    pub(super) fn now(
        &mut self,
        run: &AgentRun,
        durable_phase_clock: Option<AgentRunTimestamp>,
        control: &dyn AgentControllerControl,
    ) -> Result<AgentRunTimestamp, LiveOwnerFailure> {
        if control.is_cancelled() {
            return Err(LiveOwnerFailure::Cancelled);
        }
        if self.bound != Some((run.id(), run.created_at(), run.budget())) {
            return Err(LiveOwnerFailure::Binding);
        }
        let elapsed = self
            .prior_owner_elapsed_ms
            .checked_add(
                u64::try_from(self.started.elapsed().as_millis())
                    .map_err(|_| LiveOwnerFailure::Binding)?,
            )
            .ok_or(LiveOwnerFailure::Binding)?;
        let now = logical_floor(
            run.created_at(),
            run.updated_at(),
            durable_phase_clock,
            self.last,
            elapsed,
            run.budget().duration_limit().millis(),
        )?;
        if run
            .budget_exhaustion(now)
            .map_err(|_| LiveOwnerFailure::Binding)?
            .is_some()
        {
            return Err(LiveOwnerFailure::Budget);
        }
        self.last = Some(now);
        Ok(now)
    }
}

fn logical_floor(
    created: AgentRunTimestamp,
    updated: AgentRunTimestamp,
    durable: Option<AgentRunTimestamp>,
    last: Option<AgentRunTimestamp>,
    elapsed_ms: u64,
    duration_ms: u64,
) -> Result<AgentRunTimestamp, LiveOwnerFailure> {
    if elapsed_ms >= OWNER_LIMIT_MS || duration_ms != OWNER_LIMIT_MS {
        return Err(LiveOwnerFailure::Deadline);
    }
    let deadline = created
        .unix_millis()
        .checked_add(duration_ms)
        .ok_or(LiveOwnerFailure::Binding)?;
    let current = created
        .unix_millis()
        .checked_add(elapsed_ms)
        .ok_or(LiveOwnerFailure::Binding)?
        .max(updated.unix_millis())
        .max(durable.map_or(0, AgentRunTimestamp::unix_millis))
        .checked_add(1)
        .ok_or(LiveOwnerFailure::Binding)?
        .max(last.map_or(0, AgentRunTimestamp::unix_millis));
    if current >= deadline {
        return Err(LiveOwnerFailure::Deadline);
    }
    AgentRunTimestamp::from_unix_millis(current).map_err(|_| LiveOwnerFailure::Binding)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawReviewDecision {
    version: u32,
    owner_nonce: String,
    binding_sha256: String,
    reviewer: String,
    allow: bool,
}

fn lowercase_hex(text: &str, size: usize) -> bool {
    text.len() == size
        && text
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// Validate a bounded Root decision against the SHA256 of the exact create-new
/// review-ready bytes (which bind all source/worker/Goal/Run/patch/script identities).
/// This acknowledgment is NOT a product grant or successful verification evidence.
/// The normal exact policy/grant path and current source revalidation still follow.
pub(super) fn validate_review_decision(
    bytes: &[u8],
    expected_nonce: &str,
    expected_binding_sha256: &str,
) -> Result<String, LiveOwnerFailure> {
    if bytes.len() > MAX_REVIEW_BYTES
        || bytes.iter().copied().find(|b| !b.is_ascii_whitespace()) != Some(b'{')
        || !lowercase_hex(expected_nonce, 32)
        || !lowercase_hex(expected_binding_sha256, 64)
    {
        return Err(LiveOwnerFailure::InvalidReview);
    }
    // Struct decoding rejects duplicate known keys and unknown keys; the leading
    // object check separately denies serde's sequence representation of structs.
    let value: RawReviewDecision =
        serde_json::from_slice(bytes).map_err(|_| LiveOwnerFailure::InvalidReview)?;
    if value.version != 1
        || value.owner_nonce != expected_nonce
        || value.binding_sha256 != expected_binding_sha256
        || value.reviewer.is_empty()
        || value.reviewer.len() > 128
        || !value
            .reviewer
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"/:_.-".contains(&b))
    {
        return Err(LiveOwnerFailure::InvalidReview);
    }
    if !value.allow {
        return Err(LiveOwnerFailure::ReviewDenied);
    }
    Ok(value.reviewer)
}

#[cfg(test)]
mod tests {
    use super::*;
    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn at(n: u64) -> Result<AgentRunTimestamp, a3_domain::AgentRunTimestampError> {
        AgentRunTimestamp::from_unix_millis(n)
    }

    #[test]
    fn owner_floor_counts_phase_and_review_gaps_without_refreshing_deadline() -> TestResult {
        assert_eq!(
            logical_floor(at(3)?, at(70)?, Some(at(5000)?), None, 10, OWNER_LIMIT_MS),
            Ok(at(5001)?)
        );
        assert_eq!(
            logical_floor(
                at(3)?,
                at(70)?,
                Some(at(5000)?),
                Some(at(5001)?),
                600_000,
                OWNER_LIMIT_MS
            ),
            Ok(at(600_004)?)
        );
        assert_eq!(
            logical_floor(
                at(3)?,
                at(70)?,
                Some(at(5000)?),
                None,
                900_000,
                OWNER_LIMIT_MS
            ),
            Err(LiveOwnerFailure::Deadline)
        );
        assert_eq!(
            logical_floor(
                at(3)?,
                at(70)?,
                Some(at(900_003)?),
                None,
                10,
                OWNER_LIMIT_MS
            ),
            Err(LiveOwnerFailure::Deadline)
        );
        Ok(())
    }

    #[test]
    fn review_decision_requires_exact_current_owner_binding_and_object() -> TestResult {
        let nonce = "a".repeat(32);
        let binding = "b".repeat(64);
        let valid = serde_json::json!({"version":1,"owner_nonce":nonce,"binding_sha256":binding,
            "reviewer":"subagent:/root/live_review","allow":true});
        let bytes = serde_json::to_vec(&valid)?;
        assert_eq!(
            validate_review_decision(&bytes, &nonce, &binding),
            Ok("subagent:/root/live_review".to_owned())
        );
        assert_eq!(
            validate_review_decision(&bytes, &"c".repeat(32), &binding),
            Err(LiveOwnerFailure::InvalidReview)
        );
        assert_eq!(
            validate_review_decision(b"[1,2,3]", &nonce, &binding),
            Err(LiveOwnerFailure::InvalidReview)
        );
        let duplicate = format!("{{\"version\":1,{}", String::from_utf8_lossy(&bytes[1..]));
        assert_eq!(
            validate_review_decision(duplicate.as_bytes(), &nonce, &binding),
            Err(LiveOwnerFailure::InvalidReview)
        );
        let mut denied = valid;
        denied["allow"] = serde_json::json!(false);
        let bytes = serde_json::to_vec(&denied)?;
        assert_eq!(
            validate_review_decision(&bytes, &nonce, &binding),
            Err(LiveOwnerFailure::ReviewDenied)
        );
        Ok(())
    }
}
