//! Native model-provider adapters for A^3.

mod endpoint;
mod gemini;
mod ollama;
mod openai;

pub use endpoint::{
    LocalOnlyOllamaEndpointPolicy, OllamaEndpoint, OllamaEndpointError, OllamaEndpointPolicy,
    OllamaEndpointPolicyError, OllamaEndpointScope, OllamaSettingsEndpointValidator,
};
pub use gemini::{
    ExactGeminiEndpointPolicy, GeminiEndpoint, GeminiEndpointError, GeminiEndpointPolicy,
    GeminiEndpointPolicyError, GeminiEndpointScope, GeminiModelProvider, GeminiProviderCreateError,
    GeminiSettingsEndpointValidator, LocalOnlyGeminiEndpointPolicy, StandardGeminiEndpointPolicy,
};
pub use ollama::{OllamaModelProvider, OllamaProviderCreateError};
pub use openai::{
    ExactOpenAiEndpointPolicy, LocalOnlyOpenAiEndpointPolicy, OpenAiEndpoint, OpenAiEndpointError,
    OpenAiEndpointPolicy, OpenAiEndpointPolicyError, OpenAiEndpointScope, OpenAiModelProvider,
    OpenAiProviderCreateError, OpenAiSettingsEndpointValidator, StandardOpenAiEndpointPolicy,
};
