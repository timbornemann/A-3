//! Content-free projection and conservative capacity check for an actual encoded request.

use crate::{ModelProviderFailure, ModelProviderUsage, ModelRequestTimeout};
use a3_domain::{AgentRunTimestamp, ContentHash, ModelProfile};
use std::{error::Error, fmt, time::Instant};

const MAX_WIRE_BYTES: usize = 16 * 1024 * 1024;
const PHASE_PROTOCOL_RESERVE: u32 = 128;

pub(crate) fn bounded_phase_timeout(
    requested: ModelRequestTimeout,
    deadline: AgentRunTimestamp,
    now: AgentRunTimestamp,
) -> Option<ModelRequestTimeout> {
    let requested_millis = u64::try_from(requested.duration().as_millis()).ok()?;
    let remaining = deadline.unix_millis().checked_sub(now.unix_millis())?;
    ModelRequestTimeout::from_millis(remaining.min(requested_millis)).ok()
}

pub(crate) fn phase_elapsed_timestamp(
    started: Instant,
    at: AgentRunTimestamp,
) -> Option<AgentRunTimestamp> {
    let elapsed = u64::try_from(started.elapsed().as_millis()).ok()?;
    AgentRunTimestamp::from_unix_millis(at.unix_millis().checked_add(elapsed)?).ok()
}

/// Whether the encoded request permits reasoning output in addition to the phase result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelWireThinking {
    /// The transport explicitly disables thinking.
    Disabled,
    /// The transport permits thinking and needs a separately verified accounting contract.
    Enabled,
}

/// Identity and bounds of the bytes the provider adapter will actually send.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelWireProjection {
    digest: ContentHash,
    request_bytes: u32,
    message_bytes: u32,
    format_bytes: u32,
    context_tokens: u32,
    output_tokens: u32,
    thinking: ModelWireThinking,
}

impl ModelWireProjection {
    /// Derives content-free identity from the exact encoded body, with separately measured fields.
    pub fn from_serialized(
        body: &[u8],
        message_utf8_bytes: usize,
        format_utf8_bytes: usize,
        context_tokens: u32,
        output_tokens: u32,
        thinking: ModelWireThinking,
    ) -> Result<Self, ModelProviderFailure> {
        if body.is_empty()
            || body.len() > MAX_WIRE_BYTES
            || message_utf8_bytes.saturating_add(format_utf8_bytes) > body.len()
            || output_tokens == 0
            || output_tokens >= context_tokens
        {
            return Err(ModelProviderFailure::Rejected);
        }
        let bounded = |bytes| u32::try_from(bytes).map_err(|_| ModelProviderFailure::Rejected);
        Ok(Self {
            digest: ContentHash::from_bytes(*blake3::hash(body).as_bytes()),
            request_bytes: bounded(body.len())?,
            message_bytes: bounded(message_utf8_bytes)?,
            format_bytes: bounded(format_utf8_bytes)?,
            context_tokens,
            output_tokens,
            thinking,
        })
    }

    /// Returns the digest of the complete transmitted body.
    #[must_use]
    pub const fn digest(self) -> ContentHash {
        self.digest
    }

    /// Returns the complete encoded request byte count.
    #[must_use]
    pub const fn request_bytes(self) -> u32 {
        self.request_bytes
    }

    /// Returns message text bytes, including any schema repeated inside those messages.
    #[must_use]
    pub const fn message_bytes(self) -> u32 {
        self.message_bytes
    }

    /// Returns the encoded standalone format-field schema bytes.
    #[must_use]
    pub const fn format_bytes(self) -> u32 {
        self.format_bytes
    }

    /// Returns the actual requested context limit.
    #[must_use]
    pub const fn context_tokens(self) -> u32 {
        self.context_tokens
    }

    /// Returns the actual requested generation limit.
    #[must_use]
    pub const fn output_tokens(self) -> u32 {
        self.output_tokens
    }

    /// Returns the actual thinking setting.
    #[must_use]
    pub const fn thinking(self) -> ModelWireThinking {
        self.thinking
    }
}

/// Conservative phase reservation in token units before any transport is started.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PhaseWireBudget {
    prompt_tokens: u32,
    output_tokens: u32,
}

impl PhaseWireBudget {
    /// Charges every serialized byte and reserves template, safety and output headroom.
    pub fn fit(
        projection: ModelWireProjection,
        profile: &ModelProfile,
    ) -> Result<Self, PhaseWireBudgetError> {
        let settings = profile.settings();
        let context = projection.context_tokens();
        if !profile.executable_actions_enabled()
            || context > settings.context_limit().get()
            || projection.output_tokens() != settings.output_limit().get()
            || projection.format_bytes() == 0
            || projection.thinking() != ModelWireThinking::Disabled
        {
            return Err(PhaseWireBudgetError::UnverifiedContract);
        }
        let safety = context.div_ceil(16);
        let minimum_output = u32::try_from((u64::from(context) * 22).div_ceil(100))
            .map_err(|_| PhaseWireBudgetError::Capacity)?;
        let output_tokens = minimum_output.max(projection.output_tokens());
        let prompt_tokens = projection
            .request_bytes()
            .checked_add(PHASE_PROTOCOL_RESERVE)
            .and_then(|total| total.checked_add(safety))
            .ok_or(PhaseWireBudgetError::Capacity)?;
        if prompt_tokens
            .checked_add(output_tokens)
            .is_none_or(|total| total > context)
        {
            return Err(PhaseWireBudgetError::Capacity);
        }
        Ok(Self {
            prompt_tokens,
            output_tokens,
        })
    }

    /// Returns the charged prompt and framing allowance.
    #[must_use]
    pub const fn prompt_tokens(self) -> u32 {
        self.prompt_tokens
    }

    /// Returns the greater of requested generation and the 22 percent reserve.
    #[must_use]
    pub const fn output_tokens(self) -> u32 {
        self.output_tokens
    }

    pub(crate) fn accepts_usage(
        self,
        projection: ModelWireProjection,
        usage: ModelProviderUsage,
    ) -> bool {
        let (Some(prompt), Some(output)) = (usage.prompt_tokens(), usage.output_tokens()) else {
            return false;
        };
        prompt > 0
            && output > 0
            && prompt <= u64::from(self.prompt_tokens)
            && output <= u64::from(projection.output_tokens())
            && prompt + output <= u64::from(projection.context_tokens())
    }
}

/// Why this encoded phase cannot be dispatched.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhaseWireBudgetError {
    /// No verified structured output and thinking contract matches the projected request.
    UnverifiedContract,
    /// Complete request and reserves exceed the actual encoded context limit.
    Capacity,
}

impl fmt::Display for PhaseWireBudgetError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::UnverifiedContract => "phase wire options do not match the verified profile",
            Self::Capacity => "complete phase request exceeds the context capacity",
        })
    }
}

impl Error for PhaseWireBudgetError {}

#[cfg(test)]
mod tests {
    use super::*;
    use a3_domain::{
        ModelCapabilities, ModelContextLimit, ModelId, ModelOutputLimit, ModelParallelismLimit,
        ModelProfileSettings, ModelPromptSchemaGrounding, ModelProviderId, ModelSamplingProfile,
        ModelStopSequences, ModelStructuredOutputCapability, ModelTemperature,
        ModelTokenCountingStrategy, ModelToolCallMode, ModelTopP,
    };

    type TestResult<T = ()> = Result<T, Box<dyn Error>>;

    fn profile(output: u32) -> TestResult<ModelProfile> {
        Ok(ModelProfile::from_probe(
            ModelProviderId::try_from_string("ollama".to_owned())?,
            ModelId::try_from_string("qwen38-4k:latest".to_owned())?,
            ModelProfileSettings::new(
                ModelContextLimit::new(4096)?,
                ModelOutputLimit::new(output)?,
                ModelTokenCountingStrategy::ConservativeUtf8BytesV1,
                ModelParallelismLimit::new(1)?,
                ModelSamplingProfile::new(
                    ModelTemperature::from_milli(0)?,
                    ModelTopP::from_milli(1000)?,
                ),
                ModelStopSequences::empty(),
                ModelPromptSchemaGrounding::FormatFieldOnly,
            )?,
            ModelCapabilities::new(
                ModelStructuredOutputCapability::Verified,
                ModelToolCallMode::Disabled,
            ),
        ))
    }

    fn project(
        bytes: usize,
        output: u32,
        thinking: ModelWireThinking,
    ) -> TestResult<ModelWireProjection> {
        Ok(ModelWireProjection::from_serialized(
            &vec![b'x'; bytes],
            900,
            400,
            4096,
            output,
            thinking,
        )?)
    }

    #[test]
    fn encoded_body_plus_all_reserves_must_fit_the_actual_four_k_limit() -> TestResult {
        let profile = profile(1024)?;
        let exact = project(2688, 1024, ModelWireThinking::Disabled)?;
        let budget = PhaseWireBudget::fit(exact, &profile)?;
        assert_eq!(budget.prompt_tokens(), 3072);
        assert_eq!(budget.output_tokens(), 1024);
        assert_eq!(
            PhaseWireBudget::fit(project(2689, 1024, ModelWireThinking::Disabled)?, &profile),
            Err(PhaseWireBudgetError::Capacity)
        );
        assert_eq!(
            PhaseWireBudget::fit(project(2000, 1024, ModelWireThinking::Enabled)?, &profile),
            Err(PhaseWireBudgetError::UnverifiedContract)
        );
        Ok(())
    }

    #[test]
    fn configured_small_generation_does_not_shrink_the_twenty_two_percent_reserve() -> TestResult {
        let profile = profile(128)?;
        let exact =
            PhaseWireBudget::fit(project(2810, 128, ModelWireThinking::Disabled)?, &profile)?;
        assert_eq!(exact.prompt_tokens() + exact.output_tokens(), 4096);
        assert_eq!(exact.output_tokens(), 902);
        assert_eq!(
            PhaseWireBudget::fit(project(2811, 128, ModelWireThinking::Disabled)?, &profile),
            Err(PhaseWireBudgetError::Capacity)
        );
        Ok(())
    }

    #[test]
    fn actual_usage_needs_both_positive_counts_and_respects_configured_generation() -> TestResult {
        let profile = profile(128)?;
        let projection = project(2500, 128, ModelWireThinking::Disabled)?;
        let budget = PhaseWireBudget::fit(projection, &profile)?;
        assert!(budget.accepts_usage(projection, ModelProviderUsage::new(Some(880), Some(128))));
        for usage in [
            ModelProviderUsage::new(None, Some(128)),
            ModelProviderUsage::new(Some(880), None),
            ModelProviderUsage::new(Some(0), Some(128)),
            ModelProviderUsage::new(Some(880), Some(0)),
            ModelProviderUsage::new(Some(880), Some(129)),
            ModelProviderUsage::new(Some(u64::MAX), Some(128)),
        ] {
            assert!(!budget.accepts_usage(projection, usage));
        }
        Ok(())
    }
}
