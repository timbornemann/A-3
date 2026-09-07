//! Content-free request measurements for the explicitly selected live fixture only.
use a3_application::{
    AgentReadAction, ContextToolResult, ContextToolResultDigest, ContextToolResultStatus,
    ModelOperationControl, ModelProvider, ModelProviderFuture, ModelProviderRequest,
    ModelRequestTimeout, ProviderEvent,
};
use a3_domain::{AgentAction, AgentInspectTarget, ModelProviderId, SnapshotId};
use futures::StreamExt;
use std::{sync::Arc, time::Instant};

const READ_WINDOW: usize = 64;

/// Diagnostics only, owned by one executor attempt. Never a durable progress authority.
#[derive(Default)]
pub(super) struct ReadProbe {
    recent: std::collections::VecDeque<(SnapshotId, AgentReadAction, ContextToolResultDigest)>,
    evicted: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ReadKind {
    Search,
    File,
    Symbol,
    Graph,
    Claim,
    Test,
    Flow,
}

/// Deliberately no paths, query text, private IDs, result text or source bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ReadObservation {
    kind: ReadKind,
    repeated_request: bool,
    repeated_result: bool,
    status: ContextToolResultStatus,
    original_source: bool,
    source_bytes: u32,
    window_entries: usize,
    evicted: u32,
}

impl ReadProbe {
    pub(super) fn record(
        &mut self,
        action: &AgentAction,
        result: &ContextToolResult,
    ) -> Option<ReadObservation> {
        let (read, kind) = match action {
            AgentAction::Search(search) => {
                (AgentReadAction::Search(search.clone()), ReadKind::Search)
            }
            AgentAction::Inspect(inspect) => (
                AgentReadAction::Inspect(inspect.clone()),
                match inspect.target() {
                    AgentInspectTarget::File(_) => ReadKind::File,
                    AgentInspectTarget::Symbol(_) => ReadKind::Symbol,
                    AgentInspectTarget::Graph(_) => ReadKind::Graph,
                    AgentInspectTarget::Claim(_) => ReadKind::Claim,
                    AgentInspectTarget::Test(_) => ReadKind::Test,
                    AgentInspectTarget::FunctionFlow(_) => ReadKind::Flow,
                },
            ),
            _ => return None,
        };
        let repeated_request = self
            .recent
            .iter()
            .any(|(snapshot, prior, _)| *snapshot == result.snapshot_before() && *prior == read);
        let repeated_result = self.recent.iter().any(|(snapshot, _, digest)| {
            *snapshot == result.snapshot_before() && *digest == result.digest()
        });
        if self.recent.len() == READ_WINDOW {
            self.recent.pop_front();
            self.evicted = self.evicted.saturating_add(1);
        }
        self.recent
            .push_back((result.snapshot_before(), read, result.digest()));
        Some(ReadObservation {
            kind,
            repeated_request,
            repeated_result,
            status: result.status(),
            original_source: result.original_source().is_some(),
            source_bytes: result
                .original_source()
                .and_then(|source| source.location().range())
                .map_or(0, |range| range.len()),
            window_entries: self.recent.len(),
            evicted: self.evicted,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use a3_application::ContextToolResultPreview;
    use a3_domain::{
        AgentFileInspection, AgentFileLineCount, AgentFileStartLine, AgentInspectAction,
        ModuleCardClaimId, RepositoryPath, RunEventSequence, ToolRunId,
    };
    use std::error::Error;

    fn result(snapshot: u8, digest: u8) -> Result<ContextToolResult, Box<dyn Error>> {
        Ok(ContextToolResult::new(
            RunEventSequence::new(1)?,
            ToolRunId::from_bytes([1; 32]),
            ContextToolResultStatus::Succeeded,
            ContextToolResultPreview::try_from_string(
                "untrusted preview is not original code".to_owned(),
            )?,
            ContextToolResultDigest::from_bytes([digest; 32]),
            false,
            SnapshotId::from_bytes([snapshot; 32]),
            SnapshotId::from_bytes([snapshot; 32]),
        ))
    }

    fn file(start: u32) -> Result<AgentAction, Box<dyn Error>> {
        Ok(AgentAction::Inspect(AgentInspectAction::new(
            AgentInspectTarget::File(AgentFileInspection::new(
                RepositoryPath::try_from_bytes(b"private.py".to_vec())?,
                AgentFileStartLine::new(start)?,
                AgentFileLineCount::new(64)?,
            )),
        )))
    }

    #[test]
    fn live_read_probe_compares_typed_requests_not_debug_redaction_or_previews()
    -> Result<(), Box<dyn Error>> {
        let mut probe = ReadProbe::default();
        let first = probe.record(&file(1)?, &result(1, 1)?).ok_or("read")?;
        assert!(!first.repeated_request);
        assert!(!first.original_source);
        assert_eq!(first.source_bytes, 0);
        assert!(
            probe
                .record(&file(1)?, &result(1, 1)?)
                .ok_or("read")?
                .repeated_request
        );
        let later = probe.record(&file(65)?, &result(1, 1)?).ok_or("read")?;
        assert!(!later.repeated_request);
        assert!(later.repeated_result);
        assert!(
            !probe
                .record(&file(1)?, &result(2, 1)?)
                .ok_or("read")?
                .repeated_request
        );
        for id in [1, 2] {
            let claim = AgentAction::Inspect(AgentInspectAction::new(AgentInspectTarget::Claim(
                ModuleCardClaimId::from_bytes([id; 32]),
            )));
            let observation = probe.record(&claim, &result(1, 1)?).ok_or("read")?;
            assert_eq!(observation.kind, ReadKind::Claim);
            assert!(!observation.repeated_request);
        }
        let text = format!("{first:?}");
        assert!(!text.contains("private.py"));
        assert!(!text.contains("untrusted preview"));
        Ok(())
    }

    #[test]
    fn live_read_probe_has_a_bounded_explicit_attempt_window() -> Result<(), Box<dyn Error>> {
        let mut probe = ReadProbe::default();
        for start in 1..=65 {
            let observation = probe.record(&file(start)?, &result(1, 1)?).ok_or("read")?;
            assert_eq!(
                observation.window_entries,
                usize::try_from(start)?.min(READ_WINDOW)
            );
            assert_eq!(observation.evicted, start.saturating_sub(64));
        }
        assert!(
            !probe
                .record(&file(1)?, &result(1, 1)?)
                .ok_or("read")?
                .repeated_request
        );
        Ok(())
    }
}

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
                Some("A^3 SourceWork V1") => "source_work",
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
