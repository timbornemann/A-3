use super::{
    BodyHash, NormalizedRetrievalSignal, NormalizedSemanticCard, SemanticCardId, SnapshotId,
    SourceChannel,
};
use std::collections::BTreeSet;
use std::error::Error;
use std::fmt;

const MAX_PROVIDER_ID_BYTES: usize = 128;
const MAX_MODEL_ID_BYTES: usize = 512;
const MAX_EMBEDDING_DIMENSIONS: u16 = 8_192;
const MAX_EMBEDDING_BATCH_SIZE: u16 = 64;
const MAX_VECTOR_RESULTS: u16 = 100;
const MAX_PERSISTED_TIMESTAMP_MILLIS: u64 = i64::MAX as u64;

/// Stable compatibility identity derived from every vector-shaping profile field.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ModelProfileId([u8; 32]);

impl ModelProfileId {
    /// Reconstructs an ID after persisted profile metadata has been validated.
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Returns the canonical binary representation.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Display for ModelProfileId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_hex(formatter, &self.0)
    }
}

impl fmt::Debug for ModelProfileId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "ModelProfileId({self})")
    }
}

/// Validated provider identifier without endpoint or credential data.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EmbeddingProviderId(String);

impl EmbeddingProviderId {
    /// Validates a bounded provider identifier suitable for profile persistence.
    pub fn new(value: String) -> Result<Self, EmbeddingProfileTextError> {
        validate_profile_text(
            &value,
            MAX_PROVIDER_ID_BYTES,
            EmbeddingProfileTextKind::Provider,
        )?;
        Ok(Self(value))
    }

    /// Returns the provider-neutral identifier.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Validated provider-native model identifier treated only as opaque data.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EmbeddingModelId(String);

impl EmbeddingModelId {
    /// Validates a bounded model identifier without inferring capabilities from it.
    pub fn new(value: String) -> Result<Self, EmbeddingProfileTextError> {
        validate_profile_text(&value, MAX_MODEL_ID_BYTES, EmbeddingProfileTextKind::Model)?;
        Ok(Self(value))
    }

    /// Returns the opaque model identifier.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Profile text field rejected at the provider-neutral boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmbeddingProfileTextKind {
    /// Provider identifier.
    Provider,
    /// Provider-native model identifier.
    Model,
}

/// Invalid provider or model profile identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmbeddingProfileTextError {
    /// The identifier was empty or exceeded its UTF-8 byte boundary.
    InvalidLength {
        /// Rejected field.
        kind: EmbeddingProfileTextKind,
        /// Observed byte count.
        actual: usize,
    },
    /// The identifier contained a character outside its safe opaque grammar.
    UnsafeCharacter {
        /// Rejected field.
        kind: EmbeddingProfileTextKind,
    },
}

impl fmt::Display for EmbeddingProfileTextError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLength { kind, actual } => {
                write!(
                    formatter,
                    "embedding {kind} identifier has invalid length {actual}"
                )
            }
            Self::UnsafeCharacter { kind } => {
                write!(
                    formatter,
                    "embedding {kind} identifier contains an unsupported character"
                )
            }
        }
    }
}

impl Error for EmbeddingProfileTextError {}

impl fmt::Display for EmbeddingProfileTextKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Provider => "provider",
            Self::Model => "model",
        })
    }
}

/// Positive vector dimension bounded for local memory and persistence use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EmbeddingDimension(u16);

impl EmbeddingDimension {
    /// Validates a model-reported vector dimension.
    pub fn new(value: u16) -> Result<Self, EmbeddingDimensionError> {
        if value == 0 || value > MAX_EMBEDDING_DIMENSIONS {
            return Err(EmbeddingDimensionError { value });
        }
        Ok(Self(value))
    }

    /// Returns the number of float components.
    #[must_use]
    pub const fn get(self) -> u16 {
        self.0
    }
}

/// Embedding dimension outside the local resource boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EmbeddingDimensionError {
    value: u16,
}

impl fmt::Display for EmbeddingDimensionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "embedding dimension {} must be between 1 and {MAX_EMBEDDING_DIMENSIONS}",
            self.value
        )
    }
}

impl Error for EmbeddingDimensionError {}

/// Positive provider batch size bounded independently of the card-job size.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EmbeddingBatchSize(u16);

impl EmbeddingBatchSize {
    /// Validates the maximum number of cards in one provider request.
    pub fn new(value: u16) -> Result<Self, EmbeddingBatchSizeError> {
        if value == 0 || value > MAX_EMBEDDING_BATCH_SIZE {
            return Err(EmbeddingBatchSizeError { value });
        }
        Ok(Self(value))
    }

    /// Returns the provider request cardinality.
    #[must_use]
    pub const fn get(self) -> u16 {
        self.0
    }
}

/// Provider batch size outside the fixed local boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EmbeddingBatchSizeError {
    value: u16,
}

impl fmt::Display for EmbeddingBatchSizeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "embedding batch size {} must be between 1 and {MAX_EMBEDDING_BATCH_SIZE}",
            self.value
        )
    }
}

impl Error for EmbeddingBatchSizeError {}

/// Persisted component data type supported by the version-one cache.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum EmbeddingDataType {
    /// IEEE-754 32-bit floating point components.
    Float32,
}

/// Persisted vector quantization policy supported by version one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum EmbeddingQuantization {
    /// No lossy quantization is applied.
    None,
}

/// Deterministic normalization applied after validating provider output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum EmbeddingVectorNormalization {
    /// Normalize every non-zero finite vector to Euclidean unit length.
    L2Unit,
}

/// Embedding-only model capability profile used before the later general LLM profile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmbeddingModelProfile {
    id: ModelProfileId,
    provider_id: EmbeddingProviderId,
    model_id: EmbeddingModelId,
    dimension: EmbeddingDimension,
    data_type: EmbeddingDataType,
    quantization: EmbeddingQuantization,
    normalization: EmbeddingVectorNormalization,
    max_batch_size: EmbeddingBatchSize,
}

impl EmbeddingModelProfile {
    /// Creates the strict version-one float32 profile and derives its compatibility ID.
    #[must_use]
    pub fn v1(
        provider_id: EmbeddingProviderId,
        model_id: EmbeddingModelId,
        dimension: EmbeddingDimension,
        max_batch_size: EmbeddingBatchSize,
    ) -> Self {
        let data_type = EmbeddingDataType::Float32;
        let quantization = EmbeddingQuantization::None;
        let normalization = EmbeddingVectorNormalization::L2Unit;
        let id = derive_profile_id(
            &provider_id,
            &model_id,
            dimension,
            data_type,
            quantization,
            normalization,
        );
        Self {
            id,
            provider_id,
            model_id,
            dimension,
            data_type,
            quantization,
            normalization,
            max_batch_size,
        }
    }

    /// Returns the compatibility identity used to isolate cached vectors.
    #[must_use]
    pub const fn id(&self) -> ModelProfileId {
        self.id
    }

    /// Returns the provider identifier without endpoint or credential material.
    #[must_use]
    pub const fn provider_id(&self) -> &EmbeddingProviderId {
        &self.provider_id
    }

    /// Returns the opaque provider-native model identifier.
    #[must_use]
    pub const fn model_id(&self) -> &EmbeddingModelId {
        &self.model_id
    }

    /// Returns the required provider-output dimension.
    #[must_use]
    pub const fn dimension(&self) -> EmbeddingDimension {
        self.dimension
    }

    /// Returns the persisted component representation.
    #[must_use]
    pub const fn data_type(&self) -> EmbeddingDataType {
        self.data_type
    }

    /// Returns the persisted quantization policy.
    #[must_use]
    pub const fn quantization(&self) -> EmbeddingQuantization {
        self.quantization
    }

    /// Returns the deterministic post-provider normalization policy.
    #[must_use]
    pub const fn normalization(&self) -> EmbeddingVectorNormalization {
        self.normalization
    }

    /// Returns the maximum cards allowed in one provider request.
    #[must_use]
    pub const fn max_batch_size(&self) -> EmbeddingBatchSize {
        self.max_batch_size
    }

    /// Validates that persisted vector-shaping metadata exactly matches this profile ID.
    #[must_use]
    pub fn has_compatible_identity(&self) -> bool {
        self.id
            == derive_profile_id(
                &self.provider_id,
                &self.model_id,
                self.dimension,
                self.data_type,
                self.quantization,
                self.normalization,
            )
    }
}

/// Finite, non-zero and dimension-checked unit vector.
#[derive(Clone, PartialEq)]
pub struct EmbeddingVector {
    components: Vec<f32>,
}

impl Eq for EmbeddingVector {}

impl EmbeddingVector {
    /// Validates provider output and applies the profile's L2 normalization.
    pub fn normalize_l2(
        components: Vec<f32>,
        expected_dimension: EmbeddingDimension,
    ) -> Result<Self, EmbeddingVectorError> {
        if components.len() != usize::from(expected_dimension.get()) {
            return Err(EmbeddingVectorError::DimensionMismatch {
                expected: expected_dimension,
                actual: components.len(),
            });
        }
        if components.iter().any(|component| !component.is_finite()) {
            return Err(EmbeddingVectorError::NonFiniteComponent);
        }
        let squared_norm = components
            .iter()
            .map(|component| f64::from(*component).powi(2))
            .sum::<f64>();
        if !squared_norm.is_finite() || squared_norm == 0.0 {
            return Err(EmbeddingVectorError::ZeroOrInvalidNorm);
        }
        let divisor = squared_norm.sqrt();
        let normalized = components
            .into_iter()
            .map(|component| {
                let value = (f64::from(component) / divisor) as f32;
                if value == 0.0 { 0.0 } else { value }
            })
            .collect::<Vec<_>>();
        if normalized.iter().any(|component| !component.is_finite()) {
            return Err(EmbeddingVectorError::NonFiniteComponent);
        }
        Ok(Self {
            components: normalized,
        })
    }

    /// Returns the normalized float32 components for a trusted adapter boundary.
    #[must_use]
    pub fn components(&self) -> &[f32] {
        &self.components
    }

    /// Returns the already validated number of components.
    #[must_use]
    pub fn dimension(&self) -> usize {
        self.components.len()
    }
}

impl fmt::Debug for EmbeddingVector {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EmbeddingVector")
            .field("dimension", &self.components.len())
            .finish_non_exhaustive()
    }
}

/// Invalid raw provider vector.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmbeddingVectorError {
    /// Provider output did not match the explicit profile dimension.
    DimensionMismatch {
        /// Required profile dimension.
        expected: EmbeddingDimension,
        /// Observed component count.
        actual: usize,
    },
    /// At least one provider component was NaN or infinite.
    NonFiniteComponent,
    /// The vector could not be normalized because its norm was zero or invalid.
    ZeroOrInvalidNorm,
}

impl fmt::Display for EmbeddingVectorError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DimensionMismatch { expected, actual } => write!(
                formatter,
                "embedding vector has {actual} components; expected {}",
                expected.get()
            ),
            Self::NonFiniteComponent => {
                formatter.write_str("embedding vector contains a non-finite component")
            }
            Self::ZeroOrInvalidNorm => {
                formatter.write_str("embedding vector has a zero or invalid norm")
            }
        }
    }
}

impl Error for EmbeddingVectorError {}

/// Milliseconds since the Unix epoch supplied by an injected application clock.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EmbeddingTimestamp(u64);

impl EmbeddingTimestamp {
    /// Constructs a timestamp that fits the portable signed persistence representation.
    pub fn from_unix_millis(value: u64) -> Result<Self, EmbeddingTimestampError> {
        if value > MAX_PERSISTED_TIMESTAMP_MILLIS {
            return Err(EmbeddingTimestampError { value });
        }
        Ok(Self(value))
    }

    /// Returns the persisted millisecond representation.
    #[must_use]
    pub const fn as_unix_millis(self) -> u64 {
        self.0
    }
}

/// Embedding creation timestamp outside the portable persistence range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EmbeddingTimestampError {
    value: u64,
}

impl fmt::Display for EmbeddingTimestampError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "embedding timestamp {} exceeds {MAX_PERSISTED_TIMESTAMP_MILLIS} milliseconds",
            self.value
        )
    }
}

impl Error for EmbeddingTimestampError {}

/// Full regenerable semantic-cache record produced after provider validation.
#[derive(Clone, PartialEq, Eq)]
pub struct SemanticEmbedding {
    card: NormalizedSemanticCard,
    profile_id: ModelProfileId,
    vector: EmbeddingVector,
    created_at: EmbeddingTimestamp,
}

impl SemanticEmbedding {
    /// Validates and normalizes one raw provider vector against its exact profile.
    pub fn from_provider_output(
        card: NormalizedSemanticCard,
        profile: &EmbeddingModelProfile,
        components: Vec<f32>,
        created_at: EmbeddingTimestamp,
    ) -> Result<Self, EmbeddingVectorError> {
        let vector = EmbeddingVector::normalize_l2(components, profile.dimension())?;
        Ok(Self {
            card,
            profile_id: profile.id(),
            vector,
            created_at,
        })
    }

    /// Returns the normalized semantic card and its body revision.
    #[must_use]
    pub const fn card(&self) -> &NormalizedSemanticCard {
        &self.card
    }

    /// Returns the vector-compatibility profile identity.
    #[must_use]
    pub const fn profile_id(&self) -> ModelProfileId {
        self.profile_id
    }

    /// Returns the validated normalized vector.
    #[must_use]
    pub const fn vector(&self) -> &EmbeddingVector {
        &self.vector
    }

    /// Returns the injected creation timestamp.
    #[must_use]
    pub const fn created_at(&self) -> EmbeddingTimestamp {
        self.created_at
    }

    /// Returns the exact semantic-cache key.
    #[must_use]
    pub fn cache_key(&self) -> EmbeddingCacheKey {
        EmbeddingCacheKey::new(self.card.id(), self.profile_id, self.card.body_hash())
    }
}

impl fmt::Debug for SemanticEmbedding {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SemanticEmbedding")
            .field("card_id", &self.card.id())
            .field("body_hash", &self.card.body_hash())
            .field("profile_id", &self.profile_id)
            .field("dimension", &self.vector.dimension())
            .field("created_at", &self.created_at)
            .finish()
    }
}

/// Exact key preventing card-body or profile mixing in the semantic cache.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EmbeddingCacheKey {
    card_id: SemanticCardId,
    profile_id: ModelProfileId,
    body_hash: BodyHash,
}

impl EmbeddingCacheKey {
    /// Creates a key from already validated immutable identities.
    #[must_use]
    pub const fn new(
        card_id: SemanticCardId,
        profile_id: ModelProfileId,
        body_hash: BodyHash,
    ) -> Self {
        Self {
            card_id,
            profile_id,
            body_hash,
        }
    }

    /// Derives the exact key for a normalized card and profile.
    #[must_use]
    pub fn from_card(card: &NormalizedSemanticCard, profile: &EmbeddingModelProfile) -> Self {
        Self::new(card.id(), profile.id(), card.body_hash())
    }

    /// Returns the logical card identity.
    #[must_use]
    pub const fn card_id(self) -> SemanticCardId {
        self.card_id
    }

    /// Returns the vector compatibility profile identity.
    #[must_use]
    pub const fn profile_id(self) -> ModelProfileId {
        self.profile_id
    }

    /// Returns the normalized body revision.
    #[must_use]
    pub const fn body_hash(self) -> BodyHash {
        self.body_hash
    }
}

/// Optional libSQL vector search mode available for one local store.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum VectorSearchCapability {
    /// A native libSQL vector index can generate candidates.
    Indexed,
    /// Valid vectors can be compared through a bounded deterministic linear fallback.
    LinearFallback,
}

/// Positive semantic result boundary capped independently of provider batches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct VectorSearchLimit(u16);

impl VectorSearchLimit {
    /// Default number of semantic candidates supplied to later fusion.
    pub const DEFAULT: Self = Self(20);

    /// Validates the bounded result count.
    pub fn new(value: u16) -> Result<Self, VectorSearchLimitError> {
        if value == 0 || value > MAX_VECTOR_RESULTS {
            return Err(VectorSearchLimitError { value });
        }
        Ok(Self(value))
    }

    /// Returns the bounded primitive representation.
    #[must_use]
    pub const fn get(self) -> u16 {
        self.0
    }
}

/// Semantic result limit outside the fixed interactive boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VectorSearchLimitError {
    value: u16,
}

impl fmt::Display for VectorSearchLimitError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "vector search limit {} must be between 1 and {MAX_VECTOR_RESULTS}",
            self.value
        )
    }
}

impl Error for VectorSearchLimitError {}

/// Similarity-only candidate that deliberately cannot carry an EvidenceRef.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VectorHit {
    card_id: SemanticCardId,
    body_hash: BodyHash,
    profile_id: ModelProfileId,
    similarity: NormalizedRetrievalSignal,
}

impl VectorHit {
    /// Creates one non-evidentiary vector candidate after adapter score normalization.
    #[must_use]
    pub const fn new(
        card_id: SemanticCardId,
        body_hash: BodyHash,
        profile_id: ModelProfileId,
        similarity: NormalizedRetrievalSignal,
    ) -> Self {
        Self {
            card_id,
            body_hash,
            profile_id,
            similarity,
        }
    }

    /// Returns the only retrieval channel this hit may enter.
    #[must_use]
    pub const fn source_channel(self) -> SourceChannel {
        SourceChannel::Semantic
    }

    /// Returns the semantic card identity, not a factual evidence identity.
    #[must_use]
    pub const fn card_id(self) -> SemanticCardId {
        self.card_id
    }

    /// Returns the exact normalized body revision compared by the vector engine.
    #[must_use]
    pub const fn body_hash(self) -> BodyHash {
        self.body_hash
    }

    /// Returns the isolated vector compatibility profile.
    #[must_use]
    pub const fn profile_id(self) -> ModelProfileId {
        self.profile_id
    }

    /// Returns the normalized candidate similarity.
    #[must_use]
    pub const fn similarity(self) -> NormalizedRetrievalSignal {
        self.similarity
    }
}

/// Stable non-evidentiary semantic candidates for one snapshot and profile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VectorSearchResult {
    snapshot_id: SnapshotId,
    profile_id: ModelProfileId,
    capability: VectorSearchCapability,
    limit: VectorSearchLimit,
    hits: Vec<VectorHit>,
    truncated: bool,
}

impl VectorSearchResult {
    /// Canonicalizes hits by similarity and stable card revision while rejecting mixed profiles.
    pub fn new(
        snapshot_id: SnapshotId,
        profile_id: ModelProfileId,
        capability: VectorSearchCapability,
        limit: VectorSearchLimit,
        mut hits: Vec<VectorHit>,
        truncated: bool,
    ) -> Result<Self, VectorSearchResultError> {
        if hits.len() > usize::from(limit.get()) {
            return Err(VectorSearchResultError::TooManyHits);
        }
        if hits.iter().any(|hit| hit.profile_id() != profile_id) {
            return Err(VectorSearchResultError::ProfileMismatch);
        }
        let mut revisions = BTreeSet::new();
        if hits
            .iter()
            .any(|hit| !revisions.insert((hit.card_id(), hit.body_hash())))
        {
            return Err(VectorSearchResultError::DuplicateCardRevision);
        }
        hits.sort_by(|left, right| {
            right
                .similarity()
                .cmp(&left.similarity())
                .then_with(|| left.card_id().cmp(&right.card_id()))
                .then_with(|| left.body_hash().cmp(&right.body_hash()))
        });
        Ok(Self {
            snapshot_id,
            profile_id,
            capability,
            limit,
            hits,
            truncated,
        })
    }

    /// Returns the snapshot requested from the semantic cache.
    #[must_use]
    pub const fn snapshot_id(&self) -> SnapshotId {
        self.snapshot_id
    }

    /// Returns the sole vector compatibility profile represented by every hit.
    #[must_use]
    pub const fn profile_id(&self) -> ModelProfileId {
        self.profile_id
    }

    /// Returns whether native indexing or the bounded fallback generated candidates.
    #[must_use]
    pub const fn capability(&self) -> VectorSearchCapability {
        self.capability
    }

    /// Returns the requested maximum number of semantic candidates.
    #[must_use]
    pub const fn limit(&self) -> VectorSearchLimit {
        self.limit
    }

    /// Returns candidates in deterministic similarity order.
    #[must_use]
    pub fn hits(&self) -> &[VectorHit] {
        &self.hits
    }

    /// Returns whether the configured boundary omitted further compatible cards.
    #[must_use]
    pub const fn truncated(&self) -> bool {
        self.truncated
    }
}

/// Invalid adapter projection of semantic vector results.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VectorSearchResultError {
    /// The adapter returned more than the requested semantic result limit.
    TooManyHits,
    /// At least one hit came from another vector compatibility profile.
    ProfileMismatch,
    /// The same card body revision appeared more than once.
    DuplicateCardRevision,
}

impl fmt::Display for VectorSearchResultError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::TooManyHits => "vector search result exceeds its fixed boundary",
            Self::ProfileMismatch => "vector search result mixes model profiles",
            Self::DuplicateCardRevision => {
                "vector search result contains a duplicate semantic card revision"
            }
        })
    }
}

impl Error for VectorSearchResultError {}

fn validate_profile_text(
    value: &str,
    maximum: usize,
    kind: EmbeddingProfileTextKind,
) -> Result<(), EmbeddingProfileTextError> {
    if value.is_empty() || value.len() > maximum {
        return Err(EmbeddingProfileTextError::InvalidLength {
            kind,
            actual: value.len(),
        });
    }
    let safe = value.bytes().all(|byte| match kind {
        EmbeddingProfileTextKind::Provider => {
            byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-')
        }
        EmbeddingProfileTextKind::Model => {
            byte.is_ascii_alphanumeric()
                || matches!(byte, b'.' | b'_' | b'-' | b'+' | b':' | b'/' | b'@')
        }
    });
    if !safe {
        return Err(EmbeddingProfileTextError::UnsafeCharacter { kind });
    }
    Ok(())
}

fn derive_profile_id(
    provider_id: &EmbeddingProviderId,
    model_id: &EmbeddingModelId,
    dimension: EmbeddingDimension,
    data_type: EmbeddingDataType,
    quantization: EmbeddingQuantization,
    normalization: EmbeddingVectorNormalization,
) -> ModelProfileId {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"a3-embedding-profile-v1\0");
    update_length_prefixed(&mut hasher, provider_id.as_str().as_bytes());
    update_length_prefixed(&mut hasher, model_id.as_str().as_bytes());
    hasher.update(&dimension.get().to_be_bytes());
    hasher.update(&[match data_type {
        EmbeddingDataType::Float32 => 1,
    }]);
    hasher.update(&[match quantization {
        EmbeddingQuantization::None => 0,
    }]);
    hasher.update(&[match normalization {
        EmbeddingVectorNormalization::L2Unit => 1,
    }]);
    ModelProfileId(*hasher.finalize().as_bytes())
}

fn update_length_prefixed(hasher: &mut blake3::Hasher, value: &[u8]) {
    let length = u32::try_from(value.len()).unwrap_or(u32::MAX);
    hasher.update(&length.to_be_bytes());
    hasher.update(value);
}

fn write_hex(formatter: &mut fmt::Formatter<'_>, bytes: &[u8]) -> fmt::Result {
    for byte in bytes {
        write!(formatter, "{byte:02x}")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        EmbeddingBatchSize, EmbeddingDimension, EmbeddingModelId, EmbeddingModelProfile,
        EmbeddingProviderId, EmbeddingTimestamp, EmbeddingVector, EmbeddingVectorError,
        ModelProfileId, VectorHit, VectorSearchCapability, VectorSearchLimit, VectorSearchResult,
        VectorSearchResultError,
    };
    use crate::{BodyHash, NormalizedRetrievalSignal, SemanticCardId, SnapshotId, SourceChannel};

    fn profile(
        provider: &str,
        model: &str,
        dimension: u16,
    ) -> Result<EmbeddingModelProfile, Box<dyn std::error::Error>> {
        Ok(EmbeddingModelProfile::v1(
            EmbeddingProviderId::new(provider.to_owned())?,
            EmbeddingModelId::new(model.to_owned())?,
            EmbeddingDimension::new(dimension)?,
            EmbeddingBatchSize::new(8)?,
        ))
    }

    #[test]
    fn profile_identity_changes_for_provider_model_and_dimension()
    -> Result<(), Box<dyn std::error::Error>> {
        let base = profile("local", "embed-v1", 3)?;
        assert_ne!(base.id(), profile("other", "embed-v1", 3)?.id());
        assert_ne!(base.id(), profile("local", "embed-v2", 3)?.id());
        assert_ne!(base.id(), profile("local", "embed-v1", 4)?.id());
        assert!(base.has_compatible_identity());
        Ok(())
    }

    #[test]
    fn profile_resource_and_persistence_boundaries_are_explicit() {
        assert!(EmbeddingDimension::new(0).is_err());
        assert!(EmbeddingDimension::new(8_193).is_err());
        assert!(EmbeddingBatchSize::new(0).is_err());
        assert!(EmbeddingBatchSize::new(65).is_err());
        assert!(EmbeddingProviderId::new("https://endpoint".to_owned()).is_err());
        assert!(EmbeddingModelId::new("model name".to_owned()).is_err());
        assert!(EmbeddingTimestamp::from_unix_millis(i64::MAX as u64).is_ok());
        assert!(EmbeddingTimestamp::from_unix_millis((i64::MAX as u64) + 1).is_err());
    }

    #[test]
    fn vector_validation_rejects_dimension_nan_and_zero_norm()
    -> Result<(), Box<dyn std::error::Error>> {
        let dimension = EmbeddingDimension::new(2)?;
        assert!(matches!(
            EmbeddingVector::normalize_l2(vec![1.0], dimension),
            Err(EmbeddingVectorError::DimensionMismatch { .. })
        ));
        assert_eq!(
            EmbeddingVector::normalize_l2(vec![f32::NAN, 1.0], dimension),
            Err(EmbeddingVectorError::NonFiniteComponent)
        );
        assert_eq!(
            EmbeddingVector::normalize_l2(vec![0.0, 0.0], dimension),
            Err(EmbeddingVectorError::ZeroOrInvalidNorm)
        );
        let vector = EmbeddingVector::normalize_l2(vec![3.0, 4.0], dimension)?;
        assert_eq!(vector.components(), &[0.6, 0.8]);
        assert!(!format!("{vector:?}").contains("0.6"));
        Ok(())
    }

    #[test]
    fn vector_hit_is_semantic_only_and_result_rejects_profile_mixing()
    -> Result<(), Box<dyn std::error::Error>> {
        let profile_id = profile("local", "embed-v1", 2)?.id();
        let other_profile = profile("local", "embed-v2", 2)?.id();
        let hit = VectorHit::new(
            SemanticCardId::from_bytes([1; 32]),
            BodyHash::from_bytes([2; 32]),
            profile_id,
            NormalizedRetrievalSignal::FULL,
        );
        assert_eq!(hit.source_channel(), SourceChannel::Semantic);
        assert_eq!(
            VectorSearchResult::new(
                SnapshotId::from_bytes([3; 32]),
                other_profile,
                VectorSearchCapability::LinearFallback,
                super::VectorSearchLimit::DEFAULT,
                vec![hit],
                false,
            ),
            Err(VectorSearchResultError::ProfileMismatch)
        );
        Ok(())
    }

    #[test]
    fn reconstructed_profile_id_remains_an_opaque_persistence_value() {
        let id = ModelProfileId::from_bytes([9; 32]);
        assert_eq!(id.as_bytes(), &[9; 32]);
    }

    #[test]
    fn vector_result_enforces_the_requested_limit() -> Result<(), Box<dyn std::error::Error>> {
        let profile_id = profile("local", "embed-v1", 2)?.id();
        let hits = (0_u8..2)
            .map(|value| {
                VectorHit::new(
                    SemanticCardId::from_bytes([value; 32]),
                    BodyHash::from_bytes([value; 32]),
                    profile_id,
                    NormalizedRetrievalSignal::FULL,
                )
            })
            .collect();
        assert_eq!(
            VectorSearchResult::new(
                SnapshotId::from_bytes([3; 32]),
                profile_id,
                VectorSearchCapability::LinearFallback,
                VectorSearchLimit::new(1)?,
                hits,
                true,
            ),
            Err(VectorSearchResultError::TooManyHits)
        );
        Ok(())
    }
}
