//! Reusable dev-only model-provider stubs and neutral port contracts.

use a3_application::{
    ModelOperationControl, ModelProvider, ModelProviderFailure, ModelProviderFuture,
    ModelProviderRequest, ModelRequestTimeout, ProviderEvent, ProviderEventStream,
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
    use super::{StubModelProvider, StubModelProviderBehavior};
    use a3_application::{
        ModelCancellationFuture, ModelFinishReason, ModelMessage, ModelMessageRole,
        ModelOperationControl, ModelOutputChunk, ModelProvider, ModelProviderCompletion,
        ModelProviderFailure, ModelProviderRequest, ModelProviderUsage, ModelRequestTimeout,
        ProviderEvent,
    };
    use a3_domain::{ModelId, ModelProviderId};
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
            ModelId::try_from_string("test-model".to_owned())?,
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
            ModelId::try_from_string("test-model".to_owned())?,
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
            ModelId::try_from_string("test-model".to_owned())?,
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
}
