//! Reusable dev-only model-provider stubs and neutral port contracts.

use a3_application::{
    ModelCapabilityObservation, ModelCapabilityProbe, ModelCapabilityProbeFuture,
    ModelCapabilityProbeRequest, ModelOperationControl, ModelProvider, ModelProviderFailure,
    ModelProviderFuture, ModelProviderRequest, ModelRequestTimeout, ProviderEvent,
    ProviderEventStream,
};
use a3_domain::ModelProviderId;
use std::sync::Mutex;

/// Deterministic behavior selected for a neutral stub provider.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StubModelProviderBehavior {
    /// Emits the exact finite event script in order.
    Events(Vec<ProviderEvent>),
    /// Rejects stream establishment with one normalized failure.
    Failure(ModelProviderFailure),
    /// Establishes a stream that remains pending until cooperative cancellation.
    WaitForCancellation,
}

/// Deterministic behavior selected for a neutral capability-probe stub.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StubModelCapabilityProbeBehavior {
    /// Returns exact provider-neutral metadata and live capability evidence.
    Observation(ModelCapabilityObservation),
    /// Rejects the probe with one normalized provider failure.
    Failure(ModelProviderFailure),
    /// Remains pending until cooperative cancellation is requested.
    WaitForCancellation,
}

/// Content-free metadata retained for one capability-probe invocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StubModelCapabilityProbeCall {
    context_limit: u32,
    output_limit: u32,
    timeout: ModelRequestTimeout,
}

impl StubModelCapabilityProbeCall {
    /// Returns the requested effective context window.
    #[must_use]
    pub const fn context_limit(self) -> u32 {
        self.context_limit
    }

    /// Returns the requested generation bound.
    #[must_use]
    pub const fn output_limit(self) -> u32 {
        self.output_limit
    }

    /// Returns the exact neutral timeout supplied by the consumer.
    #[must_use]
    pub const fn timeout(self) -> ModelRequestTimeout {
        self.timeout
    }
}

/// Deterministic provider-neutral capability probe for application contract tests.
pub struct StubModelCapabilityProbe {
    provider_id: ModelProviderId,
    behavior: StubModelCapabilityProbeBehavior,
    calls: Mutex<Vec<StubModelCapabilityProbeCall>>,
}

impl StubModelCapabilityProbe {
    /// Creates one stub that never inspects a model name to choose its capability result.
    #[must_use]
    pub fn new(provider_id: ModelProviderId, behavior: StubModelCapabilityProbeBehavior) -> Self {
        Self {
            provider_id,
            behavior,
            calls: Mutex::new(Vec::new()),
        }
    }

    /// Returns content-free call metadata in invocation order.
    pub fn calls(
        &self,
    ) -> Result<Vec<StubModelCapabilityProbeCall>, StubModelProviderInspectError> {
        self.calls
            .lock()
            .map(|calls| calls.clone())
            .map_err(|_| StubModelProviderInspectError)
    }
}

impl std::fmt::Debug for StubModelCapabilityProbe {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let call_count = self.calls.lock().map_or(0, |calls| calls.len());
        formatter
            .debug_struct("StubModelCapabilityProbe")
            .field("provider_id", &self.provider_id)
            .field("behavior", &self.behavior)
            .field("call_count", &call_count)
            .finish()
    }
}

impl ModelCapabilityProbe for StubModelCapabilityProbe {
    fn provider_id(&self) -> &ModelProviderId {
        &self.provider_id
    }

    fn probe<'a>(
        &'a self,
        request: &'a ModelCapabilityProbeRequest,
        timeout: ModelRequestTimeout,
        control: &'a dyn ModelOperationControl,
    ) -> ModelCapabilityProbeFuture<'a> {
        let recorded = self.calls.lock().map(|mut calls| {
            calls.push(StubModelCapabilityProbeCall {
                context_limit: request.settings().context_limit().get(),
                output_limit: request.settings().output_limit().get(),
                timeout,
            });
        });
        if recorded.is_err() {
            return Box::pin(async { Err(ModelProviderFailure::Rejected) });
        }
        if control.is_cancelled() {
            return Box::pin(async { Err(ModelProviderFailure::Cancelled) });
        }
        match self.behavior {
            StubModelCapabilityProbeBehavior::Observation(observation) => {
                Box::pin(async move { Ok(observation) })
            }
            StubModelCapabilityProbeBehavior::Failure(failure) => {
                Box::pin(async move { Err(failure) })
            }
            StubModelCapabilityProbeBehavior::WaitForCancellation => Box::pin(async move {
                control.cancelled().await;
                Err(ModelProviderFailure::Cancelled)
            }),
        }
    }
}

/// Request metadata retained by the stub without retaining prompt or schema content.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StubModelProviderCall {
    message_count: usize,
    has_structured_output: bool,
    timeout: ModelRequestTimeout,
}

impl StubModelProviderCall {
    /// Returns the number of bounded input messages.
    #[must_use]
    pub const fn message_count(self) -> usize {
        self.message_count
    }

    /// Returns whether the request carried a structured-output schema.
    #[must_use]
    pub const fn has_structured_output(self) -> bool {
        self.has_structured_output
    }

    /// Returns the exact neutral timeout supplied by the consumer.
    #[must_use]
    pub const fn timeout(self) -> ModelRequestTimeout {
        self.timeout
    }
}

/// In-memory provider for deterministic consumer tests; never performs network access.
pub struct StubModelProvider {
    provider_id: ModelProviderId,
    behavior: StubModelProviderBehavior,
    calls: Mutex<Vec<StubModelProviderCall>>,
}

impl StubModelProvider {
    /// Creates one stub with a safe caller-selected identity and deterministic behavior.
    #[must_use]
    pub fn new(provider_id: ModelProviderId, behavior: StubModelProviderBehavior) -> Self {
        Self {
            provider_id,
            behavior,
            calls: Mutex::new(Vec::new()),
        }
    }

    /// Returns content-free call metadata in invocation order.
    pub fn calls(&self) -> Result<Vec<StubModelProviderCall>, StubModelProviderInspectError> {
        self.calls
            .lock()
            .map(|calls| calls.clone())
            .map_err(|_| StubModelProviderInspectError)
    }
}

impl std::fmt::Debug for StubModelProvider {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let call_count = self.calls.lock().map_or(0, |calls| calls.len());
        formatter
            .debug_struct("StubModelProvider")
            .field("provider_id", &self.provider_id)
            .field("behavior", &self.behavior)
            .field("call_count", &call_count)
            .finish()
    }
}

impl ModelProvider for StubModelProvider {
    fn provider_id(&self) -> &ModelProviderId {
        &self.provider_id
    }

    fn stream<'a>(
        &'a self,
        request: &'a ModelProviderRequest,
        timeout: ModelRequestTimeout,
        control: &'a dyn ModelOperationControl,
    ) -> ModelProviderFuture<'a> {
        let recorded = self.calls.lock().map(|mut calls| {
            calls.push(StubModelProviderCall {
                message_count: request.messages().len(),
                has_structured_output: request.structured_output().is_some(),
                timeout,
            });
        });
        if recorded.is_err() {
            return Box::pin(async { Err(ModelProviderFailure::Rejected) });
        }
        if control.is_cancelled() {
            return Box::pin(async { Err(ModelProviderFailure::Cancelled) });
        }
        match &self.behavior {
            StubModelProviderBehavior::Events(events) => {
                let events = events.clone();
                Box::pin(async move {
                    Ok(Box::pin(futures::stream::iter(events.into_iter().map(Ok)))
                        as ProviderEventStream<'a>)
                })
            }
            StubModelProviderBehavior::Failure(failure) => {
                let failure = *failure;
                Box::pin(async move { Err(failure) })
            }
            StubModelProviderBehavior::WaitForCancellation => Box::pin(async move {
                let stream = futures::stream::once(async move {
                    control.cancelled().await;
                    Err(ModelProviderFailure::Cancelled)
                });
                Ok(Box::pin(stream) as ProviderEventStream<'a>)
            }),
        }
    }
}

/// Stub call metadata could not be inspected because its test lock was poisoned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StubModelProviderInspectError;

impl std::fmt::Display for StubModelProviderInspectError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("stub model-provider call metadata is unavailable")
    }
}

impl std::error::Error for StubModelProviderInspectError {}

#[cfg(test)]
mod tests {
    use super::{
        StubModelCapabilityProbe, StubModelCapabilityProbeBehavior, StubModelProvider,
        StubModelProviderBehavior,
    };
    use a3_application::ModelCapabilityProbeRequest;
    use a3_application::{
        ModelCancellationFuture, ModelCapabilityObservation, ModelFinishReason, ModelMessage,
        ModelMessageRole, ModelOperationControl, ModelOutputChunk, ModelProvider,
        ModelProviderCompletion, ModelProviderFailure, ModelProviderRequest, ModelProviderUsage,
        ModelRequestTimeout, ProbeModelProfile, ProbeModelProfileFailure, ProviderEvent,
        ReportedModelContextLimit,
    };
    use a3_domain::{
        ModelCapabilities, ModelContextLimit, ModelId, ModelOutputLimit, ModelParallelismLimit,
        ModelProfile, ModelProfileOverride, ModelProfileOverrideRevision, ModelProfileSettings,
        ModelPromptSchemaGrounding, ModelProviderId, ModelSamplingProfile, ModelStopSequences,
        ModelStructuredOutputCapability, ModelTemperature, ModelTokenCountingStrategy,
        ModelToolCallMode, ModelTopP,
    };
    use futures::FutureExt;
    use futures::StreamExt;
    use futures::task::AtomicWaker;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::task::Poll;

    #[derive(Debug)]
    struct NeverCancelled;

    impl ModelOperationControl for NeverCancelled {
        fn is_cancelled(&self) -> bool {
            false
        }

        fn cancelled(&self) -> ModelCancellationFuture<'_> {
            Box::pin(futures::future::pending())
        }
    }

    #[derive(Debug, Default)]
    struct TestControl {
        cancelled: AtomicBool,
        waiter: AtomicWaker,
    }

    impl TestControl {
        fn cancel(&self) {
            self.cancelled.store(true, Ordering::Release);
            self.waiter.wake();
        }
    }

    impl ModelOperationControl for TestControl {
        fn is_cancelled(&self) -> bool {
            self.cancelled.load(Ordering::Acquire)
        }

        fn cancelled(&self) -> ModelCancellationFuture<'_> {
            Box::pin(futures::future::poll_fn(|context| {
                if self.is_cancelled() {
                    return Poll::Ready(());
                }
                self.waiter.register(context.waker());
                if self.is_cancelled() {
                    Poll::Ready(())
                } else {
                    Poll::Pending
                }
            }))
        }
    }

    fn profile_settings(
        context_limit: u32,
    ) -> Result<ModelProfileSettings, Box<dyn std::error::Error>> {
        Ok(ModelProfileSettings::new(
            ModelContextLimit::new(context_limit)?,
            ModelOutputLimit::new(2_048)?,
            ModelTokenCountingStrategy::ConservativeUtf8BytesV1,
            ModelParallelismLimit::new(1)?,
            ModelSamplingProfile::new(
                ModelTemperature::from_milli(0)?,
                ModelTopP::from_milli(1_000)?,
            ),
            ModelStopSequences::empty(),
            ModelPromptSchemaGrounding::RepeatSchemaInPrompt,
        )?)
    }

    fn model_profile(
        provider_id: &str,
        structured_output: ModelStructuredOutputCapability,
    ) -> Result<ModelProfile, Box<dyn std::error::Error>> {
        Ok(ModelProfile::from_probe(
            ModelProviderId::try_from_string(provider_id.to_owned())?,
            ModelId::try_from_string("test-model".to_owned())?,
            profile_settings(16_384)?,
            ModelCapabilities::new(structured_output, ModelToolCallMode::Disabled),
        ))
    }

    #[test]
    fn scripted_stub_emits_exact_events_and_retains_no_prompt()
    -> Result<(), Box<dyn std::error::Error>> {
        let events = vec![
            ProviderEvent::OutputText(ModelOutputChunk::try_from_string("answer".to_owned())?),
            ProviderEvent::Completed(ModelProviderCompletion::new(
                ModelFinishReason::Stop,
                ModelProviderUsage::new(Some(4), Some(1)),
            )),
        ];
        let provider = StubModelProvider::new(
            ModelProviderId::try_from_string("contract-stub".to_owned())?,
            StubModelProviderBehavior::Events(events.clone()),
        );
        let request = ModelProviderRequest::new(
            model_profile(
                "contract-stub",
                ModelStructuredOutputCapability::Unavailable,
            )?,
            vec![ModelMessage::try_from_string(
                ModelMessageRole::User,
                "secret prompt fixture".to_owned(),
            )?],
            None,
        )?;
        let control = NeverCancelled;
        let returned = futures::executor::block_on(async {
            provider
                .stream(&request, ModelRequestTimeout::DEFAULT, &control)
                .await?
                .collect::<Vec<_>>()
                .await
                .into_iter()
                .collect::<Result<Vec<_>, ModelProviderFailure>>()
        })?;

        assert_eq!(returned, events);
        assert_eq!(provider.calls()?.len(), 1);
        assert!(!format!("{provider:?}").contains("secret prompt fixture"));
        Ok(())
    }

    #[test]
    fn pending_stub_wakes_and_normalizes_cancellation() -> Result<(), Box<dyn std::error::Error>> {
        let provider = StubModelProvider::new(
            ModelProviderId::try_from_string("pending-stub".to_owned())?,
            StubModelProviderBehavior::WaitForCancellation,
        );
        let request = ModelProviderRequest::new(
            model_profile("pending-stub", ModelStructuredOutputCapability::Unavailable)?,
            vec![ModelMessage::try_from_string(
                ModelMessageRole::User,
                "bounded prompt".to_owned(),
            )?],
            None,
        )?;
        let control = TestControl::default();
        let mut stream = futures::executor::block_on(provider.stream(
            &request,
            ModelRequestTimeout::DEFAULT,
            &control,
        ))?;

        assert!(stream.next().now_or_never().is_none());
        control.cancel();
        assert_eq!(
            futures::executor::block_on(stream.next()),
            Some(Err(ModelProviderFailure::Cancelled))
        );
        Ok(())
    }

    #[test]
    fn failure_stub_returns_only_the_selected_normalized_failure()
    -> Result<(), Box<dyn std::error::Error>> {
        let provider = StubModelProvider::new(
            ModelProviderId::try_from_string("failure-stub".to_owned())?,
            StubModelProviderBehavior::Failure(ModelProviderFailure::Unavailable),
        );
        let request = ModelProviderRequest::new(
            model_profile("failure-stub", ModelStructuredOutputCapability::Unavailable)?,
            vec![ModelMessage::try_from_string(
                ModelMessageRole::User,
                "bounded prompt".to_owned(),
            )?],
            None,
        )?;

        assert!(matches!(
            futures::executor::block_on(provider.stream(
                &request,
                ModelRequestTimeout::DEFAULT,
                &NeverCancelled,
            )),
            Err(ModelProviderFailure::Unavailable)
        ));
        Ok(())
    }

    #[test]
    fn capability_stub_builds_a_versioned_profile_from_observed_evidence()
    -> Result<(), Box<dyn std::error::Error>> {
        let probe = StubModelCapabilityProbe::new(
            ModelProviderId::try_from_string("contract-stub".to_owned())?,
            StubModelCapabilityProbeBehavior::Observation(ModelCapabilityObservation::new(
                Some(ReportedModelContextLimit::new(32_768)?),
                ModelCapabilities::new(
                    ModelStructuredOutputCapability::Verified,
                    ModelToolCallMode::NativeProviderReported,
                ),
            )),
        );
        let request = ModelCapabilityProbeRequest::new(
            ModelId::try_from_string("opaque-model-name".to_owned())?,
            profile_settings(16_384)?,
        );
        let profile = futures::executor::block_on(ProbeModelProfile::new(&probe).execute(
            &request,
            ModelRequestTimeout::DEFAULT,
            &NeverCancelled,
        ))?;

        assert_eq!(profile.provider_id().as_str(), "contract-stub");
        assert_eq!(profile.model_id().as_str(), "opaque-model-name");
        assert!(profile.executable_actions_enabled());
        assert_eq!(probe.calls()?.len(), 1);
        assert_eq!(probe.calls()?[0].context_limit(), 16_384);
        Ok(())
    }

    #[test]
    fn failed_structured_probe_and_manual_override_cannot_enable_actions()
    -> Result<(), Box<dyn std::error::Error>> {
        let probe = StubModelCapabilityProbe::new(
            ModelProviderId::try_from_string("contract-stub".to_owned())?,
            StubModelCapabilityProbeBehavior::Observation(ModelCapabilityObservation::new(
                None,
                ModelCapabilities::new(
                    ModelStructuredOutputCapability::Unavailable,
                    ModelToolCallMode::Disabled,
                ),
            )),
        );
        let request = ModelCapabilityProbeRequest::new(
            ModelId::try_from_string("name-does-not-decide-capabilities".to_owned())?,
            profile_settings(16_384)?,
        );
        let profile = futures::executor::block_on(ProbeModelProfile::new(&probe).execute(
            &request,
            ModelRequestTimeout::DEFAULT,
            &NeverCancelled,
        ))?;
        let overridden = profile.apply_override(
            ModelProfileOverride::new(
                Some(ModelContextLimit::new(32_768)?),
                None,
                None,
                None,
                None,
                None,
                None,
            )?,
            ModelProfileOverrideRevision::new(1)?,
        )?;

        assert!(!profile.executable_actions_enabled());
        assert!(!overridden.executable_actions_enabled());
        assert_eq!(overridden.capabilities(), profile.capabilities());
        Ok(())
    }

    #[test]
    fn profile_creation_rejects_context_above_explicit_provider_metadata()
    -> Result<(), Box<dyn std::error::Error>> {
        let probe = StubModelCapabilityProbe::new(
            ModelProviderId::try_from_string("contract-stub".to_owned())?,
            StubModelCapabilityProbeBehavior::Observation(ModelCapabilityObservation::new(
                Some(ReportedModelContextLimit::new(16_384)?),
                ModelCapabilities::new(
                    ModelStructuredOutputCapability::Verified,
                    ModelToolCallMode::Disabled,
                ),
            )),
        );
        let request = ModelCapabilityProbeRequest::new(
            ModelId::try_from_string("test-model".to_owned())?,
            profile_settings(32_768)?,
        );

        assert!(matches!(
            futures::executor::block_on(ProbeModelProfile::new(&probe).execute(
                &request,
                ModelRequestTimeout::DEFAULT,
                &NeverCancelled,
            )),
            Err(ProbeModelProfileFailure::ContextLimitExceedsProvider { .. })
        ));
        Ok(())
    }
}
