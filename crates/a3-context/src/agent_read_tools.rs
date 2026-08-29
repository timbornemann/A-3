use a3_application::AgentControllerControl;
use a3_application::{
    AgentReadAction, AgentReadResult, AgentReadTimeout, AgentReadToolFailure, AgentReadTools,
    AgentReadToolsFuture, AgentSourceReadControl, AgentSourceReadFailure, AgentSourceReader,
    CompileTaskLens, CompileTaskLensFailure, ContextToolResultDigest, ContextToolResultPreview,
    ContextToolResultStatus, KnowledgeIndexFailure, KnowledgeSearchControl, KnowledgeSearchFailure,
    KnowledgeSearchStore, TaskLensClaimStore, TaskLensClaimStoreFailure, TaskLensControl,
    TaskLensControlError, TaskLensIndexStore, TaskLensTimeout,
};
use a3_domain::{
    AgentInspectTarget, AgentToolEvidence, AgentToolEvidenceSet, EvidenceRef, ExactSearchTarget,
    GraphEndpoint, GraphSymbol, ModuleClaimPolarity, ModuleClaimPredicate, ModuleRoot, Progress,
    ProjectIdentity, PublishedIndex, RepositoryPath, ResolvedModuleCardEvidence, SnapshotId,
    SymbolId, SymbolRole, TaskEvidenceId, TaskLensClaim, TaskLensSeedSet, TaskLensSeedText,
    TaskLensTarget, TaskLensTokenBudget, ToolRunId,
};
use std::collections::BTreeMap;
use std::fmt::Write;
use std::time::{Duration, Instant};

const MAX_AGENT_TOOL_PREVIEW_BYTES: usize = 16 * 1_024;
const MAX_AGENT_TOOL_EVIDENCE: usize = 100;
const MAX_TEST_INSPECTION_RESULTS: usize = 100;

/// H10 implementation of deterministic Search and typed read-only Inspect actions.
#[derive(Debug, Clone, Copy)]
pub struct DeterministicAgentReadTools<'a> {
    index: &'a dyn TaskLensIndexStore,
    search: &'a dyn KnowledgeSearchStore,
    claims: &'a dyn TaskLensClaimStore,
    source: &'a dyn AgentSourceReader,
}

impl<'a> DeterministicAgentReadTools<'a> {
    /// Composes current index, retrieval, verified-claim, and safe source-read capabilities.
    #[must_use]
    pub const fn new(
        index: &'a dyn TaskLensIndexStore,
        search: &'a dyn KnowledgeSearchStore,
        claims: &'a dyn TaskLensClaimStore,
        source: &'a dyn AgentSourceReader,
    ) -> Self {
        Self {
            index,
            search,
            claims,
            source,
        }
    }

    async fn execute_action(
        self,
        project: &ProjectIdentity,
        snapshot_id: SnapshotId,
        tool_run_id: ToolRunId,
        action: &AgentReadAction,
        timeout: AgentReadTimeout,
        control: &dyn AgentControllerControl,
    ) -> Result<AgentReadResult, AgentReadToolFailure> {
        let deadline = AgentReadDeadline::new(control, timeout.duration());
        deadline.check()?;
        let rendered = match action {
            AgentReadAction::Search(action) => {
                self.search(project, snapshot_id, action, &deadline).await?
            }
            AgentReadAction::Inspect(action) => {
                self.inspect(project, snapshot_id, action.target(), &deadline)
                    .await?
            }
        };
        deadline.check()?;
        rendered.finish(tool_run_id, snapshot_id)
    }

    async fn search(
        self,
        project: &ProjectIdentity,
        snapshot_id: SnapshotId,
        action: &a3_domain::AgentSearchAction,
        deadline: &AgentReadDeadline<'_>,
    ) -> Result<RenderedToolResult, AgentReadToolFailure> {
        let query = TaskLensSeedText::try_from_string(action.query().as_str().to_owned())
            .map_err(|_| AgentReadToolFailure::InvalidResult)?;
        let seeds = TaskLensSeedSet::new(query.clone(), query, Vec::new())
            .map_err(|_| AgentReadToolFailure::InvalidResult)?;
        let timeout_millis = u64::try_from(deadline.timeout.as_millis())
            .map_err(|_| AgentReadToolFailure::InvalidResult)?;
        let lens = CompileTaskLens::new(self.index, self.search, self.claims)
            .with_timeout(
                TaskLensTimeout::from_millis(timeout_millis)
                    .map_err(|_| AgentReadToolFailure::InvalidResult)?,
            )
            .execute(project, seeds, TaskLensTokenBudget::DEFAULT, deadline)
            .await
            .map_err(map_lens_failure)?;
        if lens.snapshot_id() != snapshot_id {
            return Err(AgentReadToolFailure::Unavailable);
        }
        let published = self.current_index(project, snapshot_id, deadline).await?;
        if published.run().id() != lens.index_run_id() {
            return Err(AgentReadToolFailure::Unavailable);
        }

        let mut output = String::new();
        writeln!(
            output,
            "SEARCH snapshot={} index_run={} requested_limit={} lens_truncated={}",
            snapshot_id,
            lens.index_run_id(),
            action.limit().get(),
            lens.truncated()
        )
        .map_err(|_| AgentReadToolFailure::InvalidResult)?;
        let mut evidence = EvidenceCollector::default();
        let mut result_count = 0usize;
        let limit = usize::from(action.limit().get());
        let mut result_truncated = lens.truncated();
        for entry in lens.entries() {
            if result_count == limit {
                result_truncated = true;
                break;
            }
            render_lens_target(&mut output, entry.target(), &published, &mut evidence)?;
            result_count += 1;
        }
        for claim in lens.claims() {
            if result_count == limit {
                result_truncated = true;
                break;
            }
            render_claim(&mut output, claim, &mut evidence)?;
            result_count += 1;
        }
        writeln!(
            output,
            "PAGE returned={} next_cursor={}",
            result_count,
            if result_truncated {
                "refine-query"
            } else {
                "none"
            }
        )
        .map_err(|_| AgentReadToolFailure::InvalidResult)?;
        Ok(RenderedToolResult::new(output, evidence, result_truncated))
    }

    async fn inspect(
        self,
        project: &ProjectIdentity,
        snapshot_id: SnapshotId,
        target: &AgentInspectTarget,
        deadline: &AgentReadDeadline<'_>,
    ) -> Result<RenderedToolResult, AgentReadToolFailure> {
        let published = self.current_index(project, snapshot_id, deadline).await?;
        match target {
            AgentInspectTarget::File(request) => {
                let graph = published.publication().graph();
                let position = graph
                    .files()
                    .binary_search_by(|revision| revision.path().cmp(request.path()))
                    .map_err(|_| AgentReadToolFailure::Unavailable)?;
                let revision = &graph.files()[position];
                let page = self
                    .source
                    .read_page(project, revision, request, deadline)
                    .await
                    .map_err(map_source_failure)?;
                if page.revision() != revision || page.start_line() != request.start_line() {
                    return Err(AgentReadToolFailure::InvalidResult);
                }
                let mut output = String::new();
                let mut evidence = EvidenceCollector::default();
                let source = page.evidence();
                let marker = evidence.insert(source);
                writeln!(
                    output,
                    "FILE {} start_line={} truncated={} next_start_line={} {}",
                    path_text(revision.path()),
                    page.start_line().get(),
                    page.truncated(),
                    page.next_start_line()
                        .map_or_else(|| "none".to_owned(), |line| line.get().to_string()),
                    marker
                )
                .map_err(|_| AgentReadToolFailure::InvalidResult)?;
                render_numbered_source(&mut output, page.start_line().get(), page.text())?;
                Ok(RenderedToolResult::new(output, evidence, page.truncated()))
            }
            AgentInspectTarget::Symbol(symbol_id) => {
                let symbol = current_symbol(&published, *symbol_id)?;
                let mut output = String::new();
                let mut evidence = EvidenceCollector::default();
                render_symbol(&mut output, symbol, &mut evidence)?;
                Ok(RenderedToolResult::new(output, evidence, false))
            }
            AgentInspectTarget::Graph(graph) => {
                let result = self
                    .search
                    .traverse_graph(project, graph.query(), deadline)
                    .await
                    .map_err(map_search_failure)?;
                if result.snapshot_id() != snapshot_id
                    || result.index_run_id() != published.run().id()
                {
                    return Err(AgentReadToolFailure::Unavailable);
                }
                let mut output = format!(
                    "GRAPH snapshot={} index_run={} hits={} truncated={}\n",
                    result.snapshot_id(),
                    result.index_run_id(),
                    result.hits().len(),
                    result.truncated()
                );
                let mut evidence = EvidenceCollector::default();
                for hit in result.hits() {
                    render_exact_target(&mut output, hit.target(), &mut evidence)?;
                    for edge in hit.path() {
                        let marker =
                            evidence.insert(AgentToolEvidence::for_span(edge.evidence().clone()));
                        writeln!(
                            output,
                            "  edge kind={:?} source={} target={} {}",
                            edge.kind(),
                            endpoint_text(edge.source()),
                            endpoint_text(edge.target()),
                            marker
                        )
                        .map_err(|_| AgentReadToolFailure::InvalidResult)?;
                    }
                }
                Ok(RenderedToolResult::new(
                    output,
                    evidence,
                    result.truncated(),
                ))
            }
            AgentInspectTarget::Claim(claim_id) => {
                let claim = self
                    .claims
                    .load_claim(project, &published, *claim_id, deadline)
                    .await
                    .map_err(map_claim_failure)?
                    .ok_or(AgentReadToolFailure::Unavailable)?;
                let mut output = String::new();
                let mut evidence = EvidenceCollector::default();
                render_claim(&mut output, &claim, &mut evidence)?;
                Ok(RenderedToolResult::new(output, evidence, false))
            }
            AgentInspectTarget::Test(selector) => {
                let selector = selector.as_str().to_lowercase();
                let mut output = format!("TEST selector_bytes={}\n", selector.len());
                let mut evidence = EvidenceCollector::default();
                let mut found = 0usize;
                let mut truncated = false;
                for symbol in published.publication().graph().symbols() {
                    if !symbol.parsed().roles().contains(SymbolRole::Test)
                        || !symbol
                            .parsed()
                            .name()
                            .as_str()
                            .to_lowercase()
                            .contains(&selector)
                    {
                        continue;
                    }
                    if found == MAX_TEST_INSPECTION_RESULTS {
                        truncated = true;
                        break;
                    }
                    render_symbol(&mut output, symbol, &mut evidence)?;
                    found += 1;
                }
                writeln!(output, "PAGE returned={found} truncated={truncated}")
                    .map_err(|_| AgentReadToolFailure::InvalidResult)?;
                Ok(RenderedToolResult::new(output, evidence, truncated))
            }
        }
    }

    async fn current_index(
        self,
        project: &ProjectIdentity,
        snapshot_id: SnapshotId,
        deadline: &AgentReadDeadline<'_>,
    ) -> Result<std::sync::Arc<PublishedIndex>, AgentReadToolFailure> {
        deadline.check()?;
        let published = self
            .index
            .load_current_index(project, deadline)
            .await
            .map_err(map_index_failure)?
            .ok_or(AgentReadToolFailure::Unavailable)?;
        if published.run().snapshot_id() != snapshot_id {
            return Err(AgentReadToolFailure::Unavailable);
        }
        Ok(published)
    }
}

impl AgentReadTools for DeterministicAgentReadTools<'_> {
    fn execute<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        snapshot_id: SnapshotId,
        tool_run_id: ToolRunId,
        action: &'a AgentReadAction,
        timeout: AgentReadTimeout,
        control: &'a dyn AgentControllerControl,
    ) -> AgentReadToolsFuture<'a> {
        Box::pin(async move {
            self.execute_action(project, snapshot_id, tool_run_id, action, timeout, control)
                .await
        })
    }
}

#[derive(Debug)]
struct AgentReadDeadline<'a> {
    control: &'a dyn AgentControllerControl,
    started: Instant,
    timeout: Duration,
}

impl<'a> AgentReadDeadline<'a> {
    fn new(control: &'a dyn AgentControllerControl, timeout: Duration) -> Self {
        Self {
            control,
            started: Instant::now(),
            timeout,
        }
    }

    fn check(&self) -> Result<(), AgentReadToolFailure> {
        if self.control.is_cancelled() {
            Err(AgentReadToolFailure::Cancelled)
        } else if self.started.elapsed() >= self.timeout {
            Err(AgentReadToolFailure::TimedOut)
        } else {
            Ok(())
        }
    }
}

impl TaskLensControl for AgentReadDeadline<'_> {
    fn is_cancelled(&self) -> bool {
        self.control.is_cancelled() || self.started.elapsed() >= self.timeout
    }

    fn report_progress(&self, _progress: Progress) -> Result<(), TaskLensControlError> {
        Ok(())
    }
}

impl KnowledgeSearchControl for AgentReadDeadline<'_> {
    fn is_cancelled(&self) -> bool {
        self.control.is_cancelled() || self.started.elapsed() >= self.timeout
    }
}

impl AgentSourceReadControl for AgentReadDeadline<'_> {
    fn is_cancelled(&self) -> bool {
        self.control.is_cancelled() || self.started.elapsed() >= self.timeout
    }
}

#[derive(Debug, Default)]
struct EvidenceCollector {
    items: BTreeMap<TaskEvidenceId, AgentToolEvidence>,
    truncated: bool,
}

impl EvidenceCollector {
    fn insert(&mut self, evidence: AgentToolEvidence) -> String {
        let id = evidence.id();
        if self.items.contains_key(&id) {
            return evidence_marker(&evidence);
        }
        if self.items.len() == MAX_AGENT_TOOL_EVIDENCE {
            self.truncated = true;
            return String::from("evidence=omitted:limit");
        }
        let marker = evidence_marker(&evidence);
        self.items.insert(id, evidence);
        marker
    }

    fn into_set(
        self,
        snapshot_id: SnapshotId,
    ) -> Result<AgentToolEvidenceSet, AgentReadToolFailure> {
        AgentToolEvidenceSet::new(snapshot_id, self.items.into_values().collect())
            .map_err(|_| AgentReadToolFailure::InvalidResult)
    }
}

struct RenderedToolResult {
    output: String,
    evidence: EvidenceCollector,
    truncated: bool,
}

impl RenderedToolResult {
    fn new(output: String, evidence: EvidenceCollector, truncated: bool) -> Self {
        Self {
            output,
            evidence,
            truncated,
        }
    }

    fn finish(
        self,
        tool_run_id: ToolRunId,
        snapshot_id: SnapshotId,
    ) -> Result<AgentReadResult, AgentReadToolFailure> {
        let normalized = self.output.replace("\r\n", "\n").replace('\r', "\n");
        let normalized = normalized.trim();
        let observed_output_bytes =
            u64::try_from(normalized.len()).map_err(|_| AgentReadToolFailure::InvalidResult)?;
        let mut hasher = blake3::Hasher::new_derive_key("a3.agent-read-result.v1");
        hasher.update(normalized.as_bytes());
        let digest = ContextToolResultDigest::from_bytes(*hasher.finalize().as_bytes());
        let (preview, preview_truncated) = bounded_preview(normalized);
        let evidence_truncated = self.evidence.truncated;
        let evidence = self.evidence.into_set(snapshot_id)?;
        AgentReadResult::new(
            tool_run_id,
            ContextToolResultStatus::Succeeded,
            ContextToolResultPreview::try_from_string(preview)
                .map_err(|_| AgentReadToolFailure::InvalidResult)?,
            digest,
            self.truncated || preview_truncated || evidence_truncated,
            snapshot_id,
            evidence,
            observed_output_bytes,
        )
        .map_err(|_| AgentReadToolFailure::InvalidResult)
    }
}

fn bounded_preview(output: &str) -> (String, bool) {
    if output.len() <= MAX_AGENT_TOOL_PREVIEW_BYTES {
        return (output.to_owned(), false);
    }
    let mut boundary = MAX_AGENT_TOOL_PREVIEW_BYTES;
    while !output.is_char_boundary(boundary) {
        boundary -= 1;
    }
    (output[..boundary].to_owned(), true)
}

fn render_lens_target(
    output: &mut String,
    target: &TaskLensTarget,
    published: &PublishedIndex,
    evidence: &mut EvidenceCollector,
) -> Result<(), AgentReadToolFailure> {
    match target {
        TaskLensTarget::Repository(card) => writeln!(
            output,
            "RESULT repository files={} symbols={} entrypoints={}",
            card.file_count(),
            card.symbol_count(),
            card.entrypoints().symbols().len()
        )
        .map_err(|_| AgentReadToolFailure::InvalidResult),
        TaskLensTarget::Module(module) => {
            let root = match module.root() {
                Some(ModuleRoot::Repository) => String::from("."),
                Some(ModuleRoot::Directory(path)) => path_text(path),
                None => String::from("graph-community"),
            };
            writeln!(
                output,
                "RESULT module id={} kind={:?} root={}",
                module.id(),
                module.kind(),
                root
            )
            .map_err(|_| AgentReadToolFailure::InvalidResult)?;
            for manifest in module.manifests() {
                let marker = evidence.insert(AgentToolEvidence::for_file(manifest.clone()));
                writeln!(
                    output,
                    "  manifest={} {}",
                    path_text(manifest.path()),
                    marker
                )
                .map_err(|_| AgentReadToolFailure::InvalidResult)?;
            }
            for id in module.central_symbols().symbols() {
                if let Ok(symbol) = current_symbol(published, *id) {
                    render_symbol(output, symbol, evidence)?;
                }
            }
            Ok(())
        }
        TaskLensTarget::File(revision) => {
            let marker = evidence.insert(AgentToolEvidence::for_file(revision.clone()));
            writeln!(
                output,
                "RESULT file path={} {}",
                path_text(revision.path()),
                marker
            )
            .map_err(|_| AgentReadToolFailure::InvalidResult)
        }
        TaskLensTarget::Symbol(symbol) => render_symbol(output, symbol, evidence),
        TaskLensTarget::SourceSpan {
            symbol_id,
            evidence: source,
        } => {
            let marker = evidence.insert(AgentToolEvidence::for_span(source.clone()));
            writeln!(output, "RESULT span symbol={} {}", symbol_id, marker)
                .map_err(|_| AgentReadToolFailure::InvalidResult)
        }
    }
}

fn render_symbol(
    output: &mut String,
    symbol: &GraphSymbol,
    evidence: &mut EvidenceCollector,
) -> Result<(), AgentReadToolFailure> {
    let source = EvidenceRef::new(
        symbol.revision().clone(),
        symbol.parsed().declaration_range(),
    );
    let marker = evidence.insert(AgentToolEvidence::for_span(source));
    writeln!(
        output,
        "RESULT symbol id={} kind={:?} name={} signature={} {}",
        symbol.id(),
        symbol.parsed().kind(),
        symbol.parsed().name().as_str(),
        symbol
            .parsed()
            .signature()
            .map_or("none", |signature| signature.as_str()),
        marker
    )
    .map_err(|_| AgentReadToolFailure::InvalidResult)
}

fn render_exact_target(
    output: &mut String,
    target: &ExactSearchTarget,
    evidence: &mut EvidenceCollector,
) -> Result<(), AgentReadToolFailure> {
    match target {
        ExactSearchTarget::File(revision) => {
            let marker = evidence.insert(AgentToolEvidence::for_file(revision.clone()));
            writeln!(
                output,
                "RESULT file path={} {}",
                path_text(revision.path()),
                marker
            )
            .map_err(|_| AgentReadToolFailure::InvalidResult)
        }
        ExactSearchTarget::Symbol(symbol) => render_symbol(output, symbol.symbol(), evidence),
    }
}

fn render_claim(
    output: &mut String,
    claim: &TaskLensClaim,
    evidence: &mut EvidenceCollector,
) -> Result<(), AgentReadToolFailure> {
    writeln!(
        output,
        "RESULT claim id={:?} module={} kind={:?} polarity={} confidence={} predicate={}",
        claim.id(),
        claim.module_id(),
        claim.kind(),
        match claim.polarity() {
            ModuleClaimPolarity::Affirms => "affirms",
            ModuleClaimPolarity::Denies => "denies",
        },
        claim.confidence().basis_points(),
        predicate_text(claim.predicate())
    )
    .map_err(|_| AgentReadToolFailure::InvalidResult)?;
    for source in claim.evidence() {
        let source = match source {
            ResolvedModuleCardEvidence::File { revision, .. } => {
                AgentToolEvidence::for_file(revision.clone())
            }
            ResolvedModuleCardEvidence::Symbol { symbol, .. } => {
                AgentToolEvidence::for_span(EvidenceRef::new(
                    symbol.revision().clone(),
                    symbol.parsed().declaration_range(),
                ))
            }
            ResolvedModuleCardEvidence::GraphEdge { edge, .. } => {
                AgentToolEvidence::for_span(edge.evidence().clone())
            }
        };
        let marker = evidence.insert(source);
        writeln!(output, "  {marker}").map_err(|_| AgentReadToolFailure::InvalidResult)?;
    }
    Ok(())
}

fn predicate_text(predicate: &ModuleClaimPredicate) -> String {
    match predicate {
        ModuleClaimPredicate::Path(path) => format!("path:{}", path_text(path)),
        ModuleClaimPredicate::Symbol(id) => format!("symbol:{id}"),
        ModuleClaimPredicate::Relation {
            source,
            target,
            kind,
        } => format!(
            "relation:{kind:?}:{}:{}",
            endpoint_text(source),
            endpoint_text(target)
        ),
        ModuleClaimPredicate::Observed(statement) => {
            format!("observation:{}", statement.as_str())
        }
        ModuleClaimPredicate::ArchitecturalIntent(statement) => {
            format!("hypothesis:{}", statement.as_str())
        }
    }
}

fn render_numbered_source(
    output: &mut String,
    start_line: u32,
    source: &str,
) -> Result<(), AgentReadToolFailure> {
    let mut line = start_line;
    for value in source.split_inclusive('\n') {
        write!(output, "{line:>6}| {value}").map_err(|_| AgentReadToolFailure::InvalidResult)?;
        line = line
            .checked_add(1)
            .ok_or(AgentReadToolFailure::InvalidResult)?;
    }
    if source.is_empty() {
        writeln!(output, "{start_line:>6}| <EOF>")
            .map_err(|_| AgentReadToolFailure::InvalidResult)?;
    } else if !source.ends_with('\n') {
        output.push('\n');
    }
    Ok(())
}

fn current_symbol(
    published: &PublishedIndex,
    symbol_id: SymbolId,
) -> Result<&GraphSymbol, AgentReadToolFailure> {
    let symbols = published.publication().graph().symbols();
    let position = symbols
        .binary_search_by_key(&symbol_id, |symbol| symbol.id())
        .map_err(|_| AgentReadToolFailure::Unavailable)?;
    Ok(&symbols[position])
}

fn evidence_marker(evidence: &AgentToolEvidence) -> String {
    let location = evidence.location();
    let line = location
        .range()
        .map_or(1, |range| range.start_position().row().saturating_add(1));
    format!(
        "evidence={} source={}:{}",
        evidence.id(),
        path_text(location.revision().path()),
        line
    )
}

fn endpoint_text(endpoint: &GraphEndpoint) -> String {
    match endpoint {
        GraphEndpoint::File(path) => format!("file:{}", path_text(path)),
        GraphEndpoint::Symbol(id) => format!("symbol:{id}"),
    }
}

fn path_text(path: &RepositoryPath) -> String {
    let mut encoded = String::with_capacity(path.as_bytes().len());
    for byte in path.as_bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'.' | b'-' | b'_') {
            encoded.push(char::from(*byte));
        } else {
            encoded.push('%');
            encoded.push(hex_digit(byte >> 4));
            encoded.push(hex_digit(byte & 0x0f));
        }
    }
    encoded
}

const fn hex_digit(value: u8) -> char {
    match value {
        0..=9 => (b'0' + value) as char,
        _ => (b'A' + value - 10) as char,
    }
}

fn map_lens_failure(failure: CompileTaskLensFailure) -> AgentReadToolFailure {
    match failure {
        CompileTaskLensFailure::Cancelled => AgentReadToolFailure::Cancelled,
        CompileTaskLensFailure::TimedOut => AgentReadToolFailure::TimedOut,
        CompileTaskLensFailure::IndexUnavailable => AgentReadToolFailure::Unavailable,
        CompileTaskLensFailure::Index(source) => map_index_failure(source),
        CompileTaskLensFailure::Search(source) => map_search_failure(source),
        CompileTaskLensFailure::Claims(source) => map_claim_failure(source),
        CompileTaskLensFailure::Semantic(_)
        | CompileTaskLensFailure::InvalidSeedQuery
        | CompileTaskLensFailure::InvalidChannelProjection
        | CompileTaskLensFailure::CandidateSet(_)
        | CompileTaskLensFailure::CandidateSets(_)
        | CompileTaskLensFailure::Fusion(_)
        | CompileTaskLensFailure::Compile(_)
        | CompileTaskLensFailure::ProgressUnavailable
        | CompileTaskLensFailure::ResourceLimit => AgentReadToolFailure::InvalidResult,
    }
}

fn map_index_failure(failure: KnowledgeIndexFailure) -> AgentReadToolFailure {
    match failure {
        KnowledgeIndexFailure::Cancelled => AgentReadToolFailure::Cancelled,
        KnowledgeIndexFailure::TimedOut => AgentReadToolFailure::TimedOut,
        KnowledgeIndexFailure::Storage(_)
        | KnowledgeIndexFailure::SnapshotNotFound
        | KnowledgeIndexFailure::IndexRunNotFound => AgentReadToolFailure::Unavailable,
        KnowledgeIndexFailure::SnapshotConflict
        | KnowledgeIndexFailure::IndexRunAlreadyActive
        | KnowledgeIndexFailure::IndexRunSequenceConflict
        | KnowledgeIndexFailure::IndexRunSequenceExhausted
        | KnowledgeIndexFailure::InvalidIndexRunTransition
        | KnowledgeIndexFailure::IndexPublicationMismatch
        | KnowledgeIndexFailure::IndexPublicationTooLarge
        | KnowledgeIndexFailure::ProgressUnavailable => AgentReadToolFailure::InvalidResult,
    }
}

fn map_search_failure(failure: KnowledgeSearchFailure) -> AgentReadToolFailure {
    match failure {
        KnowledgeSearchFailure::Cancelled => AgentReadToolFailure::Cancelled,
        KnowledgeSearchFailure::TimedOut => AgentReadToolFailure::TimedOut,
        KnowledgeSearchFailure::IndexUnavailable
        | KnowledgeSearchFailure::SeedUnavailable
        | KnowledgeSearchFailure::ProjectionUnavailable(_)
        | KnowledgeSearchFailure::Storage(_) => AgentReadToolFailure::Unavailable,
        KnowledgeSearchFailure::InvalidCursor | KnowledgeSearchFailure::InvalidStoredProjection => {
            AgentReadToolFailure::InvalidResult
        }
    }
}

fn map_claim_failure(failure: TaskLensClaimStoreFailure) -> AgentReadToolFailure {
    match failure {
        TaskLensClaimStoreFailure::Cancelled => AgentReadToolFailure::Cancelled,
        TaskLensClaimStoreFailure::TimedOut => AgentReadToolFailure::TimedOut,
        TaskLensClaimStoreFailure::Storage(_) => AgentReadToolFailure::Unavailable,
        TaskLensClaimStoreFailure::InvalidStoredProjection => AgentReadToolFailure::InvalidResult,
    }
}

fn map_source_failure(failure: AgentSourceReadFailure) -> AgentReadToolFailure {
    match failure {
        AgentSourceReadFailure::Cancelled => AgentReadToolFailure::Cancelled,
        AgentSourceReadFailure::Denied | AgentSourceReadFailure::SecretCandidate => {
            AgentReadToolFailure::Denied
        }
        AgentSourceReadFailure::Unavailable | AgentSourceReadFailure::Stale => {
            AgentReadToolFailure::Unavailable
        }
        AgentSourceReadFailure::FileTooLarge
        | AgentSourceReadFailure::InvalidEncoding
        | AgentSourceReadFailure::BinaryContent
        | AgentSourceReadFailure::LineTooLong
        | AgentSourceReadFailure::InvalidPage => AgentReadToolFailure::InvalidResult,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use a3_application::{
        AgentReadTools, AgentSourcePage, AgentSourceReaderFuture, KnowledgeSearchFuture,
        TaskLensClaimLimit, TaskLensClaimReadFuture, TaskLensClaimResult, TaskLensClaimStoreFuture,
        TaskLensIndexStoreFuture,
    };
    use a3_domain::{
        AgentFileInspection, AgentFileLineCount, AgentFileStartLine, AgentInspectAction,
        AgentSearchLimit, AgentSearchQuery, CanonicalDirectory, ContentHash, ExactSearchCursor,
        ExactSearchPage, ExactSearchPageSize, ExactSearchQuery, GitHead, GitReferenceName,
        GraphTraversalResult, IndexPublication, IndexRunId, IndexRunRecord, IndexRunSequence,
        IndexRunStatus, LexicalSearchCursor, LexicalSearchPage, LexicalSearchPageSize,
        LexicalSearchQuery, LinkedGraph, ModulePolicyVersion, ModuleProjection, ModuleSymbolSet,
        RankProjection, RankingPolicyVersion, RepositoryCard, RepositoryId, RepositoryIdentity,
        SourcePosition, SourceRange, WorktreeAnchorId, WorktreeId, WorktreeIdentity,
    };
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Debug)]
    struct Active;

    impl AgentControllerControl for Active {
        fn is_cancelled(&self) -> bool {
            false
        }
    }

    #[derive(Debug)]
    struct MemoryIndex {
        published: Arc<PublishedIndex>,
    }

    impl TaskLensIndexStore for MemoryIndex {
        fn load_current_index<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
            _control: &'a dyn TaskLensControl,
        ) -> TaskLensIndexStoreFuture<'a> {
            let published = Arc::clone(&self.published);
            Box::pin(async move { Ok(Some(published)) })
        }
    }

    #[derive(Debug)]
    struct EmptySearch {
        index_run_id: IndexRunId,
        snapshot_id: SnapshotId,
        exact_calls: AtomicUsize,
        lexical_calls: AtomicUsize,
    }

    impl KnowledgeSearchStore for EmptySearch {
        fn search_exact<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
            _query: &'a ExactSearchQuery,
            page_size: ExactSearchPageSize,
            _cursor: Option<&'a ExactSearchCursor>,
            _control: &'a dyn KnowledgeSearchControl,
        ) -> KnowledgeSearchFuture<'a, ExactSearchPage> {
            self.exact_calls.fetch_add(1, Ordering::SeqCst);
            let result = ExactSearchPage::new(
                self.index_run_id,
                self.snapshot_id,
                Vec::new(),
                None,
                page_size,
            )
            .map_err(|_| KnowledgeSearchFailure::InvalidStoredProjection);
            Box::pin(async move { result })
        }

        fn search_lexical<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
            _query: &'a LexicalSearchQuery,
            page_size: LexicalSearchPageSize,
            _cursor: Option<&'a LexicalSearchCursor>,
            _control: &'a dyn KnowledgeSearchControl,
        ) -> KnowledgeSearchFuture<'a, LexicalSearchPage> {
            self.lexical_calls.fetch_add(1, Ordering::SeqCst);
            let result = LexicalSearchPage::new(
                self.index_run_id,
                self.snapshot_id,
                Vec::new(),
                None,
                page_size,
            )
            .map_err(|_| KnowledgeSearchFailure::InvalidStoredProjection);
            Box::pin(async move { result })
        }

        fn traverse_graph<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
            query: &'a a3_domain::TraversalQuery,
            _control: &'a dyn KnowledgeSearchControl,
        ) -> KnowledgeSearchFuture<'a, GraphTraversalResult> {
            let result = GraphTraversalResult::new(
                self.index_run_id,
                self.snapshot_id,
                query.clone(),
                Vec::new(),
                false,
            )
            .map_err(|_| KnowledgeSearchFailure::InvalidStoredProjection);
            Box::pin(async move { result })
        }
    }

    #[derive(Debug)]
    struct EmptyClaims;

    impl TaskLensClaimStore for EmptyClaims {
        fn load_claims<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
            _published: &'a PublishedIndex,
            _limit: TaskLensClaimLimit,
            _control: &'a dyn TaskLensControl,
        ) -> TaskLensClaimStoreFuture<'a> {
            let result = TaskLensClaimResult::new(Vec::new(), false)
                .map_err(|_| TaskLensClaimStoreFailure::InvalidStoredProjection);
            Box::pin(async move { result })
        }

        fn load_claim<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
            _published: &'a PublishedIndex,
            _claim_id: a3_domain::ModuleCardClaimId,
            _control: &'a dyn TaskLensControl,
        ) -> TaskLensClaimReadFuture<'a> {
            Box::pin(async { Ok(None) })
        }
    }

    #[derive(Debug)]
    struct ExactMissClaims {
        page_calls: AtomicUsize,
        exact_calls: AtomicUsize,
    }

    impl TaskLensClaimStore for ExactMissClaims {
        fn load_claims<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
            _published: &'a PublishedIndex,
            _limit: TaskLensClaimLimit,
            _control: &'a dyn TaskLensControl,
        ) -> TaskLensClaimStoreFuture<'a> {
            self.page_calls.fetch_add(1, Ordering::SeqCst);
            let result = TaskLensClaimResult::new(Vec::new(), false)
                .map_err(|_| TaskLensClaimStoreFailure::InvalidStoredProjection);
            Box::pin(async move { result })
        }

        fn load_claim<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
            _published: &'a PublishedIndex,
            _claim_id: a3_domain::ModuleCardClaimId,
            _control: &'a dyn TaskLensControl,
        ) -> TaskLensClaimReadFuture<'a> {
            self.exact_calls.fetch_add(1, Ordering::SeqCst);
            Box::pin(async { Ok(None) })
        }
    }

    #[derive(Debug)]
    struct FixedSource {
        calls: AtomicUsize,
    }

    impl AgentSourceReader for FixedSource {
        fn read_page<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
            expected_revision: &'a a3_domain::FileRevision,
            request: &'a AgentFileInspection,
            _control: &'a dyn AgentSourceReadControl,
        ) -> AgentSourceReaderFuture<'a> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let revision = expected_revision.clone();
            let start_line = request.start_line();
            Box::pin(async move {
                AgentSourcePage::new(
                    revision,
                    SourceRange::new(0, 6, SourcePosition::new(0, 0), SourcePosition::new(1, 0))
                        .map_err(|_| AgentSourceReadFailure::InvalidPage)?,
                    start_line,
                    "alpha\n".to_owned(),
                    Some(
                        AgentFileStartLine::new(2)
                            .map_err(|_| AgentSourceReadFailure::InvalidPage)?,
                    ),
                    true,
                )
                .map_err(|_| AgentSourceReadFailure::InvalidPage)
            })
        }
    }

    #[test]
    fn search_uses_task_lens_and_returns_bounded_page_metadata()
    -> Result<(), Box<dyn std::error::Error>> {
        let snapshot_id = SnapshotId::from_bytes([1; 32]);
        let published = Arc::new(empty_index(snapshot_id)?);
        let index = MemoryIndex { published };
        let search = EmptySearch {
            index_run_id: IndexRunId::from_bytes([2; 32]),
            snapshot_id,
            exact_calls: AtomicUsize::new(0),
            lexical_calls: AtomicUsize::new(0),
        };
        let source = FixedSource {
            calls: AtomicUsize::new(0),
        };
        let tools = DeterministicAgentReadTools::new(&index, &search, &EmptyClaims, &source);
        let action = AgentReadAction::Search(a3_domain::AgentSearchAction::new(
            AgentSearchQuery::try_from_string("architecture".to_owned())?,
            AgentSearchLimit::new(5)?,
        ));
        let result = futures::executor::block_on(tools.execute(
            &project()?,
            snapshot_id,
            ToolRunId::from_bytes([3; 32]),
            &action,
            AgentReadTimeout::DEFAULT,
            &Active,
        ))?;

        assert!(result.preview().as_str().contains("SEARCH snapshot="));
        assert!(result.preview().as_str().contains("RESULT repository"));
        assert!(result.preview().as_str().contains("PAGE returned="));
        assert!(search.exact_calls.load(Ordering::SeqCst) > 0);
        assert!(search.lexical_calls.load(Ordering::SeqCst) > 0);
        assert!(result.evidence().is_empty());
        assert_eq!(source.calls.load(Ordering::SeqCst), 0);
        Ok(())
    }

    #[test]
    fn file_inspection_returns_clickable_evidence_and_forward_page()
    -> Result<(), Box<dyn std::error::Error>> {
        let snapshot_id = SnapshotId::from_bytes([4; 32]);
        let path = RepositoryPath::try_from_bytes(b"src/lib.rs".to_vec())?;
        let revision = a3_domain::FileRevision::new(
            path.clone(),
            ContentHash::from_bytes(*blake3::hash(b"alpha\n").as_bytes()),
        );
        let published = Arc::new(index_with_file(snapshot_id, revision)?);
        let index = MemoryIndex { published };
        let search = EmptySearch {
            index_run_id: IndexRunId::from_bytes([2; 32]),
            snapshot_id,
            exact_calls: AtomicUsize::new(0),
            lexical_calls: AtomicUsize::new(0),
        };
        let source = FixedSource {
            calls: AtomicUsize::new(0),
        };
        let tools = DeterministicAgentReadTools::new(&index, &search, &EmptyClaims, &source);
        let action = AgentReadAction::Inspect(AgentInspectAction::new(AgentInspectTarget::File(
            AgentFileInspection::new(
                path,
                AgentFileStartLine::new(1)?,
                AgentFileLineCount::new(1)?,
            ),
        )));
        let result = futures::executor::block_on(tools.execute(
            &project()?,
            snapshot_id,
            ToolRunId::from_bytes([5; 32]),
            &action,
            AgentReadTimeout::DEFAULT,
            &Active,
        ))?;

        assert!(result.preview().as_str().contains("source=src/lib.rs:1"));
        assert!(result.preview().as_str().contains("next_start_line=2"));
        assert!(result.preview().as_str().contains("     1| alpha"));
        assert_eq!(result.evidence().evidence().len(), 1);
        assert!(result.truncated());
        assert_eq!(source.calls.load(Ordering::SeqCst), 1);
        Ok(())
    }

    #[test]
    fn stale_snapshot_is_rejected_before_workspace_read() -> Result<(), Box<dyn std::error::Error>>
    {
        let published_snapshot = SnapshotId::from_bytes([6; 32]);
        let path = RepositoryPath::try_from_bytes(b"src/lib.rs".to_vec())?;
        let revision = a3_domain::FileRevision::new(path.clone(), ContentHash::from_bytes([7; 32]));
        let index = MemoryIndex {
            published: Arc::new(index_with_file(published_snapshot, revision)?),
        };
        let search = EmptySearch {
            index_run_id: IndexRunId::from_bytes([2; 32]),
            snapshot_id: published_snapshot,
            exact_calls: AtomicUsize::new(0),
            lexical_calls: AtomicUsize::new(0),
        };
        let source = FixedSource {
            calls: AtomicUsize::new(0),
        };
        let tools = DeterministicAgentReadTools::new(&index, &search, &EmptyClaims, &source);
        let action = AgentReadAction::Inspect(AgentInspectAction::new(AgentInspectTarget::File(
            AgentFileInspection::new(
                path,
                AgentFileStartLine::new(1)?,
                AgentFileLineCount::new(1)?,
            ),
        )));

        assert_eq!(
            futures::executor::block_on(tools.execute(
                &project()?,
                SnapshotId::from_bytes([8; 32]),
                ToolRunId::from_bytes([9; 32]),
                &action,
                AgentReadTimeout::DEFAULT,
                &Active,
            )),
            Err(AgentReadToolFailure::Unavailable)
        );
        assert_eq!(source.calls.load(Ordering::SeqCst), 0);
        Ok(())
    }

    #[test]
    fn claim_inspection_uses_exact_lookup_instead_of_a_leading_page()
    -> Result<(), Box<dyn std::error::Error>> {
        let snapshot_id = SnapshotId::from_bytes([13; 32]);
        let index = MemoryIndex {
            published: Arc::new(empty_index(snapshot_id)?),
        };
        let search = EmptySearch {
            index_run_id: IndexRunId::from_bytes([2; 32]),
            snapshot_id,
            exact_calls: AtomicUsize::new(0),
            lexical_calls: AtomicUsize::new(0),
        };
        let claims = ExactMissClaims {
            page_calls: AtomicUsize::new(0),
            exact_calls: AtomicUsize::new(0),
        };
        let source = FixedSource {
            calls: AtomicUsize::new(0),
        };
        let tools = DeterministicAgentReadTools::new(&index, &search, &claims, &source);
        let action = AgentReadAction::Inspect(AgentInspectAction::new(AgentInspectTarget::Claim(
            a3_domain::ModuleCardClaimId::from_bytes([14; 32]),
        )));

        assert_eq!(
            futures::executor::block_on(tools.execute(
                &project()?,
                snapshot_id,
                ToolRunId::from_bytes([15; 32]),
                &action,
                AgentReadTimeout::DEFAULT,
                &Active,
            )),
            Err(AgentReadToolFailure::Unavailable)
        );
        assert_eq!(claims.exact_calls.load(Ordering::SeqCst), 1);
        assert_eq!(claims.page_calls.load(Ordering::SeqCst), 0);
        Ok(())
    }

    #[test]
    fn oversized_normalized_result_keeps_only_a_utf8_safe_preview()
    -> Result<(), Box<dyn std::error::Error>> {
        let output = format!("{}é-tail", "x".repeat(MAX_AGENT_TOOL_PREVIEW_BYTES));
        let observed = output.len();
        let rendered = RenderedToolResult::new(output, EvidenceCollector::default(), false);

        let result = rendered.finish(
            ToolRunId::from_bytes([16; 32]),
            SnapshotId::from_bytes([17; 32]),
        )?;

        assert!(result.truncated());
        assert!(result.preview().as_str().len() <= MAX_AGENT_TOOL_PREVIEW_BYTES);
        assert_eq!(result.observed_output_bytes(), u64::try_from(observed)?);
        assert!(!result.preview().as_str().contains("tail"));
        Ok(())
    }

    fn empty_index(snapshot_id: SnapshotId) -> Result<PublishedIndex, Box<dyn std::error::Error>> {
        make_index(snapshot_id, Vec::new())
    }

    fn index_with_file(
        snapshot_id: SnapshotId,
        revision: a3_domain::FileRevision,
    ) -> Result<PublishedIndex, Box<dyn std::error::Error>> {
        make_index(snapshot_id, vec![revision])
    }

    fn make_index(
        snapshot_id: SnapshotId,
        files: Vec<a3_domain::FileRevision>,
    ) -> Result<PublishedIndex, Box<dyn std::error::Error>> {
        let file_count = u32::try_from(files.len())?;
        let graph = LinkedGraph::new(snapshot_id, files, Vec::new(), Vec::new(), Vec::new())?;
        let ranking = RankProjection::new(snapshot_id, RankingPolicyVersion::v1(), Vec::new())?;
        let policy = ModulePolicyVersion::v1();
        let card = RepositoryCard::new(
            snapshot_id,
            policy,
            Vec::new(),
            Vec::new(),
            ModuleSymbolSet::empty(),
            file_count,
            0,
        )?;
        let modules = ModuleProjection::new(snapshot_id, policy, Vec::new(), Vec::new(), card)?;
        let publication = IndexPublication::new(graph, ranking, Vec::new(), modules)?;
        let run = IndexRunRecord::new(
            IndexRunId::from_bytes([2; 32]),
            snapshot_id,
            RankingPolicyVersion::v1(),
            IndexRunSequence::new(1)?,
            IndexRunStatus::Published,
        );
        Ok(PublishedIndex::new(run, publication)?)
    }

    fn project() -> Result<ProjectIdentity, Box<dyn std::error::Error>> {
        let root = CanonicalDirectory::from_canonicalized(std::fs::canonicalize(".")?)?;
        let repository_id = RepositoryId::from_bytes([10; 32]);
        ProjectIdentity::new(
            RepositoryIdentity::new(repository_id, root.clone(), None),
            WorktreeIdentity::new(
                WorktreeId::from_bytes([11; 32]),
                WorktreeAnchorId::from_bytes([12; 32]),
                repository_id,
                root,
            ),
            GitHead::Unborn {
                reference: GitReferenceName::try_from_full_name("refs/heads/main")?,
            },
        )
        .map_err(Into::into)
    }
}
