use crate::JobContext;
use a3_domain::{ModelId, ModelProviderId};
use futures::Stream;
use serde_json::Value;
use std::error::Error;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

const MAX_MODEL_MESSAGES: usize = 256;
const MAX_MODEL_MESSAGE_BYTES: usize = 512 * 1024;
const MAX_MODEL_REQUEST_TEXT_BYTES: usize = 2 * 1024 * 1024;
const MAX_STRUCTURED_OUTPUT_SCHEMA_BYTES: usize = 64 * 1024;
const MAX_MODEL_OUTPUT_CHUNK_BYTES: usize = 64 * 1024;
const MAX_MODEL_REQUEST_TIMEOUT_MILLIS: u64 = 300_000;

/// Future resolving when the owning model operation requests cancellation.
pub type ModelCancellationFuture<'a> = Pin<Box<dyn Future<Output = ()> + Send + 'a>>;

/// Stream of ordered provider-neutral output events or one terminal normalized failure.
pub type ProviderEventStream<'a> =
    Pin<Box<dyn Stream<Item = Result<ProviderEvent, ModelProviderFailure>> + Send + 'a>>;

/// Future establishing one streaming provider response.
pub type ModelProviderFuture<'a> = Pin<
    Box<dyn Future<Output = Result<ProviderEventStream<'a>, ModelProviderFailure>> + Send + 'a>,
>;

/// Cooperative cancellation boundary visible to a concrete provider adapter.
pub trait ModelOperationControl: fmt::Debug + Send + Sync {
    /// Returns whether cancellation has already been requested.
    fn is_cancelled(&self) -> bool;

    /// Returns a wakeable future for cancellation during a stalled network read.
    fn cancelled(&self) -> ModelCancellationFuture<'_>;
}

impl ModelOperationControl for JobContext {
    fn is_cancelled(&self) -> bool {
        self.cancellation_token().is_cancelled()
    }

    fn cancelled(&self) -> ModelCancellationFuture<'_> {
        Box::pin(self.cancellation_token().cancelled())
    }
}

/// Positive total provider request deadline, including the complete streamed body.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelRequestTimeout(Duration);

impl ModelRequestTimeout {
    /// Default local generation deadline.
    pub const DEFAULT: Self = Self(Duration::from_secs(120));

    /// Creates a non-zero timeout capped at five minutes.
    pub fn from_millis(value: u64) -> Result<Self, ModelRequestTimeoutError> {
        if value == 0 || value > MAX_MODEL_REQUEST_TIMEOUT_MILLIS {
            return Err(ModelRequestTimeoutError { value });
        }
        Ok(Self(Duration::from_millis(value)))
    }

    /// Returns the neutral duration enforced by the adapter.
    #[must_use]
    pub const fn duration(self) -> Duration {
        self.0
    }
}

/// Model request timeout was zero or exceeded the fixed maximum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelRequestTimeoutError {
    value: u64,
}

impl fmt::Display for ModelRequestTimeoutError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "model request timeout {} ms must be between 1 and {MAX_MODEL_REQUEST_TIMEOUT_MILLIS}",
            self.value
        )
    }
}

impl Error for ModelRequestTimeoutError {}

/// Provider-neutral chat role accepted by the V1 text boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ModelMessageRole {
    /// Harness and policy instructions.
    System,
    /// Current user or controller input.
    User,
    /// Prior model content retained intentionally in the current context pack.
    Assistant,
}

/// One bounded text message whose content is redacted from debug output.
#[derive(Clone, PartialEq, Eq)]
pub struct ModelMessage {
    role: ModelMessageRole,
    content: String,
}

impl ModelMessage {
    /// Validates one non-empty bounded message while retaining ordinary newlines and tabs.
    pub fn try_from_string(
        role: ModelMessageRole,
        content: String,
    ) -> Result<Self, ModelMessageError> {
        if content.is_empty() || content.len() > MAX_MODEL_MESSAGE_BYTES {
            return Err(ModelMessageError::InvalidLength {
                actual: content.len(),
            });
        }
        if content
            .chars()
            .any(|character| character.is_control() && !matches!(character, '\n' | '\r' | '\t'))
        {
            return Err(ModelMessageError::UnsafeControlCharacter);
        }
        Ok(Self { role, content })
    }

    /// Returns the provider-neutral role.
    #[must_use]
    pub const fn role(&self) -> ModelMessageRole {
        self.role
    }

    /// Returns content only to a concrete provider request encoder.
    #[must_use]
    pub fn content(&self) -> &str {
        &self.content
    }
}

impl fmt::Debug for ModelMessage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ModelMessage")
            .field("role", &self.role)
            .field("bytes", &self.content.len())
            .finish()
    }
}

/// Invalid model message text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelMessageError {
    /// Text was empty or exceeded the per-message allocation bound.
    InvalidLength {
        /// Observed UTF-8 bytes.
        actual: usize,
    },
    /// Text contained a control character other than newline, carriage return, or tab.
    UnsafeControlCharacter,
}

impl fmt::Display for ModelMessageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLength { actual } => {
                write!(formatter, "model message has invalid byte length {actual}")
            }
            Self::UnsafeControlCharacter => {
                formatter.write_str("model message contains an unsafe control character")
            }
        }
    }
}

impl Error for ModelMessageError {}

/// Bounded JSON Schema requested from providers supporting structured output.
#[derive(Clone, PartialEq, Eq)]
pub struct StructuredOutputSchema {
    value: Value,
    encoded_bytes: usize,
}

impl StructuredOutputSchema {
    /// Accepts only a bounded JSON object at the neutral provider boundary.
    pub fn new(value: Value) -> Result<Self, StructuredOutputSchemaError> {
        if !value.is_object() {
            return Err(StructuredOutputSchemaError::NotObject);
        }
        let encoded_bytes = serde_json::to_vec(&value)
            .map_err(StructuredOutputSchemaError::Encode)?
            .len();
        if encoded_bytes > MAX_STRUCTURED_OUTPUT_SCHEMA_BYTES {
            return Err(StructuredOutputSchemaError::TooLarge {
                actual: encoded_bytes,
            });
        }
        Ok(Self {
            value,
            encoded_bytes,
        })
    }

    /// Returns the validated schema only to a concrete provider encoder.
    #[must_use]
    pub const fn value(&self) -> &Value {
        &self.value
    }
}

impl fmt::Debug for StructuredOutputSchema {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("StructuredOutputSchema")
            .field("encoded_bytes", &self.encoded_bytes)
            .finish()
    }
}

/// Structured-output schema violated its neutral allocation or shape boundary.
#[derive(Debug)]
pub enum StructuredOutputSchemaError {
    /// Root was not a JSON object.
    NotObject,
    /// Canonical JSON encoding failed.
    Encode(serde_json::Error),
    /// Encoded schema exceeded 64 KiB.
    TooLarge {
        /// Observed encoded bytes.
        actual: usize,
    },
}

impl fmt::Display for StructuredOutputSchemaError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotObject => {
                formatter.write_str("structured-output schema root is not an object")
            }
            Self::Encode(_) => formatter.write_str("structured-output schema could not be encoded"),
            Self::TooLarge { actual } => {
                write!(formatter, "structured-output schema has {actual} bytes")
            }
        }
    }
}

impl Error for StructuredOutputSchemaError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Encode(error) => Some(error),
            Self::NotObject | Self::TooLarge { .. } => None,
        }
    }
}

/// Bounded provider-neutral text generation request.
#[derive(Clone, PartialEq, Eq)]
pub struct ModelProviderRequest {
    model_id: ModelId,
    messages: Vec<ModelMessage>,
    structured_output: Option<StructuredOutputSchema>,
    message_bytes: usize,
}

impl ModelProviderRequest {
    /// Validates message cardinality and aggregate allocation before HTTP encoding.
    pub fn new(
        model_id: ModelId,
        messages: Vec<ModelMessage>,
        structured_output: Option<StructuredOutputSchema>,
    ) -> Result<Self, ModelProviderRequestError> {
        if messages.is_empty() || messages.len() > MAX_MODEL_MESSAGES {
            return Err(ModelProviderRequestError::InvalidMessageCount {
                actual: messages.len(),
            });
        }
        let message_bytes = messages.iter().try_fold(0_usize, |total, message| {
            total
                .checked_add(message.content.len())
                .ok_or(ModelProviderRequestError::TextTooLarge)
        })?;
        if message_bytes > MAX_MODEL_REQUEST_TEXT_BYTES {
            return Err(ModelProviderRequestError::TextTooLarge);
        }
        Ok(Self {
            model_id,
            messages,
            structured_output,
            message_bytes,
        })
    }

    /// Returns the opaque provider-native model identity.
    #[must_use]
    pub const fn model_id(&self) -> &ModelId {
        &self.model_id
    }

    /// Returns messages in exact context order.
    #[must_use]
    pub fn messages(&self) -> &[ModelMessage] {
        &self.messages
    }

    /// Returns the optional strict structured-output schema.
    #[must_use]
    pub const fn structured_output(&self) -> Option<&StructuredOutputSchema> {
        self.structured_output.as_ref()
    }
}

impl fmt::Debug for ModelProviderRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ModelProviderRequest")
            .field("model_id", &self.model_id)
            .field("message_count", &self.messages.len())
            .field("message_bytes", &self.message_bytes)
            .field("structured_output", &self.structured_output.is_some())
            .finish()
    }
}

/// Invalid request cardinality or total prompt allocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelProviderRequestError {
    /// Message list was empty or exceeded 256 entries.
    InvalidMessageCount {
        /// Observed messages.
        actual: usize,
    },
    /// Aggregate message bytes exceeded 2 MiB or overflowed.
    TextTooLarge,
}

impl fmt::Display for ModelProviderRequestError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidMessageCount { actual } => {
                write!(formatter, "model request contains {actual} messages")
            }
            Self::TextTooLarge => formatter.write_str("model request text exceeds its boundary"),
        }
    }
}

impl Error for ModelProviderRequestError {}

/// One non-empty bounded output fragment with redacted debug formatting.
#[derive(Clone, PartialEq, Eq)]
pub struct ModelOutputChunk(String);

impl ModelOutputChunk {
    /// Applies the allocation boundary before provider output enters orchestration.
    pub fn try_from_string(value: String) -> Result<Self, ModelOutputChunkError> {
        if value.is_empty() || value.len() > MAX_MODEL_OUTPUT_CHUNK_BYTES {
            return Err(ModelOutputChunkError {
                actual: value.len(),
            });
        }
        Ok(Self(value))
    }

    /// Returns raw output only to the bounded accumulator or strict decoder.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for ModelOutputChunk {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ModelOutputChunk")
            .field("bytes", &self.0.len())
            .finish()
    }
}

/// Provider output chunk was empty or exceeded 64 KiB.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelOutputChunkError {
    actual: usize,
}

impl fmt::Display for ModelOutputChunkError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "model output chunk has invalid byte length {}",
            self.actual
        )
    }
}

impl Error for ModelOutputChunkError {}

/// Provider-neutral reason generation stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ModelFinishReason {
    /// Model reached a normal stop condition.
    Stop,
    /// Provider stopped at an output limit.
    OutputLimit,
    /// Provider reported another non-textual terminal reason.
    Other,
}

/// Optional token usage reported by a provider without tokenizer assumptions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelProviderUsage {
    prompt_tokens: Option<u64>,
    output_tokens: Option<u64>,
}

impl ModelProviderUsage {
    /// Creates usage metadata from independently optional provider counters.
    #[must_use]
    pub const fn new(prompt_tokens: Option<u64>, output_tokens: Option<u64>) -> Self {
        Self {
            prompt_tokens,
            output_tokens,
        }
    }

    /// Returns provider-reported input tokens.
    #[must_use]
    pub const fn prompt_tokens(self) -> Option<u64> {
        self.prompt_tokens
    }

    /// Returns provider-reported generated tokens.
    #[must_use]
    pub const fn output_tokens(self) -> Option<u64> {
        self.output_tokens
    }
}

/// Exactly one terminal completion record for a successful provider stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelProviderCompletion {
    reason: ModelFinishReason,
    usage: ModelProviderUsage,
}

impl ModelProviderCompletion {
    /// Creates a content-free terminal record.
    #[must_use]
    pub const fn new(reason: ModelFinishReason, usage: ModelProviderUsage) -> Self {
        Self { reason, usage }
    }

    /// Returns the normalized terminal reason.
    #[must_use]
    pub const fn reason(self) -> ModelFinishReason {
        self.reason
    }

    /// Returns optional provider counters.
    #[must_use]
    pub const fn usage(self) -> ModelProviderUsage {
        self.usage
    }
}

/// Ordered output from a streaming model provider.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderEvent {
    /// One bounded text fragment in provider order.
    OutputText(ModelOutputChunk),
    /// Sole terminal event after the response body has ended cleanly.
    Completed(ModelProviderCompletion),
}

/// General provider-neutral text generation capability.
pub trait ModelProvider: fmt::Debug + Send + Sync {
    /// Returns the stable provider identity without endpoint or credential data.
    fn provider_id(&self) -> &ModelProviderId;

    /// Establishes an ordered bounded event stream for one validated request.
    fn stream<'a>(
        &'a self,
        request: &'a ModelProviderRequest,
        timeout: ModelRequestTimeout,
        control: &'a dyn ModelOperationControl,
    ) -> ModelProviderFuture<'a>;
}

/// Stable provider failure without endpoint, payload, model output, or credential details.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelProviderFailure {
    /// Configured provider could not be reached or ended unexpectedly.
    Unavailable,
    /// Provider rejected the validated request.
    Rejected,
    /// Provider response violated the neutral or adapter schema.
    InvalidResponse,
    /// Total request deadline elapsed before the complete response body.
    TimedOut,
    /// Cooperative cancellation interrupted connect or body streaming.
    Cancelled,
    /// Current endpoint policy did not authorize this endpoint.
    EndpointDenied,
}

impl fmt::Display for ModelProviderFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Unavailable => "model provider is unavailable",
            Self::Rejected => "model provider rejected the request",
            Self::InvalidResponse => "model provider returned an invalid response",
            Self::TimedOut => "model provider request timed out",
            Self::Cancelled => "model provider request was cancelled",
            Self::EndpointDenied => "model provider endpoint is not authorized",
        })
    }
}

impl Error for ModelProviderFailure {}

#[cfg(test)]
mod tests {
    use super::{
        ModelMessage, ModelMessageRole, ModelOutputChunk, ModelProviderRequest,
        ModelRequestTimeout, StructuredOutputSchema,
    };
    use a3_domain::ModelId;
    use serde_json::json;

    #[test]
    fn neutral_request_and_output_boundaries_redact_content()
    -> Result<(), Box<dyn std::error::Error>> {
        let message = ModelMessage::try_from_string(
            ModelMessageRole::User,
            "secret prompt fixture".to_owned(),
        )?;
        let schema = StructuredOutputSchema::new(json!({
            "type": "object",
            "properties": {"answer": {"type": "string"}},
            "required": ["answer"],
            "additionalProperties": false
        }))?;
        let request = ModelProviderRequest::new(
            ModelId::try_from_string("gemma3:4b".to_owned())?,
            vec![message],
            Some(schema),
        )?;
        let chunk = ModelOutputChunk::try_from_string("secret output fixture".to_owned())?;

        assert!(!format!("{request:?}").contains("secret prompt fixture"));
        assert!(!format!("{chunk:?}").contains("secret output fixture"));
        assert!(ModelRequestTimeout::from_millis(0).is_err());
        assert!(ModelRequestTimeout::from_millis(300_001).is_err());
        assert!(
            ModelMessage::try_from_string(ModelMessageRole::User, "x".repeat(524_289)).is_err()
        );
        assert!(ModelOutputChunk::try_from_string("x".repeat(65_537)).is_err());
        Ok(())
    }
}
