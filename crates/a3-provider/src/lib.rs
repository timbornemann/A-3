//! Native model-provider adapters for A^3.

mod endpoint;
mod gemini;
mod ollama;
mod openai;
mod openai_compatible;

pub use endpoint::{
    ExactOllamaEndpointPolicy, LocalOnlyOllamaEndpointPolicy, OllamaEndpoint, OllamaEndpointError,
    OllamaEndpointPolicy, OllamaEndpointPolicyError, OllamaEndpointScope,
    OllamaSettingsEndpointValidator,
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
pub use openai_compatible::{
    ExactOpenAiCompatibleEndpointPolicy, LocalOnlyOpenAiCompatibleEndpointPolicy,
    OpenAiCompatibleEndpoint, OpenAiCompatibleEndpointError, OpenAiCompatibleEndpointPolicy,
    OpenAiCompatibleEndpointPolicyError, OpenAiCompatibleEndpointScope,
    OpenAiCompatibleModelProvider, OpenAiCompatibleProviderCreateError,
    OpenAiCompatibleSettingsEndpointValidator,
};
