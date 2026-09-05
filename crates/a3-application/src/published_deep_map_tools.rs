use crate::{
    DeepMapReadControl, DeepMapReadFailure, DeepMapReadFuture, DeepMapReadTimeout,
    DeepMapReadTools, ExplorerObservation, IndexPersistenceControl, IndexPersistenceControlError,
    KnowledgeIndexFailure, KnowledgeIndexStore,
};
use a3_domain::{
    ExploreTarget, ExplorerSearchAction, ExplorerSearchKind, ExplorerSearchQuery, GraphEdge,
    GraphEndpoint, GraphSymbol, ModuleCardEvidenceId, ModuleRoot, Progress, ProjectIdentity,
    PublishedIndex, RepositoryPath, SnapshotId, SymbolId, SyntaxRelationKind,
};
use std::fmt;
use std::sync::Arc;
use std::time::{Duration, Instant};

const MAX_PREVIEW_BYTES: usize = 16_384;
const MAX_EVIDENCE_IDS: usize = 100;
const MAX_PATH_DISPLAY_CHARS: usize = 512;

/// Read-only Deep-Map tools reconstructed solely from the latest published deterministic index.
pub struct PublishedIndexDeepMapReadTools {
    store: Arc<dyn KnowledgeIndexStore>,
}

impl PublishedIndexDeepMapReadTools {
    /// Narrows the existing index store to the two explorer read capabilities.
    #[must_use]
    pub fn new(store: Arc<dyn KnowledgeIndexStore>) -> Self {
        Self { store }
    }

    async fn load(
        &self,
        project: &ProjectIdentity,
        snapshot_id: SnapshotId,
        timeout: DeepMapReadTimeout,
        control: &dyn DeepMapReadControl,
    ) -> Result<PublishedIndex, DeepMapReadFailure> {
        let operation = PublishedReadControl::new(control, timeout.duration());
        operation.ensure_active()?;
        let published = self
            .store
            .latest_published_index(project, &operation)
            .await
            .map_err(|failure| operation.classify(failure))?
            .ok_or(DeepMapReadFailure::SnapshotUnavailable)?;
        operation.ensure_active()?;
        if published.run().snapshot_id() != snapshot_id {
            return Err(DeepMapReadFailure::SnapshotUnavailable);
        }
        Ok(published)
    }
}

impl fmt::Debug for PublishedIndexDeepMapReadTools {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PublishedIndexDeepMapReadTools")
            .finish_non_exhaustive()
    }
}

impl DeepMapReadTools for PublishedIndexDeepMapReadTools {
    fn inspect<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        snapshot_id: SnapshotId,
        target: &'a ExploreTarget,
        timeout: DeepMapReadTimeout,
        control: &'a dyn DeepMapReadControl,
    ) -> DeepMapReadFuture<'a> {
        Box::pin(async move {
            let operation = PublishedReadControl::new(control, timeout.duration());
            let published = self.load(project, snapshot_id, timeout, control).await?;
            let observation = inspect_target(&published, target)?;
            let ExploreTarget::Symbol(root) = target else {
                return Ok(observation);
            };
            let request = a3_domain::FunctionFlowReadRequest::new(
                *root,
                Vec::new(),
                a3_domain::FunctionFlowReadView::Steps(0),
            )
            .map_err(|_| DeepMapReadFailure::Rejected)?;
            let document = crate::ExploreFunctionFlows::new(Arc::clone(&self.store))
                .read_document(project, published.run().id(), &request, &operation)
                .await
                .map_err(|e| match e {
                    crate::FunctionFlowReadFailure::Storage(failure) => operation.classify(failure),
                    _ => DeepMapReadFailure::InvalidResponse,
                })?;
            let Some(document) = document else {
                return Ok(observation);
            };
            let mut preview = BoundedPreview::new();
            preview.push(observation.preview().to_owned());
            let mut evidence = observation.evidence_ids().to_vec();
            for (ordinal, source) in document.evidence.iter().enumerate() {
                let id = ModuleCardEvidenceId::for_file_revision_v1(source.revision());
                if !evidence.contains(&id) {
                    evidence.push(id);
                }
                preview.push(format!(
                    "flow_source={ordinal} evidence_id={}",
                    hex(id.as_bytes())
                ));
            }
            preview.push(document.text);
            if observation.truncated() || document.truncated {
                preview.mark_truncated();
            }
            operation.ensure_active()?;
            found(preview, evidence)
        })
    }

    fn search<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        snapshot_id: SnapshotId,
        action: &'a ExplorerSearchAction,
        timeout: DeepMapReadTimeout,
        control: &'a dyn DeepMapReadControl,
    ) -> DeepMapReadFuture<'a> {
        Box::pin(async move {
            let published = self.load(project, snapshot_id, timeout, control).await?;
            search_index(&published, action)
        })
    }
}

fn inspect_target(
    published: &PublishedIndex,
    target: &ExploreTarget,
) -> Result<ExplorerObservation, DeepMapReadFailure> {
    let mut preview = BoundedPreview::new();
    let mut evidence = Vec::new();
    match target {
        ExploreTarget::Module(module_id) => {
            let modules = published.publication().modules();
            let module = modules
                .modules()
                .iter()
                .find(|module| module.id() == *module_id)
                .ok_or(DeepMapReadFailure::TargetUnavailable)?;
            preview.push(format!(
                "module id={} kind={} root={}",
                module.id(),
                module_kind(module.kind()),
                module_root(module.root())
            ));
            for manifest in module.manifests() {
                push_revision("manifest", manifest, &mut preview, &mut evidence);
            }
            for membership in modules
                .memberships()
                .iter()
                .filter(|membership| membership.module_id() == *module_id)
            {
                if evidence.len() >= MAX_EVIDENCE_IDS {
                    preview.mark_truncated();
                    break;
                }
                let symbol = published
                    .publication()
                    .graph()
                    .symbols()
                    .iter()
                    .find(|symbol| symbol.id() == membership.symbol_id())
                    .ok_or(DeepMapReadFailure::InvalidResponse)?;
                push_symbol("member", symbol, &mut preview, &mut evidence);
            }
        }
        ExploreTarget::Manifest { path, content_hash } => {
            let revision = published
                .publication()
                .manifest_files()
                .iter()
                .find(|revision| {
                    revision.path() == path && revision.content_hash() == *content_hash
                })
                .ok_or(DeepMapReadFailure::TargetUnavailable)?;
            push_revision("manifest", revision, &mut preview, &mut evidence);
        }
        ExploreTarget::Symbol(symbol_id) => {
            let symbol =
                find_symbol(published, *symbol_id).ok_or(DeepMapReadFailure::TargetUnavailable)?;
            push_symbol("symbol", symbol, &mut preview, &mut evidence);
            for edge in published
                .publication()
                .graph()
                .edges()
                .iter()
                .filter(|edge| edge_contains_symbol(edge, *symbol_id))
                .take(24)
            {
                push_edge(edge, &mut preview, &mut evidence);
            }
        }
    }
    found(preview, evidence)
}

fn search_index(
    published: &PublishedIndex,
    action: &ExplorerSearchAction,
) -> Result<ExplorerObservation, DeepMapReadFailure> {
    let mut preview = BoundedPreview::new();
    let mut evidence = Vec::new();
    let limit = usize::from(action.limit().get()).min(MAX_EVIDENCE_IDS);
    match (action.kind(), action.query()) {
        (
            ExplorerSearchKind::Exact | ExplorerSearchKind::Lexical,
            ExplorerSearchQuery::Text(query),
        ) => {
            let needle = query.to_lowercase();
            for revision in published.publication().graph().files() {
                if evidence.len() >= limit {
                    break;
                }
                let path = path_display(revision.path());
                let candidate = path.to_lowercase();
                let matched = if action.kind() == ExplorerSearchKind::Exact {
                    candidate == needle
                } else {
                    candidate.contains(&needle)
                };
                if matched {
                    push_revision("file", revision, &mut preview, &mut evidence);
                }
            }
            for symbol in published.publication().graph().symbols() {
                if evidence.len() >= limit {
                    break;
                }
                let name = symbol.parsed().name().as_str().to_lowercase();
                let signature = symbol
                    .parsed()
                    .signature()
                    .map(|value| value.as_str().to_lowercase());
                let matched = if action.kind() == ExplorerSearchKind::Exact {
                    name == needle || signature.as_deref() == Some(needle.as_str())
                } else {
                    name.contains(&needle)
                        || signature
                            .as_deref()
                            .is_some_and(|value| value.contains(&needle))
                };
                if matched {
                    push_symbol("symbol", symbol, &mut preview, &mut evidence);
                }
            }
        }
        (kind, ExplorerSearchQuery::Symbol(symbol_id))
            if !matches!(
                kind,
                ExplorerSearchKind::Exact | ExplorerSearchKind::Lexical
            ) =>
        {
            if find_symbol(published, *symbol_id).is_none() {
                return Err(DeepMapReadFailure::TargetUnavailable);
            }
            for edge in published
                .publication()
                .graph()
                .edges()
                .iter()
                .filter(|edge| edge_matches_search(edge, kind, *symbol_id))
                .take(limit)
            {
                push_edge(edge, &mut preview, &mut evidence);
            }
        }
        _ => return Err(DeepMapReadFailure::Rejected),
    }
    if evidence.is_empty() {
        Ok(ExplorerObservation::not_found())
    } else {
        found(preview, evidence)
    }
}

fn found(
    preview: BoundedPreview,
    evidence: Vec<ModuleCardEvidenceId>,
) -> Result<ExplorerObservation, DeepMapReadFailure> {
    if evidence.is_empty() {
        return Ok(ExplorerObservation::not_found());
    }
    let truncated = preview.truncated;
    ExplorerObservation::found(preview.finish(), evidence, truncated)
        .map_err(|_| DeepMapReadFailure::InvalidResponse)
}

fn push_revision(
    label: &str,
    revision: &a3_domain::FileRevision,
    preview: &mut BoundedPreview,
    evidence: &mut Vec<ModuleCardEvidenceId>,
) {
    if evidence.len() >= MAX_EVIDENCE_IDS {
        preview.mark_truncated();
        return;
    }
    let id = ModuleCardEvidenceId::for_file_revision_v1(revision);
    preview.push(format!(
        "{label} path={} evidence_id={}",
        path_display(revision.path()),
        hex(id.as_bytes())
    ));
    evidence.push(id);
}

fn push_symbol(
    label: &str,
    symbol: &GraphSymbol,
    preview: &mut BoundedPreview,
    evidence: &mut Vec<ModuleCardEvidenceId>,
) {
    if evidence.len() >= MAX_EVIDENCE_IDS {
        preview.mark_truncated();
        return;
    }
    let id = ModuleCardEvidenceId::for_symbol_v1(symbol);
    preview.push(format!(
        "{label} name={} kind={:?} visibility={:?} path={} symbol_id={} evidence_id={}",
        symbol.parsed().name().as_str(),
        symbol.parsed().kind(),
        symbol.parsed().visibility(),
        path_display(symbol.revision().path()),
        symbol.id(),
        hex(id.as_bytes())
    ));
    evidence.push(id);
}

fn push_edge(
    edge: &GraphEdge,
    preview: &mut BoundedPreview,
    evidence: &mut Vec<ModuleCardEvidenceId>,
) {
    if evidence.len() >= MAX_EVIDENCE_IDS {
        preview.mark_truncated();
        return;
    }
    let id = ModuleCardEvidenceId::for_graph_edge_v1(edge);
    preview.push(format!(
        "relation kind={} source={} target={} evidence_id={}",
        relation_kind(edge.kind()),
        endpoint_display(edge.source()),
        endpoint_display(edge.target()),
        hex(id.as_bytes())
    ));
    evidence.push(id);
}

fn find_symbol(published: &PublishedIndex, symbol_id: SymbolId) -> Option<&GraphSymbol> {
    published
        .publication()
        .graph()
        .symbols()
        .iter()
        .find(|symbol| symbol.id() == symbol_id)
}

fn edge_contains_symbol(edge: &GraphEdge, symbol_id: SymbolId) -> bool {
    matches!(edge.source(), GraphEndpoint::Symbol(value) if *value == symbol_id)
        || matches!(edge.target(), GraphEndpoint::Symbol(value) if *value == symbol_id)
}

fn edge_matches_search(edge: &GraphEdge, kind: ExplorerSearchKind, symbol_id: SymbolId) -> bool {
    let source = matches!(edge.source(), GraphEndpoint::Symbol(value) if *value == symbol_id);
    let target = matches!(edge.target(), GraphEndpoint::Symbol(value) if *value == symbol_id);
    match kind {
        ExplorerSearchKind::Callers => target && edge.kind() == SyntaxRelationKind::Calls,
        ExplorerSearchKind::Callees => source && edge.kind() == SyntaxRelationKind::Calls,
        ExplorerSearchKind::Imports => source && edge.kind() == SyntaxRelationKind::Imports,
        ExplorerSearchKind::Exports => source && edge.kind() == SyntaxRelationKind::Exports,
        ExplorerSearchKind::Tests => (source || target) && edge.kind() == SyntaxRelationKind::Tests,
        ExplorerSearchKind::Exact | ExplorerSearchKind::Lexical => false,
    }
}

fn endpoint_display(endpoint: &GraphEndpoint) -> String {
    match endpoint {
        GraphEndpoint::File(path) => format!("file:{}", path_display(path)),
        GraphEndpoint::Symbol(symbol_id) => format!("symbol:{symbol_id}"),
    }
}

fn module_root(root: Option<&ModuleRoot>) -> String {
    match root {
        Some(ModuleRoot::Repository) => "repository".to_owned(),
        Some(ModuleRoot::Directory(path)) => path_display(path),
        None => "graph-community".to_owned(),
    }
}

const fn module_kind(kind: a3_domain::ModuleKind) -> &'static str {
    match kind {
        a3_domain::ModuleKind::ManifestBoundary => "manifest-boundary",
        a3_domain::ModuleKind::PathBoundary => "path-boundary",
        a3_domain::ModuleKind::GraphCommunity => "graph-community",
    }
}

const fn relation_kind(kind: SyntaxRelationKind) -> &'static str {
    match kind {
        SyntaxRelationKind::Contains => "contains",
        SyntaxRelationKind::Defines => "defines",
        SyntaxRelationKind::Imports => "imports",
        SyntaxRelationKind::Exports => "exports",
        SyntaxRelationKind::Calls => "calls",
        SyntaxRelationKind::Implements => "implements",
        SyntaxRelationKind::Extends => "extends",
        SyntaxRelationKind::Reads => "reads",
        SyntaxRelationKind::Writes => "writes",
        SyntaxRelationKind::Configures => "configures",
        SyntaxRelationKind::Tests => "tests",
        SyntaxRelationKind::Builds => "builds",
        SyntaxRelationKind::Documents => "documents",
    }
}

fn path_display(path: &RepositoryPath) -> String {
    String::from_utf8_lossy(path.as_bytes())
        .chars()
        .take(MAX_PATH_DISPLAY_CHARS)
        .map(|character| {
            if character.is_control() {
                '\u{fffd}'
            } else {
                character
            }
        })
        .collect()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

struct BoundedPreview {
    value: String,
    truncated: bool,
}

impl BoundedPreview {
    fn new() -> Self {
        Self {
            value: String::new(),
            truncated: false,
        }
    }

    fn push(&mut self, line: String) {
        if self.truncated {
            return;
        }
        let separator = usize::from(!self.value.is_empty());
        let available = MAX_PREVIEW_BYTES.saturating_sub(self.value.len() + separator);
        if line.len() <= available {
            if separator == 1 {
                self.value.push('\n');
            }
            self.value.push_str(&line);
            return;
        }
        if separator == 1 && available > 0 {
            self.value.push('\n');
        }
        let mut end = available.min(line.len());
        while end > 0 && !line.is_char_boundary(end) {
            end -= 1;
        }
        self.value.push_str(&line[..end]);
        self.truncated = true;
    }

    const fn mark_truncated(&mut self) {
        self.truncated = true;
    }

    fn finish(self) -> String {
        self.value
    }
}

#[derive(Debug)]
struct PublishedReadControl<'a> {
    control: &'a dyn DeepMapReadControl,
    started: Instant,
    timeout: Duration,
}

impl<'a> PublishedReadControl<'a> {
    fn new(control: &'a dyn DeepMapReadControl, timeout: Duration) -> Self {
        Self {
            control,
            started: Instant::now(),
            timeout,
        }
    }

    fn ensure_active(&self) -> Result<(), DeepMapReadFailure> {
        if self.control.is_cancelled() {
            Err(DeepMapReadFailure::Cancelled)
        } else if self.started.elapsed() >= self.timeout {
            Err(DeepMapReadFailure::TimedOut)
        } else {
            Ok(())
        }
    }

    fn classify(&self, failure: KnowledgeIndexFailure) -> DeepMapReadFailure {
        if self.control.is_cancelled() || failure == KnowledgeIndexFailure::Cancelled {
            DeepMapReadFailure::Cancelled
        } else if self.started.elapsed() >= self.timeout
            || failure == KnowledgeIndexFailure::TimedOut
        {
            DeepMapReadFailure::TimedOut
        } else {
            DeepMapReadFailure::SnapshotUnavailable
        }
    }
}

impl IndexPersistenceControl for PublishedReadControl<'_> {
    fn is_cancelled(&self) -> bool {
        self.control.is_cancelled() || self.started.elapsed() >= self.timeout
    }

    fn report_progress(&self, _progress: Progress) -> Result<(), IndexPersistenceControlError> {
        self.ensure_active()
            .map_err(|_| IndexPersistenceControlError::Unavailable)
    }
}

#[cfg(test)]
mod tests {
    use super::{MAX_PATH_DISPLAY_CHARS, path_display};
    use a3_domain::RepositoryPath;
    use std::error::Error;

    #[test]
    fn model_path_display_is_control_free_and_individually_bounded() -> Result<(), Box<dyn Error>> {
        let controlled = RepositoryPath::try_from_bytes(b"src/\nmodule.rs".to_vec())?;
        assert_eq!(path_display(&controlled), "src/\u{fffd}module.rs");

        let long = (0..20)
            .map(|_| "a".repeat(40))
            .collect::<Vec<_>>()
            .join("/");
        let long = RepositoryPath::try_from_bytes(long.into_bytes())?;
        assert_eq!(path_display(&long).chars().count(), MAX_PATH_DISPLAY_CHARS);
        Ok(())
    }
}
