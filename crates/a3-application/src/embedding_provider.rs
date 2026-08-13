use crate::{JobContext, ModelOperationControl, ModelProviderFailure, ModelRequestTimeout};
use a3_domain::{
    EmbeddingBatchSize, EmbeddingDimension, EmbeddingModelId, EmbeddingModelProfile,
    EmbeddingProviderId, NormalizedSemanticCard,
};
use std::error::Error;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

const MAX_RAW_VECTORS_PER_RESPONSE: usize = 64;
const MAX_RAW_VECTOR_DIMENSION: usize = 8_192;
const MAX_EMBEDDING_REQUEST_TIMEOUT_MILLIS: u64 = 120_000;

/// Owned future returned by the object-safe embedding-provider port.
pub type EmbeddingProviderFuture<'a> =
    Pin<Box<dyn Future<Output = Result<RawEmbeddingBatch, EmbeddingProviderFailure>> + Send + 'a>>;

/// Future returned by a bounded provider-owned embedding capability probe.
pub type EmbeddingCapabilityProbeFuture<'a> =
    Pin<Box<dyn Future<Output = Result<EmbeddingDimension, ModelProviderFailure>> + Send + 'a>>;

/// Provider-neutral input for one real embedding dimension probe.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmbeddingCapabilityProbeRequest {
    model_id: EmbeddingModelId,
    max_batch_size: EmbeddingBatchSize,
}

impl EmbeddingCapabilityProbeRequest {
    /// Binds an opaque provider model identity to its user-selected local batch limit.
    #[must_use]
    pub const fn new(model_id: EmbeddingModelId, max_batch_size: EmbeddingBatchSize) -> Self {
        Self {
            model_id,
            max_batch_size,
        }
    }

    /// Returns the provider-native model identity.
    #[must_use]
    pub const fn model_id(&self) -> &EmbeddingModelId {
        &self.model_id
    }

    /// Returns the bounded operational batch limit retained by the resulting profile.
    #[must_use]
    pub const fn max_batch_size(&self) -> EmbeddingBatchSize {
        self.max_batch_size
    }
}

/// Application-owned boundary for a real provider embedding-dimension observation.
pub trait EmbeddingCapabilityProbe: fmt::Debug + Send + Sync {
    /// Returns the stable provider identity without endpoint or credentials.
    fn provider_id(&self) -> &EmbeddingProviderId;

    /// Submits one fixed bounded probe input and returns only its validated dimension.
    fn probe_embedding<'a>(
        &'a self,
        request: &'a EmbeddingCapabilityProbeRequest,
        timeout: ModelRequestTimeout,
        control: &'a dyn ModelOperationControl,
    ) -> EmbeddingCapabilityProbeFuture<'a>;
}

/// Creates the embedding-specific profile only from one live dimension observation.
#[derive(Debug)]
pub struct ProbeEmbeddingModelProfile<'a> {
    probe: &'a dyn EmbeddingCapabilityProbe,
}

impl<'a> ProbeEmbeddingModelProfile<'a> {
    /// Binds the use case to a concrete provider adapter supplied by composition.
    #[must_use]
    pub const fn new(probe: &'a dyn EmbeddingCapabilityProbe) -> Self {
        Self { probe }
    }

    /// Performs the bounded live probe and derives a vector-isolated V1 profile.
    pub async fn execute(
        &self,
        request: &EmbeddingCapabilityProbeRequest,
        timeout: ModelRequestTimeout,
        control: &dyn ModelOperationControl,
    ) -> Result<EmbeddingModelProfile, ModelProviderFailure> {
        let dimension = self
            .probe
            .probe_embedding(request, timeout, control)
            .await?;
        Ok(EmbeddingModelProfile::v1(
            self.probe.provider_id().clone(),
            request.model_id().clone(),
            dimension,
            request.max_batch_size(),
        ))
    }
}

/// Cooperative cancellation visible to provider and storage adapters.
pub trait EmbeddingOperationControl: fmt::Debug + Send + Sync {
    /// Returns whether the owning job requested cancellation.
    fn is_cancelled(&self) -> bool;
}

impl EmbeddingOperationControl for JobContext {
    fn is_cancelled(&self) -> bool {
        self.cancellation_token().is_cancelled()
    }
}

/// Positive per-request timeout enforced by each concrete provider adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EmbeddingRequestTimeout(Duration);

impl EmbeddingRequestTimeout {
    /// Default local embedding-provider deadline.
    pub const DEFAULT: Self = Self(Duration::from_secs(30));

    /// Creates a non-zero timeout capped at two minutes.
    pub fn from_millis(value: u64) -> Result<Self, EmbeddingRequestTimeoutError> {
        if value == 0 || value > MAX_EMBEDDING_REQUEST_TIMEOUT_MILLIS {
            return Err(EmbeddingRequestTimeoutError { value });
        }
        Ok(Self(Duration::from_millis(value)))
    }

    /// Returns the provider-neutral duration.
    #[must_use]
    pub const fn duration(self) -> Duration {
        self.0
    }
}

/// Embedding request timeout outside the fixed local boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EmbeddingRequestTimeoutError {
    value: u64,
}

impl fmt::Display for EmbeddingRequestTimeoutError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "embedding request timeout {} ms must be between 1 and {MAX_EMBEDDING_REQUEST_TIMEOUT_MILLIS}",
            self.value
        )
    }
}

impl Error for EmbeddingRequestTimeoutError {}

/// Bounded raw float vectors returned by a provider before domain validation.
#[derive(Clone, PartialEq)]
pub struct RawEmbeddingBatch {
    vectors: Vec<Vec<f32>>,
}

impl RawEmbeddingBatch {
    /// Applies allocation boundaries before provider output enters orchestration.
    pub fn new(vectors: Vec<Vec<f32>>) -> Result<Self, RawEmbeddingBatchError> {
        if vectors.len() > MAX_RAW_VECTORS_PER_RESPONSE {
            return Err(RawEmbeddingBatchError::TooManyVectors {
                actual: vectors.len(),
            });
        }
        if let Some(actual) = vectors
            .iter()
            .map(Vec::len)
            .find(|dimension| *dimension > MAX_RAW_VECTOR_DIMENSION)
        {
            return Err(RawEmbeddingBatchError::VectorTooLarge { actual });
        }
        Ok(Self { vectors })
    }

    /// Moves bounded raw vectors into the validating application use case.
    #[must_use]
    pub fn into_vectors(self) -> Vec<Vec<f32>> {
        self.vectors
    }

    /// Returns the bounded response cardinality without exposing components.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.vectors.len()
    }

    /// Returns whether the response contains no vectors.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.vectors.is_empty()
    }
}

impl fmt::Debug for RawEmbeddingBatch {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let dimensions = self.vectors.iter().map(Vec::len).collect::<Vec<_>>();
        formatter
            .debug_struct("RawEmbeddingBatch")
            .field("vector_count", &self.vectors.len())
            .field("dimensions", &dimensions)
            .finish()
    }
}

/// Provider response exceeded its pre-validation allocation boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RawEmbeddingBatchError {
    /// More vectors were returned than one provider request may contain.
    TooManyVectors {
        /// Observed vector count.
        actual: usize,
    },
    /// A vector exceeded the maximum supported dimension.
    VectorTooLarge {
        /// Observed component count.
        actual: usize,
    },
}

impl fmt::Display for RawEmbeddingBatchError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooManyVectors { actual } => write!(
                formatter,
                "provider returned {actual} vectors; maximum is {MAX_RAW_VECTORS_PER_RESPONSE}"
            ),
            Self::VectorTooLarge { actual } => write!(
                formatter,
                "provider returned a vector with {actual} components; maximum is {MAX_RAW_VECTOR_DIMENSION}"
            ),
        }
    }
}

impl Error for RawEmbeddingBatchError {}

/// Provider-neutral local embedding request boundary.
pub trait EmbeddingProvider: fmt::Debug + Send + Sync {
    /// Embeds one non-empty profile-bounded batch in input order.
    fn embed<'a>(
        &'a self,
        profile: &'a EmbeddingModelProfile,
        cards: &'a [NormalizedSemanticCard],
        timeout: EmbeddingRequestTimeout,
        control: &'a dyn EmbeddingOperationControl,
    ) -> EmbeddingProviderFuture<'a>;
}

/// Stable provider failure classification without payloads, endpoints, or credentials.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmbeddingProviderFailure {
    /// Configured local provider could not be reached.
    Unavailable,
    /// Provider rejected the validated profile or bounded request.
    Rejected,
    /// Provider response failed its bounded neutral schema.
    InvalidResponse,
    /// Provider enforced the request timeout.
    TimedOut,
    /// Provider observed cooperative cancellation.
    Cancelled,
}

impl fmt::Display for EmbeddingProviderFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Unavailable => "local embedding provider is unavailable",
            Self::Rejected => "local embedding provider rejected the request",
            Self::InvalidResponse => "local embedding provider returned an invalid response",
            Self::TimedOut => "local embedding provider request timed out",
            Self::Cancelled => "local embedding provider request was cancelled",
        })
    }
}

impl Error for EmbeddingProviderFailure {}

#[cfg(test)]
mod tests {
    use super::{EmbeddingRequestTimeout, RawEmbeddingBatch, RawEmbeddingBatchError};

    #[test]
    fn timeout_and_raw_response_are_bounded_and_redacted() {
        assert!(EmbeddingRequestTimeout::from_millis(0).is_err());
        assert!(EmbeddingRequestTimeout::from_millis(120_001).is_err());
        let batch =
            RawEmbeddingBatch::new(vec![vec![0.125, 0.25]]).map_err(|error| error.to_string());
        assert!(batch.is_ok());
        if let Ok(batch) = batch {
            assert!(!format!("{batch:?}").contains("0.125"));
        }
        assert_eq!(
            RawEmbeddingBatch::new(vec![vec![0.0; 8_193]]),
            Err(RawEmbeddingBatchError::VectorTooLarge { actual: 8_193 })
        );
    }
}
