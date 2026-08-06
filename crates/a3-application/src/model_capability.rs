use crate::{ModelOperationControl, ModelProviderFailure, ModelRequestTimeout};
use a3_domain::{
    ModelCapabilities, ModelContextLimit, ModelId, ModelProfile, ModelProfileSettings,
    ModelProviderId,
};
use std::error::Error;
use std::fmt;
use std::future::Future;
use std::pin::Pin;

/// Future returned by the object-safe provider capability-probe port.
pub type ModelCapabilityProbeFuture<'a> = Pin<
    Box<dyn Future<Output = Result<ModelCapabilityObservation, ModelProviderFailure>> + Send + 'a>,
>;

/// Provider-reported maximum context window before profile-policy validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct ReportedModelContextLimit(u32);

impl ReportedModelContextLimit {
    /// Accepts one non-zero provider-reported context limit.
    pub const fn new(value: u32) -> Result<Self, ReportedModelContextLimitError> {
        if value == 0 {
            Err(ReportedModelContextLimitError)
        } else {
            Ok(Self(value))
        }
    }

    /// Returns the provider-reported token count.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// Provider metadata reported an unusable zero-sized context window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReportedModelContextLimitError;

impl fmt::Display for ReportedModelContextLimitError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("provider-reported model context limit must be non-zero")
    }
}

impl Error for ReportedModelContextLimitError {}

/// Bounded provider-neutral inputs for one startup capability probe.
#[derive(Clone, PartialEq, Eq)]
pub struct ModelCapabilityProbeRequest {
    model_id: ModelId,
    settings: ModelProfileSettings,
}

impl ModelCapabilityProbeRequest {
    /// Combines an opaque provider-native model identity with validated requested settings.
    #[must_use]
    pub const fn new(model_id: ModelId, settings: ModelProfileSettings) -> Self {
        Self { model_id, settings }
    }

    /// Returns the model identity selected by the user or composition root.
    #[must_use]
    pub const fn model_id(&self) -> &ModelId {
        &self.model_id
    }

    /// Returns the requested run-shaping settings used by the probe and resulting profile.
    #[must_use]
    pub const fn settings(&self) -> &ModelProfileSettings {
        &self.settings
    }
}

impl fmt::Debug for ModelCapabilityProbeRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ModelCapabilityProbeRequest")
            .field("model_id", &self.model_id)
            .field("context_limit", &self.settings.context_limit())
            .field("output_limit", &self.settings.output_limit())
            .field("parallelism_limit", &self.settings.parallelism_limit())
            .finish_non_exhaustive()
    }
}

/// Provider-neutral capability evidence collected without model-name inference.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelCapabilityObservation {
    reported_context_limit: Option<ReportedModelContextLimit>,
    capabilities: ModelCapabilities,
}

impl ModelCapabilityObservation {
    /// Records independently optional provider metadata and live capability evidence.
    #[must_use]
    pub const fn new(
        reported_context_limit: Option<ReportedModelContextLimit>,
        capabilities: ModelCapabilities,
    ) -> Self {
        Self {
            reported_context_limit,
            capabilities,
        }
    }

    /// Returns the provider-reported maximum context window when trustworthy metadata existed.
    #[must_use]
    pub const fn reported_context_limit(self) -> Option<ReportedModelContextLimit> {
        self.reported_context_limit
    }

    /// Returns the actual structured-output result and provider-reported tool mode.
    #[must_use]
    pub const fn capabilities(self) -> ModelCapabilities {
        self.capabilities
    }
}

/// Application-owned capability boundary implemented by concrete provider adapters.
pub trait ModelCapabilityProbe: fmt::Debug + Send + Sync {
    /// Returns the stable provider identity without endpoint or credential data.
    fn provider_id(&self) -> &ModelProviderId;

    /// Performs one bounded live probe and returns evidence rather than name-based guesses.
    fn probe<'a>(
        &'a self,
        request: &'a ModelCapabilityProbeRequest,
        timeout: ModelRequestTimeout,
        control: &'a dyn ModelOperationControl,
    ) -> ModelCapabilityProbeFuture<'a>;
}

/// Creates a complete versioned profile from one concrete capability adapter.
#[derive(Debug)]
pub struct ProbeModelProfile<'a> {
    probe: &'a dyn ModelCapabilityProbe,
}

impl<'a> ProbeModelProfile<'a> {
    /// Binds the use case to a provider adapter supplied by the composition root.
    #[must_use]
    pub const fn new(probe: &'a dyn ModelCapabilityProbe) -> Self {
        Self { probe }
    }

    /// Probes the selected model, enforces reported limits, and creates its V1 profile.
    pub async fn execute(
        &self,
        request: &ModelCapabilityProbeRequest,
        timeout: ModelRequestTimeout,
        control: &dyn ModelOperationControl,
    ) -> Result<ModelProfile, ProbeModelProfileFailure> {
        let observation = self
            .probe
            .probe(request, timeout, control)
            .await
            .map_err(ProbeModelProfileFailure::Provider)?;
        enforce_reported_context_limit(
            request.settings().context_limit(),
            observation.reported_context_limit(),
        )?;
        Ok(ModelProfile::from_probe(
            self.probe.provider_id().clone(),
            request.model_id().clone(),
            request.settings().clone(),
            observation.capabilities(),
        ))
    }
}

fn enforce_reported_context_limit(
    requested: ModelContextLimit,
    reported: Option<ReportedModelContextLimit>,
) -> Result<(), ProbeModelProfileFailure> {
    if let Some(reported) = reported
        && requested.get() > reported.get()
    {
        return Err(ProbeModelProfileFailure::ContextLimitExceedsProvider {
            requested,
            reported,
        });
    }
    Ok(())
}

/// Capability probing failed or contradicted the configured context limit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeModelProfileFailure {
    /// The provider probe failed at its normalized adapter boundary.
    Provider(ModelProviderFailure),
    /// Requested effective context exceeded explicit provider metadata.
    ContextLimitExceedsProvider {
        /// User-selected or default effective context window.
        requested: ModelContextLimit,
        /// Provider-reported maximum context window.
        reported: ReportedModelContextLimit,
    },
}

impl fmt::Display for ProbeModelProfileFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Provider(error) => error.fmt(formatter),
            Self::ContextLimitExceedsProvider {
                requested,
                reported,
            } => write!(
                formatter,
                "requested model context {} exceeds provider-reported maximum {}",
                requested.get(),
                reported.get()
            ),
        }
    }
}

impl Error for ProbeModelProfileFailure {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Provider(error) => Some(error),
            Self::ContextLimitExceedsProvider { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ReportedModelContextLimit, enforce_reported_context_limit};
    use a3_domain::ModelContextLimit;

    #[test]
    fn provider_context_metadata_is_non_zero_and_never_silently_exceeded()
    -> Result<(), Box<dyn std::error::Error>> {
        assert!(ReportedModelContextLimit::new(0).is_err());
        assert!(
            enforce_reported_context_limit(
                ModelContextLimit::new(16_384)?,
                Some(ReportedModelContextLimit::new(32_768)?),
            )
            .is_ok()
        );
        assert!(
            enforce_reported_context_limit(
                ModelContextLimit::new(32_768)?,
                Some(ReportedModelContextLimit::new(16_384)?),
            )
            .is_err()
        );
        Ok(())
    }
}
