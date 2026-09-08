//! Native evaluation only: bounded output-shape metadata, never an admission or repair path.
use a3_application::{
    ModelOperationControl, ModelProvider, ModelProviderCompletion, ModelProviderFuture,
    ModelProviderRequest, ModelRequestTimeout, ProviderEvent, ProviderEventStream,
};
use a3_domain::ModelProviderId;
use futures::StreamExt;
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};

const MAX_PREFIX_BYTES: usize = 16 * 1024;
const MAX_RECORDS: usize = 48;
pub(super) type Records = Arc<Mutex<Vec<Value>>>;

#[derive(Debug)]
pub(super) struct ObservedProvider {
    inner: Arc<dyn ModelProvider>,
    records: Records,
}

impl ObservedProvider {
    pub(super) fn new(inner: Arc<dyn ModelProvider>, records: Records) -> Self {
        Self { inner, records }
    }
}

impl ModelProvider for ObservedProvider {
    fn provider_id(&self) -> &ModelProviderId {
        self.inner.provider_id()
    }

    fn stream<'a>(
        &'a self,
        request: &'a ModelProviderRequest,
        timeout: ModelRequestTimeout,
        control: &'a dyn ModelOperationControl,
    ) -> ModelProviderFuture<'a> {
        Box::pin(async move {
            let stream = self.inner.stream(request, timeout, control).await?;
            Ok(observe_stream(stream, &self.records))
        })
    }
}

fn observe_stream<'a>(
    stream: ProviderEventStream<'a>,
    records: &'a Records,
) -> ProviderEventStream<'a> {
    let mut shape = OutputShape::default();
    // inspect borrows each event; the original stream, errors and completion are
    // forwarded unchanged, under the same caller-owned timeout and cancellation.
    Box::pin(stream.inspect(move |event| match event {
        Ok(ProviderEvent::OutputText(chunk)) => shape.push(chunk.as_str()),
        Ok(ProviderEvent::Completed(done)) => {
            if let Ok(mut records) = records.lock()
                && records.len() < MAX_RECORDS
            {
                records.push(shape.summary(*done));
            }
        }
        Err(_) => {}
    }))
}

#[derive(Default)]
struct OutputShape {
    bytes: usize,
    chunks: usize,
    prefix: String,
}

impl OutputShape {
    fn push(&mut self, chunk: &str) {
        let remaining = MAX_PREFIX_BYTES.saturating_sub(self.bytes);
        self.bytes = self.bytes.saturating_add(chunk.len());
        self.chunks = self.chunks.saturating_add(1);
        let mut end = remaining.min(chunk.len());
        while !chunk.is_char_boundary(end) {
            end -= 1;
        }
        self.prefix.push_str(&chunk[..end]);
    }

    fn summary(&self, done: ModelProviderCompletion) -> Value {
        let complete_capture = self.bytes == self.prefix.len();
        json!({
            "finish":format!("{:?}", done.reason()),
            "output_bytes":self.bytes,"chunks":self.chunks,
            "captured_prefix_bytes":self.prefix.len(),"capture_complete":complete_capture,
            "valid_json":complete_capture.then(|| serde_json::from_str::<Value>(&self.prefix).is_ok()),
            "prompt_tokens":done.usage().prompt_tokens(),"output_tokens":done.usage().output_tokens(),
            // Lexical observations of fixed protocol strings only, not parsed evidence
            // counts, validated fields or semantic proof. Never store a prefix or value.
            "quoted_anchor_ref_occurrences":self.prefix.matches("\"anchor_ref\"").count(),
            "quoted_text_occurrences":self.prefix.matches("\"text\"").count()
        })
    }
}

#[test]
fn research_stream_shape_is_bounded_content_free_and_not_an_admission()
-> Result<(), Box<dyn std::error::Error>> {
    use a3_application::{ModelFinishReason, ModelProviderUsage};
    let stopped = ModelProviderCompletion::new(
        ModelFinishReason::Stop,
        ModelProviderUsage::new(Some(40), Some(8)),
    );
    let mut shape = OutputShape::default();
    shape.push("{\"text\":\"private sentinel\",\"evidence\":[{\"anchor_");
    shape.push("ref\":\"E1\"}]}");
    let summary = shape.summary(stopped);
    assert_eq!(summary["valid_json"], true);
    assert_eq!(summary["quoted_anchor_ref_occurrences"], 1);
    assert_eq!(summary["quoted_text_occurrences"], 1);
    assert_eq!(summary["chunks"], 2);
    assert!(!summary.to_string().contains("sentinel"));
    let truncated = shape.summary(ModelProviderCompletion::new(
        ModelFinishReason::OutputLimit,
        stopped.usage(),
    ));
    assert_eq!(truncated["valid_json"], true);
    assert_eq!(
        truncated["finish"], "OutputLimit",
        "valid JSON never replaces the provider finish reason"
    );
    shape.push(&"ö".repeat(MAX_PREFIX_BYTES));
    shape.push("\"anchor_ref\"");
    let overflow = shape.summary(stopped);
    assert_eq!(overflow["capture_complete"], false);
    assert!(overflow["valid_json"].is_null());
    assert!(shape.prefix.len() <= MAX_PREFIX_BYTES);
    assert_eq!(overflow["quoted_anchor_ref_occurrences"], 1);
    Ok(())
}

#[test]
fn research_stream_observer_preserves_events_failures_drop_and_retention()
-> Result<(), Box<dyn std::error::Error>> {
    use a3_application::{
        ModelFinishReason, ModelOutputChunk, ModelProviderFailure, ModelProviderUsage,
    };
    let expected = vec![
        Ok(ProviderEvent::OutputText(
            ModelOutputChunk::try_from_string("{}".to_owned())?,
        )),
        Ok(ProviderEvent::Completed(ModelProviderCompletion::new(
            ModelFinishReason::OutputLimit,
            ModelProviderUsage::new(None, Some(5)),
        ))),
        Err(ModelProviderFailure::Cancelled),
    ];
    futures::executor::block_on(async {
        let records = Arc::new(Mutex::new(Vec::new()));
        let events = observe_stream(Box::pin(futures::stream::iter(expected.clone())), &records)
            .collect::<Vec<_>>()
            .await;
        assert_eq!(events, expected);
        {
            let retained = records.lock().map_err(|_| "records")?;
            assert_eq!(retained.len(), 1);
            assert_eq!(retained[0]["finish"], "OutputLimit");
            assert_eq!(retained[0]["output_tokens"], 5);
        }
        let prefix = observe_stream(Box::pin(futures::stream::iter(expected.clone())), &records)
            .take(1)
            .collect::<Vec<_>>()
            .await;
        assert_eq!(prefix, expected[..1]);
        assert_eq!(
            records.lock().map_err(|_| "records")?.len(),
            1,
            "dropping an unfinished stream cannot invent completion"
        );
        records
            .lock()
            .map_err(|_| "records")?
            .resize(MAX_RECORDS, Value::Null);
        let events = observe_stream(Box::pin(futures::stream::iter(expected.clone())), &records)
            .collect::<Vec<_>>()
            .await;
        assert_eq!(events, expected);
        assert_eq!(records.lock().map_err(|_| "records")?.len(), MAX_RECORDS);
        Ok(())
    })
}
