//! Content-free request measurements for the explicitly selected live fixture only.
use a3_application::{
    ModelOperationControl, ModelProvider, ModelProviderFuture, ModelProviderRequest,
    ModelRequestTimeout, ProviderEvent,
};
use a3_domain::ModelProviderId;
use futures::StreamExt;
use std::{sync::Arc, time::Instant};

pub(super) fn observe(provider: Arc<dyn ModelProvider>, enabled: bool) -> Arc<dyn ModelProvider> {
    if enabled {
        Arc::new(Observed(provider))
    } else {
        provider
    }
}

#[derive(Debug)]
struct Observed(Arc<dyn ModelProvider>);

impl ModelProvider for Observed {
    fn provider_id(&self) -> &ModelProviderId {
        self.0.provider_id()
    }
    fn stream<'a>(
        &'a self,
        request: &'a ModelProviderRequest,
        timeout: ModelRequestTimeout,
        control: &'a dyn ModelOperationControl,
    ) -> ModelProviderFuture<'a> {
        Box::pin(async move {
            let stage = match request
                .structured_output()
                .and_then(|s| s.value().get("title"))
                .and_then(|s| s.as_str())
            {
                Some("A^3 ActionChoice V1") => "choice",
                Some("A^3 ActionArguments V1") => "arguments",
                Some("A^3 AfterChange V1") => "after_change",
                Some("A^3 AgentAction V5") => "action",
                _ => "research",
            };
            println!(
                "A3_LIVE_MODEL request stage={stage} message_bytes={} schema_bytes={}",
                request
                    .messages()
                    .iter()
                    .map(|m| m.content().len())
                    .sum::<usize>(),
                request
                    .structured_output()
                    .map(|s| s.value().to_string().len())
                    .unwrap_or(0)
            );
            let started = Instant::now();
            let stream = self.0.stream(request, timeout, control).await?;
            Ok(Box::pin(stream.inspect(move |event| {
                if let Ok(ProviderEvent::Completed(completion)) = event {
                    println!("A3_LIVE_MODEL completion stage={stage} reason={:?} prompt={:?} output={:?} millis={}",
                        completion.reason(), completion.usage().prompt_tokens(),completion.usage().output_tokens(),started.elapsed().as_millis());
                }
            })) as a3_application::ProviderEventStream<'a>)
        })
    }
}
