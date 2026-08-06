use super::{ModelId, ModelProfileId, ModelProviderId};
use std::error::Error;
use std::fmt;

const MIN_MODEL_CONTEXT_TOKENS: u32 = 1_024;
const MAX_MODEL_CONTEXT_TOKENS: u32 = 1_048_576;
const MAX_MODEL_OUTPUT_TOKENS: u32 = 262_144;
const MAX_MODEL_PARALLELISM: u16 = 64;
const MAX_MODEL_TEMPERATURE_MILLI: u16 = 2_000;
const MAX_MODEL_TOP_P_MILLI: u16 = 1_000;
const MAX_MODEL_STOP_SEQUENCES: usize = 16;
const MAX_MODEL_STOP_SEQUENCE_BYTES: usize = 128;
const MODEL_PROFILE_HASH_DOMAIN: &[u8] = b"a3.model-profile.llm.v1\0";

/// Version of the persisted and run-referenced general model-profile schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ModelProfileVersion(u16);

impl ModelProfileVersion {
    /// First general model-profile schema.
    pub const V1: Self = Self(1);

    /// Reconstructs the only schema version understood by this build.
    pub const fn from_u16(value: u16) -> Result<Self, ModelProfileVersionError> {
        if value == Self::V1.0 {
            Ok(Self::V1)
        } else {
            Err(ModelProfileVersionError { value })
        }
    }

    /// Returns the stable persisted representation.
    #[must_use]
    pub const fn get(self) -> u16 {
        self.0
    }
}

/// Unknown general model-profile schema version.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelProfileVersionError {
    value: u16,
}

impl fmt::Display for ModelProfileVersionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "model-profile schema version {} is unsupported",
            self.value
        )
    }
}

impl Error for ModelProfileVersionError {}

/// Durable, content-free reference embedded into every new agent run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ModelProfileReference {
    id: ModelProfileId,
    version: ModelProfileVersion,
}

impl ModelProfileReference {
    /// Reconstructs a reference after its ID and schema version were validated.
    #[must_use]
    pub const fn new(id: ModelProfileId, version: ModelProfileVersion) -> Self {
        Self { id, version }
    }

    /// Returns the exact complete profile identity.
    #[must_use]
    pub const fn id(self) -> ModelProfileId {
        self.id
    }

    /// Returns the referenced profile schema.
    #[must_use]
    pub const fn version(self) -> ModelProfileVersion {
        self.version
    }
}

/// Effective context window used for request packing and provider configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ModelContextLimit(u32);

impl ModelContextLimit {
    /// Validates a context limit from 1,024 through 1,048,576 tokens.
    pub const fn new(value: u32) -> Result<Self, ModelContextLimitError> {
        if value < MIN_MODEL_CONTEXT_TOKENS || value > MAX_MODEL_CONTEXT_TOKENS {
            Err(ModelContextLimitError { value })
        } else {
            Ok(Self(value))
        }
    }

    /// Returns the effective token limit.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// Context limit was outside the bounded local operating range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelContextLimitError {
    value: u32,
}

impl fmt::Display for ModelContextLimitError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "model context limit {} must be between {MIN_MODEL_CONTEXT_TOKENS} and {MAX_MODEL_CONTEXT_TOKENS}",
            self.value
        )
    }
}

impl Error for ModelContextLimitError {}

/// Maximum generated tokens reserved for one provider response.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ModelOutputLimit(u32);

impl ModelOutputLimit {
    /// Validates a positive output limit within the global allocation bound.
    pub const fn new(value: u32) -> Result<Self, ModelOutputLimitError> {
        if value == 0 || value > MAX_MODEL_OUTPUT_TOKENS {
            Err(ModelOutputLimitError { value })
        } else {
            Ok(Self(value))
        }
    }

    /// Returns the maximum generated token count.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// Output limit was zero or exceeded the fixed resource boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelOutputLimitError {
    value: u32,
}

impl fmt::Display for ModelOutputLimitError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "model output limit {} must be between 1 and {MAX_MODEL_OUTPUT_TOKENS}",
            self.value
        )
    }
}

impl Error for ModelOutputLimitError {}

/// Maximum concurrent requests allowed for one local model profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ModelParallelismLimit(u16);

impl ModelParallelismLimit {
    /// Validates a positive concurrency limit capped for bounded local scheduling.
    pub const fn new(value: u16) -> Result<Self, ModelParallelismLimitError> {
        if value == 0 || value > MAX_MODEL_PARALLELISM {
            Err(ModelParallelismLimitError { value })
        } else {
            Ok(Self(value))
        }
    }

    /// Returns the maximum concurrent request count.
    #[must_use]
    pub const fn get(self) -> u16 {
        self.0
    }
}

/// Parallelism was zero or exceeded the scheduler boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelParallelismLimitError {
    value: u16,
}

impl fmt::Display for ModelParallelismLimitError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "model parallelism {} must be between 1 and {MAX_MODEL_PARALLELISM}",
            self.value
        )
    }
}

impl Error for ModelParallelismLimitError {}

/// Fixed-point temperature in thousandths, avoiding floating-point profile identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ModelTemperature(u16);

impl ModelTemperature {
    /// Validates a temperature from 0.000 through 2.000.
    pub const fn from_milli(value: u16) -> Result<Self, ModelTemperatureError> {
        if value > MAX_MODEL_TEMPERATURE_MILLI {
            Err(ModelTemperatureError { value })
        } else {
            Ok(Self(value))
        }
    }

    /// Returns the fixed-point thousandths value.
    #[must_use]
    pub const fn milli(self) -> u16 {
        self.0
    }
}

/// Temperature exceeded the bounded sampling range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelTemperatureError {
    value: u16,
}

impl fmt::Display for ModelTemperatureError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "model temperature {} must not exceed {MAX_MODEL_TEMPERATURE_MILLI} thousandths",
            self.value
        )
    }
}

impl Error for ModelTemperatureError {}

/// Fixed-point nucleus-sampling probability in thousandths.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ModelTopP(u16);

impl ModelTopP {
    /// Validates a top-p probability from 0.001 through 1.000.
    pub const fn from_milli(value: u16) -> Result<Self, ModelTopPError> {
        if value == 0 || value > MAX_MODEL_TOP_P_MILLI {
            Err(ModelTopPError { value })
        } else {
            Ok(Self(value))
        }
    }

    /// Returns the fixed-point thousandths value.
    #[must_use]
    pub const fn milli(self) -> u16 {
        self.0
    }
}

/// Top-p was zero or exceeded one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelTopPError {
    value: u16,
}

impl fmt::Display for ModelTopPError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "model top-p {} must be between 1 and {MAX_MODEL_TOP_P_MILLI} thousandths",
            self.value
        )
    }
}

impl Error for ModelTopPError {}

/// Deterministic, fully versioned sampling parameters for one profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ModelSamplingProfile {
    temperature: ModelTemperature,
    top_p: ModelTopP,
}

impl ModelSamplingProfile {
    /// Combines independently validated fixed-point sampling values.
    #[must_use]
    pub const fn new(temperature: ModelTemperature, top_p: ModelTopP) -> Self {
        Self { temperature, top_p }
    }

    /// Returns the fixed-point temperature.
    #[must_use]
    pub const fn temperature(self) -> ModelTemperature {
        self.temperature
    }

    /// Returns the fixed-point nucleus probability.
    #[must_use]
    pub const fn top_p(self) -> ModelTopP {
        self.top_p
    }
}

/// One bounded stop condition whose text is hidden from debug output.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ModelStopSequence(String);

impl ModelStopSequence {
    /// Validates one non-empty stop sequence up to 128 UTF-8 bytes.
    pub fn try_from_string(value: String) -> Result<Self, ModelStopSequenceError> {
        if value.is_empty() || value.len() > MAX_MODEL_STOP_SEQUENCE_BYTES {
            return Err(ModelStopSequenceError::InvalidLength {
                actual: value.len(),
            });
        }
        if value
            .chars()
            .any(|character| character.is_control() && !matches!(character, '\n' | '\r' | '\t'))
        {
            return Err(ModelStopSequenceError::UnsafeControlCharacter);
        }
        Ok(Self(value))
    }

    /// Returns stop text only to a concrete provider request encoder.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for ModelStopSequence {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ModelStopSequence")
            .field("bytes", &self.0.len())
            .finish()
    }
}

/// Stop sequence violated its allocation or control-character boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelStopSequenceError {
    /// Text was empty or longer than 128 UTF-8 bytes.
    InvalidLength {
        /// Observed byte count.
        actual: usize,
    },
    /// Text contained an unsupported control character.
    UnsafeControlCharacter,
}

impl fmt::Display for ModelStopSequenceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLength { actual } => {
                write!(
                    formatter,
                    "model stop sequence has invalid byte length {actual}"
                )
            }
            Self::UnsafeControlCharacter => {
                formatter.write_str("model stop sequence contains an unsafe control character")
            }
        }
    }
}

impl Error for ModelStopSequenceError {}

/// Canonically ordered unique set of zero through sixteen stop conditions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelStopSequences(Vec<ModelStopSequence>);

impl ModelStopSequences {
    /// Returns the empty stop set.
    #[must_use]
    pub const fn empty() -> Self {
        Self(Vec::new())
    }

    /// Sorts and validates one bounded unique set.
    pub fn new(mut values: Vec<ModelStopSequence>) -> Result<Self, ModelStopSequencesError> {
        if values.len() > MAX_MODEL_STOP_SEQUENCES {
            return Err(ModelStopSequencesError::TooMany {
                actual: values.len(),
            });
        }
        values.sort();
        if values.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(ModelStopSequencesError::Duplicate);
        }
        Ok(Self(values))
    }

    /// Returns canonical stop conditions for provider encoding.
    #[must_use]
    pub fn as_slice(&self) -> &[ModelStopSequence] {
        &self.0
    }
}

/// Stop-condition collection exceeded its bound or repeated one value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelStopSequencesError {
    /// More than sixteen conditions were supplied.
    TooMany {
        /// Observed condition count.
        actual: usize,
    },
    /// The same stop condition occurred more than once.
    Duplicate,
}

impl fmt::Display for ModelStopSequencesError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooMany { actual } => write!(
                formatter,
                "model profile has {actual} stop conditions; maximum is {MAX_MODEL_STOP_SEQUENCES}"
            ),
            Self::Duplicate => formatter.write_str("model profile repeats a stop condition"),
        }
    }
}

impl Error for ModelStopSequencesError {}

/// Provider-neutral result of the real structured-output self-test.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ModelStructuredOutputCapability {
    /// A live request produced output matching the exact probe schema.
    Verified,
    /// The live schema probe failed or could not be completed.
    Unavailable,
}

/// Native tool-call mode reported by the provider independently of structured-output proof.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ModelToolCallMode {
    /// No native tool-call capability is used.
    Disabled,
    /// Provider metadata reports native tools; H6 may still choose schema text mode.
    NativeProviderReported,
}

/// Capability result retained by the versioned profile without model-name inference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ModelCapabilities {
    structured_output: ModelStructuredOutputCapability,
    tool_call_mode: ModelToolCallMode,
}

impl ModelCapabilities {
    /// Creates an explicit capability observation.
    #[must_use]
    pub const fn new(
        structured_output: ModelStructuredOutputCapability,
        tool_call_mode: ModelToolCallMode,
    ) -> Self {
        Self {
            structured_output,
            tool_call_mode,
        }
    }

    /// Returns the live structured-output probe result.
    #[must_use]
    pub const fn structured_output(self) -> ModelStructuredOutputCapability {
        self.structured_output
    }

    /// Returns provider-reported native tool mode.
    #[must_use]
    pub const fn tool_call_mode(self) -> ModelToolCallMode {
        self.tool_call_mode
    }

    /// Executable schema actions require a successful live structured-output probe.
    #[must_use]
    pub const fn executable_actions_enabled(self) -> bool {
        matches!(
            self.structured_output,
            ModelStructuredOutputCapability::Verified
        )
    }
}

/// Deterministic token-cost strategy selected by the model profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ModelTokenCountingStrategy {
    /// Counts every UTF-8 byte as one token, a tokenizer-independent safe upper bound.
    ConservativeUtf8BytesV1,
}

impl ModelTokenCountingStrategy {
    /// Computes a deterministic conservative cost without provider access.
    pub fn count_text(self, text: &str) -> Result<ModelTokenCount, ModelTokenCountError> {
        match self {
            Self::ConservativeUtf8BytesV1 => ModelTokenCount::from_usize(text.len()),
        }
    }
}

/// Bounded exact integer token-cost estimate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ModelTokenCount(u32);

impl ModelTokenCount {
    fn from_usize(value: usize) -> Result<Self, ModelTokenCountError> {
        u32::try_from(value)
            .map(Self)
            .map_err(|_| ModelTokenCountError)
    }

    /// Returns the deterministic count.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// Token count exceeded the fixed 32-bit context arithmetic range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelTokenCountError;

impl fmt::Display for ModelTokenCountError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("model token count exceeds the supported range")
    }
}

impl Error for ModelTokenCountError {}

/// Whether the JSON Schema is also repeated in prompt text for model grounding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ModelPromptSchemaGrounding {
    /// Only the provider format field carries the schema.
    FormatFieldOnly,
    /// Static prompt construction repeats the same canonical schema text.
    RepeatSchemaInPrompt,
}

/// Validated run-shaping settings independent of capability evidence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelProfileSettings {
    context_limit: ModelContextLimit,
    output_limit: ModelOutputLimit,
    token_counting: ModelTokenCountingStrategy,
    parallelism_limit: ModelParallelismLimit,
    sampling: ModelSamplingProfile,
    stop_sequences: ModelStopSequences,
    schema_grounding: ModelPromptSchemaGrounding,
}

impl ModelProfileSettings {
    /// Validates the cross-field relation between context and output bounds.
    pub fn new(
        context_limit: ModelContextLimit,
        output_limit: ModelOutputLimit,
        token_counting: ModelTokenCountingStrategy,
        parallelism_limit: ModelParallelismLimit,
        sampling: ModelSamplingProfile,
        stop_sequences: ModelStopSequences,
        schema_grounding: ModelPromptSchemaGrounding,
    ) -> Result<Self, ModelProfileError> {
        if output_limit.get() > context_limit.get() {
            return Err(ModelProfileError::OutputExceedsContext);
        }
        Ok(Self {
            context_limit,
            output_limit,
            token_counting,
            parallelism_limit,
            sampling,
            stop_sequences,
            schema_grounding,
        })
    }

    /// Returns the effective context window.
    #[must_use]
    pub const fn context_limit(&self) -> ModelContextLimit {
        self.context_limit
    }

    /// Returns the generated-output bound.
    #[must_use]
    pub const fn output_limit(&self) -> ModelOutputLimit {
        self.output_limit
    }

    /// Returns the deterministic token-cost strategy.
    #[must_use]
    pub const fn token_counting(&self) -> ModelTokenCountingStrategy {
        self.token_counting
    }

    /// Returns the local request concurrency limit.
    #[must_use]
    pub const fn parallelism_limit(&self) -> ModelParallelismLimit {
        self.parallelism_limit
    }

    /// Returns deterministic sampling parameters.
    #[must_use]
    pub const fn sampling(&self) -> ModelSamplingProfile {
        self.sampling
    }

    /// Returns canonical stop conditions.
    #[must_use]
    pub const fn stop_sequences(&self) -> &ModelStopSequences {
        &self.stop_sequences
    }

    /// Returns prompt-level schema grounding behavior.
    #[must_use]
    pub const fn schema_grounding(&self) -> ModelPromptSchemaGrounding {
        self.schema_grounding
    }
}

/// Audit source of the effective profile settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ModelProfileSource {
    /// Settings were validated together with a live provider capability observation.
    Probe,
    /// A user changed run-shaping settings without changing capability evidence.
    ManualOverride(ModelProfileOverrideRevision),
}

/// Monotone non-zero user override revision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ModelProfileOverrideRevision(u32);

impl ModelProfileOverrideRevision {
    /// Validates a non-zero override revision.
    pub const fn new(value: u32) -> Result<Self, ModelProfileOverrideRevisionError> {
        if value == 0 {
            Err(ModelProfileOverrideRevisionError)
        } else {
            Ok(Self(value))
        }
    }

    /// Returns the persisted revision.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// Override revision was zero.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelProfileOverrideRevisionError;

impl fmt::Display for ModelProfileOverrideRevisionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("model-profile override revision must be non-zero")
    }
}

impl Error for ModelProfileOverrideRevisionError {}

/// Complete version-one local model profile used by request building and context packing.
#[derive(Clone, PartialEq, Eq)]
pub struct ModelProfile {
    id: ModelProfileId,
    version: ModelProfileVersion,
    provider_id: ModelProviderId,
    model_id: ModelId,
    settings: ModelProfileSettings,
    capabilities: ModelCapabilities,
    source: ModelProfileSource,
}

impl ModelProfile {
    /// Builds a V1 profile from explicit settings and a live capability observation.
    #[must_use]
    pub fn from_probe(
        provider_id: ModelProviderId,
        model_id: ModelId,
        settings: ModelProfileSettings,
        capabilities: ModelCapabilities,
    ) -> Self {
        Self::build(
            provider_id,
            model_id,
            settings,
            capabilities,
            ModelProfileSource::Probe,
        )
    }

    fn build(
        provider_id: ModelProviderId,
        model_id: ModelId,
        settings: ModelProfileSettings,
        capabilities: ModelCapabilities,
        source: ModelProfileSource,
    ) -> Self {
        let version = ModelProfileVersion::V1;
        let id = derive_model_profile_id(
            version,
            &provider_id,
            &model_id,
            &settings,
            capabilities,
            source,
        );
        Self {
            id,
            version,
            provider_id,
            model_id,
            settings,
            capabilities,
            source,
        }
    }

    /// Returns the complete profile identity.
    #[must_use]
    pub const fn id(&self) -> ModelProfileId {
        self.id
    }

    /// Returns the profile schema version.
    #[must_use]
    pub const fn version(&self) -> ModelProfileVersion {
        self.version
    }

    /// Returns the stable provider identity.
    #[must_use]
    pub const fn provider_id(&self) -> &ModelProviderId {
        &self.provider_id
    }

    /// Returns the opaque provider-native model identity.
    #[must_use]
    pub const fn model_id(&self) -> &ModelId {
        &self.model_id
    }

    /// Returns all run-shaping settings.
    #[must_use]
    pub const fn settings(&self) -> &ModelProfileSettings {
        &self.settings
    }

    /// Returns capability evidence without consulting the model name.
    #[must_use]
    pub const fn capabilities(&self) -> ModelCapabilities {
        self.capabilities
    }

    /// Returns whether settings came directly from the probe or a later override.
    #[must_use]
    pub const fn source(&self) -> ModelProfileSource {
        self.source
    }

    /// Returns the durable content-free reference for a new agent run.
    #[must_use]
    pub const fn reference(&self) -> ModelProfileReference {
        ModelProfileReference::new(self.id, self.version)
    }

    /// Returns whether H6 may submit executable structured actions with this profile.
    #[must_use]
    pub const fn executable_actions_enabled(&self) -> bool {
        self.capabilities.executable_actions_enabled()
    }

    /// Applies user-selected settings while preserving the exact capability observation.
    pub fn apply_override(
        &self,
        profile_override: ModelProfileOverride,
        revision: ModelProfileOverrideRevision,
    ) -> Result<Self, ModelProfileOverrideError> {
        let settings = profile_override.apply(&self.settings)?;
        Ok(Self::build(
            self.provider_id.clone(),
            self.model_id.clone(),
            settings,
            self.capabilities,
            ModelProfileSource::ManualOverride(revision),
        ))
    }
}

impl fmt::Debug for ModelProfile {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ModelProfile")
            .field("id", &self.id)
            .field("version", &self.version)
            .field("provider_id", &self.provider_id)
            .field("model_id", &self.model_id)
            .field("context_limit", &self.settings.context_limit)
            .field("output_limit", &self.settings.output_limit)
            .field("parallelism_limit", &self.settings.parallelism_limit)
            .field("stop_sequence_count", &self.settings.stop_sequences.0.len())
            .field("capabilities", &self.capabilities)
            .field("source", &self.source)
            .finish()
    }
}

/// Invalid cross-field relation or reconstructed profile identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelProfileError {
    /// Generated output cannot exceed the complete context window.
    OutputExceedsContext,
}

impl fmt::Display for ModelProfileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OutputExceedsContext => {
                formatter.write_str("model output limit exceeds the context limit")
            }
        }
    }
}

impl Error for ModelProfileError {}

/// Optional user replacements for run-shaping settings; capability fields are absent by design.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelProfileOverride {
    context_limit: Option<ModelContextLimit>,
    output_limit: Option<ModelOutputLimit>,
    token_counting: Option<ModelTokenCountingStrategy>,
    parallelism_limit: Option<ModelParallelismLimit>,
    sampling: Option<ModelSamplingProfile>,
    stop_sequences: Option<ModelStopSequences>,
    schema_grounding: Option<ModelPromptSchemaGrounding>,
}

impl ModelProfileOverride {
    /// Creates one non-empty override that cannot alter capability evidence.
    pub fn new(
        context_limit: Option<ModelContextLimit>,
        output_limit: Option<ModelOutputLimit>,
        token_counting: Option<ModelTokenCountingStrategy>,
        parallelism_limit: Option<ModelParallelismLimit>,
        sampling: Option<ModelSamplingProfile>,
        stop_sequences: Option<ModelStopSequences>,
        schema_grounding: Option<ModelPromptSchemaGrounding>,
    ) -> Result<Self, ModelProfileOverrideError> {
        if context_limit.is_none()
            && output_limit.is_none()
            && token_counting.is_none()
            && parallelism_limit.is_none()
            && sampling.is_none()
            && stop_sequences.is_none()
            && schema_grounding.is_none()
        {
            return Err(ModelProfileOverrideError::Empty);
        }
        Ok(Self {
            context_limit,
            output_limit,
            token_counting,
            parallelism_limit,
            sampling,
            stop_sequences,
            schema_grounding,
        })
    }

    fn apply(
        self,
        current: &ModelProfileSettings,
    ) -> Result<ModelProfileSettings, ModelProfileOverrideError> {
        ModelProfileSettings::new(
            self.context_limit.unwrap_or(current.context_limit),
            self.output_limit.unwrap_or(current.output_limit),
            self.token_counting.unwrap_or(current.token_counting),
            self.parallelism_limit.unwrap_or(current.parallelism_limit),
            self.sampling.unwrap_or(current.sampling),
            self.stop_sequences
                .unwrap_or_else(|| current.stop_sequences.clone()),
            self.schema_grounding.unwrap_or(current.schema_grounding),
        )
        .map_err(ModelProfileOverrideError::InvalidSettings)
    }
}

/// Manual override was empty or made the settings internally inconsistent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelProfileOverrideError {
    /// No field was supplied.
    Empty,
    /// Replacements violated a cross-field profile invariant.
    InvalidSettings(ModelProfileError),
}

impl fmt::Display for ModelProfileOverrideError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("model-profile override contains no changes"),
            Self::InvalidSettings(error) => error.fmt(formatter),
        }
    }
}

impl Error for ModelProfileOverrideError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidSettings(error) => Some(error),
            Self::Empty => None,
        }
    }
}

fn derive_model_profile_id(
    version: ModelProfileVersion,
    provider_id: &ModelProviderId,
    model_id: &ModelId,
    settings: &ModelProfileSettings,
    capabilities: ModelCapabilities,
    source: ModelProfileSource,
) -> ModelProfileId {
    let mut hasher = blake3::Hasher::new();
    hasher.update(MODEL_PROFILE_HASH_DOMAIN);
    hasher.update(&version.get().to_be_bytes());
    update_length_prefixed(&mut hasher, provider_id.as_str().as_bytes());
    update_length_prefixed(&mut hasher, model_id.as_str().as_bytes());
    hasher.update(&settings.context_limit.get().to_be_bytes());
    hasher.update(&settings.output_limit.get().to_be_bytes());
    hasher.update(&[token_strategy_code(settings.token_counting)]);
    hasher.update(&settings.parallelism_limit.get().to_be_bytes());
    hasher.update(&settings.sampling.temperature.milli().to_be_bytes());
    hasher.update(&settings.sampling.top_p.milli().to_be_bytes());
    hasher.update(&[
        structured_output_code(capabilities.structured_output),
        tool_call_mode_code(capabilities.tool_call_mode),
        schema_grounding_code(settings.schema_grounding),
    ]);
    hasher.update(
        &u64::try_from(settings.stop_sequences.0.len())
            .unwrap_or(u64::MAX)
            .to_be_bytes(),
    );
    for stop in &settings.stop_sequences.0 {
        update_length_prefixed(&mut hasher, stop.as_str().as_bytes());
    }
    match source {
        ModelProfileSource::Probe => {
            hasher.update(&[0]);
        }
        ModelProfileSource::ManualOverride(revision) => {
            hasher.update(&[1]);
            hasher.update(&revision.get().to_be_bytes());
        }
    };
    ModelProfileId::from_bytes(*hasher.finalize().as_bytes())
}

fn update_length_prefixed(hasher: &mut blake3::Hasher, value: &[u8]) {
    hasher.update(&u64::try_from(value.len()).unwrap_or(u64::MAX).to_be_bytes());
    hasher.update(value);
}

const fn token_strategy_code(value: ModelTokenCountingStrategy) -> u8 {
    match value {
        ModelTokenCountingStrategy::ConservativeUtf8BytesV1 => 1,
    }
}

const fn structured_output_code(value: ModelStructuredOutputCapability) -> u8 {
    match value {
        ModelStructuredOutputCapability::Verified => 1,
        ModelStructuredOutputCapability::Unavailable => 0,
    }
}

const fn tool_call_mode_code(value: ModelToolCallMode) -> u8 {
    match value {
        ModelToolCallMode::Disabled => 0,
        ModelToolCallMode::NativeProviderReported => 1,
    }
}

const fn schema_grounding_code(value: ModelPromptSchemaGrounding) -> u8 {
    match value {
        ModelPromptSchemaGrounding::FormatFieldOnly => 0,
        ModelPromptSchemaGrounding::RepeatSchemaInPrompt => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ModelCapabilities, ModelContextLimit, ModelOutputLimit, ModelParallelismLimit,
        ModelProfile, ModelProfileOverride, ModelProfileOverrideRevision, ModelProfileSettings,
        ModelProfileSource, ModelPromptSchemaGrounding, ModelSamplingProfile, ModelStopSequence,
        ModelStopSequences, ModelStructuredOutputCapability, ModelTemperature,
        ModelTokenCountingStrategy, ModelToolCallMode, ModelTopP,
    };
    use crate::{ModelId, ModelProviderId};

    fn settings() -> Result<ModelProfileSettings, Box<dyn std::error::Error>> {
        Ok(ModelProfileSettings::new(
            ModelContextLimit::new(16_384)?,
            ModelOutputLimit::new(4_096)?,
            ModelTokenCountingStrategy::ConservativeUtf8BytesV1,
            ModelParallelismLimit::new(1)?,
            ModelSamplingProfile::new(
                ModelTemperature::from_milli(0)?,
                ModelTopP::from_milli(1_000)?,
            ),
            ModelStopSequences::new(vec![ModelStopSequence::try_from_string(
                "secret-stop-fixture".to_owned(),
            )?])?,
            ModelPromptSchemaGrounding::RepeatSchemaInPrompt,
        )?)
    }

    fn profile(
        structured_output: ModelStructuredOutputCapability,
    ) -> Result<ModelProfile, Box<dyn std::error::Error>> {
        Ok(ModelProfile::from_probe(
            ModelProviderId::try_from_string("ollama".to_owned())?,
            ModelId::try_from_string("opaque-model:tag".to_owned())?,
            settings()?,
            ModelCapabilities::new(structured_output, ModelToolCallMode::NativeProviderReported),
        ))
    }

    #[test]
    fn profile_identity_is_deterministic_versioned_and_run_safe()
    -> Result<(), Box<dyn std::error::Error>> {
        let first = profile(ModelStructuredOutputCapability::Verified)?;
        let repeated = profile(ModelStructuredOutputCapability::Verified)?;
        let failed = profile(ModelStructuredOutputCapability::Unavailable)?;

        assert_eq!(first.id(), repeated.id());
        assert_ne!(first.id(), failed.id());
        assert_eq!(first.reference().id(), first.id());
        assert_eq!(first.reference().version(), first.version());
        assert_eq!(first.source(), ModelProfileSource::Probe);
        assert!(first.executable_actions_enabled());
        assert!(!failed.executable_actions_enabled());
        assert!(!format!("{first:?}").contains("secret-stop-fixture"));
        Ok(())
    }

    #[test]
    fn manual_override_changes_settings_but_never_upgrades_capability_evidence()
    -> Result<(), Box<dyn std::error::Error>> {
        let failed = profile(ModelStructuredOutputCapability::Unavailable)?;
        let profile_override = ModelProfileOverride::new(
            Some(ModelContextLimit::new(32_768)?),
            None,
            None,
            Some(ModelParallelismLimit::new(2)?),
            None,
            None,
            None,
        )?;
        let overridden =
            failed.apply_override(profile_override, ModelProfileOverrideRevision::new(1)?)?;

        assert_ne!(failed.id(), overridden.id());
        assert_eq!(overridden.settings().context_limit().get(), 32_768);
        assert_eq!(overridden.settings().parallelism_limit().get(), 2);
        assert_eq!(overridden.capabilities(), failed.capabilities());
        assert!(!overridden.executable_actions_enabled());
        assert_eq!(
            overridden.source(),
            ModelProfileSource::ManualOverride(ModelProfileOverrideRevision::new(1)?)
        );
        assert!(ModelProfileOverride::new(None, None, None, None, None, None, None).is_err());
        Ok(())
    }

    #[test]
    fn limits_and_conservative_counter_reject_unsafe_budget_assumptions()
    -> Result<(), Box<dyn std::error::Error>> {
        assert!(ModelContextLimit::new(1_023).is_err());
        assert!(ModelOutputLimit::new(0).is_err());
        assert!(ModelParallelismLimit::new(0).is_err());
        assert!(
            ModelProfileSettings::new(
                ModelContextLimit::new(1_024)?,
                ModelOutputLimit::new(2_048)?,
                ModelTokenCountingStrategy::ConservativeUtf8BytesV1,
                ModelParallelismLimit::new(1)?,
                ModelSamplingProfile::new(
                    ModelTemperature::from_milli(0)?,
                    ModelTopP::from_milli(1_000)?,
                ),
                ModelStopSequences::empty(),
                ModelPromptSchemaGrounding::FormatFieldOnly,
            )
            .is_err()
        );
        assert_eq!(
            ModelTokenCountingStrategy::ConservativeUtf8BytesV1
                .count_text("A^3 🦀")?
                .get(),
            "A^3 🦀".len() as u32
        );
        assert!(ModelTemperature::from_milli(2_001).is_err());
        assert!(ModelTopP::from_milli(0).is_err());
        assert!(ModelProfileOverrideRevision::new(0).is_err());
        Ok(())
    }

    #[test]
    fn stop_sequences_are_bounded_canonical_unique_and_redacted()
    -> Result<(), Box<dyn std::error::Error>> {
        let first = ModelStopSequence::try_from_string("z-stop".to_owned())?;
        let second = ModelStopSequence::try_from_string("a-stop".to_owned())?;
        let stops = ModelStopSequences::new(vec![first.clone(), second])?;
        assert_eq!(stops.as_slice()[0].as_str(), "a-stop");
        assert!(ModelStopSequences::new(vec![first.clone(), first]).is_err());
        assert!(ModelStopSequence::try_from_string("x".repeat(129)).is_err());
        assert!(!format!("{:?}", stops.as_slice()[1]).contains("z-stop"));
        Ok(())
    }
}
