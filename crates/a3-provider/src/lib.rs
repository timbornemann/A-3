//! Local model-provider adapters for A^3.

mod endpoint;
mod gemini;
mod ollama;

pub use endpoint::{
    LocalOnlyOllamaEndpointPolicy, OllamaEndpoint, OllamaEndpointError, OllamaEndpointPolicy,
    OllamaEndpointPolicyError, OllamaEndpointScope, OllamaSettingsEndpointValidator,
};
pub use gemini::{
    GeminiEndpoint, GeminiEndpointError, GeminiEndpointPolicy, GeminiEndpointPolicyError,
    GeminiEndpointScope, GeminiModelProvider, GeminiProviderCreateError,
    GeminiSettingsEndpointValidator, LocalOnlyGeminiEndpointPolicy, StandardGeminiEndpointPolicy,
};
pub use ollama::{OllamaModelProvider, OllamaProviderCreateError};
