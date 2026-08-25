use crate::{
    ModelOperationControl, ModelProvider, ModelProviderCompletion, ModelProviderFailure,
    ModelProviderRequest, ModelRequestTimeout, ProviderEvent,
};
use futures::future::{Either, select};
use futures::{FutureExt, StreamExt, pin_mut};
use std::error::Error;
use std::fmt;
use std::time::{Duration, Instant};

const MAX_TRANSIENT_MODEL_ATTEMPTS: u8 = 2;
const TRANSIENT_MODEL_RETRY_DELAY: Duration = Duration::from_secs(1);

pub(crate) struct CollectedModelOutput {
    output: String,
    completion: ModelProviderCompletion,
}

impl CollectedModelOutput {
    pub(crate) fn into_parts(self) -> (String, ModelProviderCompletion) {
        (self.output, self.completion)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ModelStreamCollectionFailure {
    Provider(ModelProviderFailure),
    OutputTooLarge(usize),
}

impl fmt::Display for ModelStreamCollectionFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Provider(_) => formatter.write_str("model stream collection failed"),
            Self::OutputTooLarge(actual) => {
                write!(
                    formatter,
                    "model output exceeded its byte limit at {actual} bytes"
                )
            }
        }
    }
}

impl Error for ModelStreamCollectionFailure {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Provider(source) => Some(source),
            Self::OutputTooLarge(_) => None,
        }
    }
}

pub(crate) async fn collect_model_stream(
    provider: &dyn ModelProvider,
    request: &ModelProviderRequest,
    timeout: ModelRequestTimeout,
    control: &dyn ModelOperationControl,
    max_output_bytes: usize,
) -> Result<CollectedModelOutput, ModelStreamCollectionFailure> {
    let deadline = Instant::now().checked_add(timeout.duration()).ok_or(
        ModelStreamCollectionFailure::Provider(ModelProviderFailure::TimedOut),
    )?;
    for attempt in 0..MAX_TRANSIENT_MODEL_ATTEMPTS {
        if control.is_cancelled() {
            return Err(ModelStreamCollectionFailure::Provider(
                ModelProviderFailure::Cancelled,
            ));
        }
        let attempt_timeout = remaining_timeout(deadline)?;
        match collect_model_stream_once(
            provider,
            request,
            attempt_timeout,
            control,
            max_output_bytes,
        )
        .await
        {
            Err(ModelStreamCollectionFailure::Provider(ModelProviderFailure::Unavailable))
                if attempt + 1 < MAX_TRANSIENT_MODEL_ATTEMPTS =>
            {
                wait_for_retry(control, deadline).await?;
            }
            result => return result,
        }
    }
    Err(ModelStreamCollectionFailure::Provider(
        ModelProviderFailure::Unavailable,
    ))
}

async fn wait_for_retry(
    control: &dyn ModelOperationControl,
    deadline: Instant,
) -> Result<(), ModelStreamCollectionFailure> {
    if deadline.saturating_duration_since(Instant::now()) <= TRANSIENT_MODEL_RETRY_DELAY {
        return Err(ModelStreamCollectionFailure::Provider(
            ModelProviderFailure::TimedOut,
        ));
    }
    let delay = tokio::time::sleep(TRANSIENT_MODEL_RETRY_DELAY).fuse();
    let cancelled = control.cancelled().fuse();
    pin_mut!(delay, cancelled);
    match select(cancelled, delay).await {
        Either::Left(((), _)) => Err(ModelStreamCollectionFailure::Provider(
            ModelProviderFailure::Cancelled,
        )),
        Either::Right(((), _)) => Ok(()),
    }
}

fn remaining_timeout(
    deadline: Instant,
) -> Result<ModelRequestTimeout, ModelStreamCollectionFailure> {
    let remaining = deadline.saturating_duration_since(Instant::now());
    let millis = u64::try_from(remaining.as_millis())
        .map_err(|_| ModelStreamCollectionFailure::Provider(ModelProviderFailure::TimedOut))?;
    ModelRequestTimeout::from_millis(millis)
        .map_err(|_| ModelStreamCollectionFailure::Provider(ModelProviderFailure::TimedOut))
}

async fn collect_model_stream_once(
    provider: &dyn ModelProvider,
    request: &ModelProviderRequest,
    timeout: ModelRequestTimeout,
    control: &dyn ModelOperationControl,
    max_output_bytes: usize,
) -> Result<CollectedModelOutput, ModelStreamCollectionFailure> {
    let mut stream = provider
        .stream(request, timeout, control)
        .await
        .map_err(ModelStreamCollectionFailure::Provider)?;
    let mut output = String::new();
    let mut completion = None;
    while let Some(event) = stream.next().await {
        match event.map_err(ModelStreamCollectionFailure::Provider)? {
            ProviderEvent::OutputText(chunk) if completion.is_none() => {
                let next = output
                    .len()
                    .checked_add(chunk.as_str().len())
                    .ok_or(ModelStreamCollectionFailure::OutputTooLarge(usize::MAX))?;
                if next > max_output_bytes {
                    return Err(ModelStreamCollectionFailure::OutputTooLarge(next));
                }
                output.push_str(chunk.as_str());
            }
            ProviderEvent::Completed(value) if completion.is_none() => {
                completion = Some(value);
            }
            ProviderEvent::OutputText(_) | ProviderEvent::Completed(_) => {
                return Err(ModelStreamCollectionFailure::Provider(
                    ModelProviderFailure::InvalidResponse,
                ));
            }
        }
    }
    let completion = completion.ok_or(ModelStreamCollectionFailure::Provider(
        ModelProviderFailure::InvalidResponse,
    ))?;
    Ok(CollectedModelOutput { output, completion })
}

#[cfg(test)]
mod tests {
    use super::{ModelStreamCollectionFailure, collect_model_stream};
    use crate::{
        ModelCancellationFuture, ModelFinishReason, ModelMessage, ModelMessageRole,
        ModelOperationControl, ModelOutputChunk, ModelProvider, ModelProviderCompletion,
        ModelProviderFailure, ModelProviderFuture, ModelProviderRequest, ModelProviderUsage,
        ModelRequestTimeout, ProviderEvent, ProviderEventStream,
    };
    use a3_domain::{
        ModelCapabilities, ModelContextLimit, ModelId, ModelOutputLimit, ModelParallelismLimit,
        ModelProfile, ModelProfileSettings, ModelPromptSchemaGrounding, ModelProviderId,
        ModelSamplingProfile, ModelStopSequences, ModelStructuredOutputCapability,
        ModelTemperature, ModelTokenCountingStrategy, ModelToolCallMode, ModelTopP,
    };
    use futures::stream;
    use futures::task::AtomicWaker;
    use std::collections::VecDeque;
    use std::error::Error;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::task::Poll;
    use std::time::Duration;

    #[derive(Debug)]
    enum Attempt {
        Failure(ModelProviderFailure),
        Events(Vec<Result<ProviderEvent, ModelProviderFailure>>),
    }

    #[derive(Debug)]
    struct SequencedProvider {
        provider_id: ModelProviderId,
        attempts: Mutex<VecDeque<Attempt>>,
        calls: AtomicUsize,
    }

    impl SequencedProvider {
        fn new(attempts: Vec<Attempt>) -> Result<Self, Box<dyn Error>> {
            Ok(Self {
                provider_id: ModelProviderId::try_from_string("test-provider".to_owned())?,
                attempts: Mutex::new(attempts.into()),
                calls: AtomicUsize::new(0),
            })
        }

        fn calls(&self) -> usize {
            self.calls.load(Ordering::Acquire)
        }
    }

    impl ModelProvider for SequencedProvider {
        fn provider_id(&self) -> &ModelProviderId {
            &self.provider_id
        }

        fn stream<'a>(
            &'a self,
            _request: &'a ModelProviderRequest,
            _timeout: ModelRequestTimeout,
            _control: &'a dyn ModelOperationControl,
        ) -> ModelProviderFuture<'a> {
            Box::pin(async move {
                self.calls.fetch_add(1, Ordering::AcqRel);
                let attempt = self
                    .attempts
                    .lock()
                    .map_err(|_| ModelProviderFailure::Unavailable)?
                    .pop_front()
                    .ok_or(ModelProviderFailure::InvalidResponse)?;
                match attempt {
                    Attempt::Failure(failure) => Err(failure),
                    Attempt::Events(events) => {
                        Ok(Box::pin(stream::iter(events)) as ProviderEventStream<'a>)
                    }
                }
            })
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
    fn one_transient_stream_failure_retries_from_an_empty_output() -> Result<(), Box<dyn Error>> {
        let provider = SequencedProvider::new(vec![
            Attempt::Events(vec![
                Ok(output("discarded partial output")?),
                Err(ModelProviderFailure::Unavailable),
            ]),
            Attempt::Events(vec![Ok(output("complete output")?), Ok(completed())]),
        ])?;
        let runtime = runtime()?;
        let collected = runtime.block_on(collect_model_stream(
            &provider,
            &request()?,
            ModelRequestTimeout::DEFAULT,
            &TestControl::default(),
            65_536,
        ))?;
        let (output, completion) = collected.into_parts();

        assert_eq!(output, "complete output");
        assert_eq!(completion.reason(), ModelFinishReason::Stop);
        assert_eq!(provider.calls(), 2);
        Ok(())
    }

    #[test]
    fn transient_retry_is_bounded_and_non_transient_failures_are_not_retried()
    -> Result<(), Box<dyn Error>> {
        let unavailable = SequencedProvider::new(vec![
            Attempt::Failure(ModelProviderFailure::Unavailable),
            Attempt::Failure(ModelProviderFailure::Unavailable),
            Attempt::Events(vec![Ok(completed())]),
        ])?;
        let runtime = runtime()?;
        assert!(matches!(
            runtime.block_on(collect_model_stream(
                &unavailable,
                &request()?,
                ModelRequestTimeout::DEFAULT,
                &TestControl::default(),
                65_536,
            )),
            Err(ModelStreamCollectionFailure::Provider(
                ModelProviderFailure::Unavailable
            ))
        ));
        assert_eq!(unavailable.calls(), 2);

        let exhausted_deadline = SequencedProvider::new(vec![
            Attempt::Failure(ModelProviderFailure::Unavailable),
            Attempt::Events(vec![Ok(completed())]),
        ])?;
        assert!(matches!(
            runtime.block_on(collect_model_stream(
                &exhausted_deadline,
                &request()?,
                ModelRequestTimeout::from_millis(100)?,
                &TestControl::default(),
                65_536,
            )),
            Err(ModelStreamCollectionFailure::Provider(
                ModelProviderFailure::TimedOut
            ))
        ));
        assert_eq!(exhausted_deadline.calls(), 1);

        for failure in [
            ModelProviderFailure::Rejected,
            ModelProviderFailure::InvalidResponse,
            ModelProviderFailure::TimedOut,
            ModelProviderFailure::Cancelled,
            ModelProviderFailure::EndpointDenied,
        ] {
            let provider = SequencedProvider::new(vec![
                Attempt::Failure(failure),
                Attempt::Events(vec![Ok(completed())]),
            ])?;
            assert!(matches!(
                runtime.block_on(collect_model_stream(
                    &provider,
                    &request()?,
                    ModelRequestTimeout::DEFAULT,
                    &TestControl::default(),
                    65_536,
                )),
                Err(ModelStreamCollectionFailure::Provider(actual)) if actual == failure
            ));
            assert_eq!(provider.calls(), 1);
        }
        Ok(())
    }

    #[test]
    fn cancellation_interrupts_the_retry_backoff_before_a_second_request()
    -> Result<(), Box<dyn Error>> {
        let provider = SequencedProvider::new(vec![
            Attempt::Failure(ModelProviderFailure::Unavailable),
            Attempt::Events(vec![Ok(completed())]),
        ])?;
        let request = request()?;
        let control = TestControl::default();
        let runtime = runtime()?;
        let result = runtime.block_on(async {
            let collection = collect_model_stream(
                &provider,
                &request,
                ModelRequestTimeout::DEFAULT,
                &control,
                65_536,
            );
            let cancellation = async {
                tokio::time::sleep(Duration::from_millis(10)).await;
                control.cancel();
            };
            let (result, ()) = futures::join!(collection, cancellation);
            result
        });

        assert!(matches!(
            result,
            Err(ModelStreamCollectionFailure::Provider(
                ModelProviderFailure::Cancelled
            ))
        ));
        assert_eq!(provider.calls(), 1);
        Ok(())
    }

    fn output(value: &str) -> Result<ProviderEvent, Box<dyn Error>> {
        Ok(ProviderEvent::OutputText(
            ModelOutputChunk::try_from_string(value.to_owned())?,
        ))
    }

    const fn completed() -> ProviderEvent {
        ProviderEvent::Completed(ModelProviderCompletion::new(
            ModelFinishReason::Stop,
            ModelProviderUsage::new(None, None),
        ))
    }

    fn request() -> Result<ModelProviderRequest, Box<dyn Error>> {
        let profile = ModelProfile::from_probe(
            ModelProviderId::try_from_string("test-provider".to_owned())?,
            ModelId::try_from_string("test-model".to_owned())?,
            ModelProfileSettings::new(
                ModelContextLimit::new(16_384)?,
                ModelOutputLimit::new(2_048)?,
                ModelTokenCountingStrategy::ConservativeUtf8BytesV1,
                ModelParallelismLimit::new(1)?,
                ModelSamplingProfile::new(
                    ModelTemperature::from_milli(0)?,
                    ModelTopP::from_milli(1_000)?,
                ),
                ModelStopSequences::empty(),
                ModelPromptSchemaGrounding::RepeatSchemaInPrompt,
            )?,
            ModelCapabilities::new(
                ModelStructuredOutputCapability::Verified,
                ModelToolCallMode::Disabled,
            ),
        );
        Ok(ModelProviderRequest::new(
            profile,
            vec![ModelMessage::try_from_string(
                ModelMessageRole::User,
                "bounded test request".to_owned(),
            )?],
            None,
        )?)
    }

    fn runtime() -> Result<tokio::runtime::Runtime, Box<dyn Error>> {
        Ok(tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()?)
    }
}
