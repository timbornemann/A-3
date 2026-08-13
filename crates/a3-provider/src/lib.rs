//! Local model-provider adapters for A^3.

mod endpoint;
mod ollama;

pub use endpoint::{
    LocalOnlyOllamaEndpointPolicy, OllamaEndpoint, OllamaEndpointError, OllamaEndpointPolicy,
    OllamaEndpointPolicyError, OllamaEndpointScope, OllamaSettingsEndpointValidator,
};
pub use ollama::{OllamaModelProvider, OllamaProviderCreateError};
