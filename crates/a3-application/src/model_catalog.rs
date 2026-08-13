use crate::{ModelOperationControl, ModelProviderFailure, ModelRequestTimeout};
use a3_domain::{ModelId, ModelProviderId};
use std::fmt;
use std::future::Future;
use std::pin::Pin;

const MAX_DISCOVERED_MODELS: usize = 256;

/// Future returned by a provider-neutral local model-catalog adapter.
pub type ModelCatalogFuture<'a> =
    Pin<Box<dyn Future<Output = Result<ProviderModelCatalog, ModelProviderFailure>> + Send + 'a>>;

/// Bounded provider-neutral list of locally advertised model identities.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderModelCatalog {
    provider_id: ModelProviderId,
    model_ids: Vec<ModelId>,
    truncated: bool,
}

impl ProviderModelCatalog {
    /// Canonicalizes one provider observation without inferring any model capability.
    #[must_use]
    pub fn from_observation(
        provider_id: ModelProviderId,
        mut model_ids: Vec<ModelId>,
        source_truncated: bool,
    ) -> Self {
        model_ids.sort();
        model_ids.dedup();
        let truncated = source_truncated || model_ids.len() > MAX_DISCOVERED_MODELS;
        model_ids.truncate(MAX_DISCOVERED_MODELS);
        Self {
            provider_id,
            model_ids,
            truncated,
        }
    }

    /// Returns the credential- and endpoint-free provider identity.
    #[must_use]
    pub const fn provider_id(&self) -> &ModelProviderId {
        &self.provider_id
    }

    /// Returns the canonical unique model-ID prefix.
    #[must_use]
    pub fn model_ids(&self) -> &[ModelId] {
        &self.model_ids
    }

    /// Returns whether the provider observation exceeded the V1 result boundary.
    #[must_use]
    pub const fn truncated(&self) -> bool {
        self.truncated
    }
}

/// Provider-neutral port for one explicit, bounded model-catalog request.
pub trait ModelCatalogProvider: fmt::Debug + Send + Sync {
    /// Returns the adapter identity without endpoint material.
    fn provider_id(&self) -> &ModelProviderId;

    /// Lists locally advertised model IDs under one total deadline and cancellation boundary.
    fn discover_models<'a>(
        &'a self,
        timeout: ModelRequestTimeout,
        control: &'a dyn ModelOperationControl,
    ) -> ModelCatalogFuture<'a>;
}

/// Executes an explicit model-catalog read through a concrete provider adapter.
#[derive(Debug, Clone, Copy)]
pub struct DiscoverProviderModels<'a> {
    provider: &'a dyn ModelCatalogProvider,
}

impl<'a> DiscoverProviderModels<'a> {
    /// Binds the use case to one concrete provider capability.
    #[must_use]
    pub const fn new(provider: &'a dyn ModelCatalogProvider) -> Self {
        Self { provider }
    }

    /// Returns a bounded catalog only when the adapter preserves its provider identity.
    pub async fn execute(
        &self,
        timeout: ModelRequestTimeout,
        control: &dyn ModelOperationControl,
    ) -> Result<ProviderModelCatalog, ModelProviderFailure> {
        let catalog = self.provider.discover_models(timeout, control).await?;
        if catalog.provider_id() != self.provider.provider_id() {
            return Err(ModelProviderFailure::InvalidResponse);
        }
        Ok(catalog)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        DiscoverProviderModels, ModelCatalogFuture, ModelCatalogProvider, ProviderModelCatalog,
    };
    use crate::{
        ModelCancellationFuture, ModelOperationControl, ModelProviderFailure, ModelRequestTimeout,
    };
    use a3_domain::{ModelId, ModelProviderId};

    #[derive(Debug)]
    struct Control;

    impl ModelOperationControl for Control {
        fn is_cancelled(&self) -> bool {
            false
        }

        fn cancelled(&self) -> ModelCancellationFuture<'_> {
            Box::pin(std::future::pending())
        }
    }

    #[derive(Debug)]
    struct Stub {
        provider_id: ModelProviderId,
        result_provider_id: ModelProviderId,
    }

    impl ModelCatalogProvider for Stub {
        fn provider_id(&self) -> &ModelProviderId {
            &self.provider_id
        }

        fn discover_models<'a>(
            &'a self,
            _timeout: ModelRequestTimeout,
            _control: &'a dyn ModelOperationControl,
        ) -> ModelCatalogFuture<'a> {
            Box::pin(async move {
                Ok(ProviderModelCatalog::from_observation(
                    self.result_provider_id.clone(),
                    vec![
                        ModelId::try_from_string("zeta:latest".to_owned())
                            .map_err(|_| ModelProviderFailure::InvalidResponse)?,
                        ModelId::try_from_string("alpha:7b".to_owned())
                            .map_err(|_| ModelProviderFailure::InvalidResponse)?,
                        ModelId::try_from_string("alpha:7b".to_owned())
                            .map_err(|_| ModelProviderFailure::InvalidResponse)?,
                    ],
                    false,
                ))
            })
        }
    }

    fn provider_id(value: &str) -> Result<ModelProviderId, a3_domain::ModelIdentityError> {
        ModelProviderId::try_from_string(value.to_owned())
    }

    #[test]
    fn catalog_is_sorted_unique_and_bounded() -> Result<(), a3_domain::ModelIdentityError> {
        let mut models = (0..300)
            .map(|index| ModelId::try_from_string(format!("model-{index:03}")))
            .collect::<Result<Vec<_>, _>>()?;
        models.push(ModelId::try_from_string("model-000".to_owned())?);
        let catalog = ProviderModelCatalog::from_observation(provider_id("ollama")?, models, false);
        assert_eq!(catalog.model_ids().len(), 256);
        assert_eq!(catalog.model_ids()[0].as_str(), "model-000");
        assert_eq!(catalog.model_ids()[255].as_str(), "model-255");
        assert!(catalog.truncated());
        Ok(())
    }

    #[test]
    fn use_case_rejects_adapter_identity_drift() -> Result<(), Box<dyn std::error::Error>> {
        let provider = Stub {
            provider_id: provider_id("ollama")?,
            result_provider_id: provider_id("other")?,
        };
        let result = futures::executor::block_on(
            DiscoverProviderModels::new(&provider)
                .execute(ModelRequestTimeout::from_millis(1_000)?, &Control),
        );
        assert_eq!(result, Err(ModelProviderFailure::InvalidResponse));
        Ok(())
    }
}
