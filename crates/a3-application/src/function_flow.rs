//! Shared bounded reads over the existing published Fast Index.
mod document;
mod trace;
pub use document::FunctionFlowReadDocument;
pub use trace::{FlowTraceDirection, FlowValueAddress, FlowValueTrace, FlowValueTraceNode};

use crate::{
    IndexPersistenceControl, IndexPersistenceControlError, KnowledgeIndexFailure,
    KnowledgeIndexStore,
};
use a3_domain::{
    EvidenceRef, FlowStepId, GraphSymbol, IndexRunId, IndexedFunctionFlow, Progress,
    ProjectIdentity, PublishedIndex, SnapshotId, SymbolId, SymbolKind,
};
use std::{
    collections::BTreeMap,
    error::Error,
    fmt,
    sync::Arc,
    time::{Duration, Instant},
};

/// Fixed per-response bound shared by UI and agent readers.
pub const FUNCTION_FLOW_PAGE_SIZE: usize = 50;
/// Maximum distinct call contexts in one query.
pub const FUNCTION_FLOW_MAX_CONTEXTS: usize = 8;
/// Content-bound root and exact call occurrences traversed from it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionFlowSelection {
    /// Existing published run, never a second analysis run.
    pub run_id: IndexRunId,
    /// Callable selected in that run.
    pub root: SymbolId,
    /// Exact occurrences; repeated calls to one target remain distinct.
    pub call_path: Vec<FlowStepId>,
}
/// One frame reached by a verified call link.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionFlowFrame {
    /// Current content-addressed symbol.
    pub owner: GraphSymbol,
    /// Bounded immutable analysis, kept inside the application boundary.
    pub flow: IndexedFunctionFlow,
}
/// Bounded inspection and all source revisions used to establish its context.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionFlowInspection {
    /// Exact selection, including the occurrence path.
    pub selection: FunctionFlowSelection,
    /// Snapshot of every frame.
    pub snapshot_id: SnapshotId,
    /// Root through selected callee, at most eight frames.
    pub frames: Vec<FunctionFlowFrame>,
}
impl FunctionFlowInspection {
    /// Returns source evidence for every expanded frame, not only the final file.
    #[must_use]
    pub fn evidence(&self) -> Vec<EvidenceRef> {
        self.frames
            .iter()
            .map(|f| EvidenceRef::new(f.owner.revision().clone(), f.flow.analysis().range()))
            .collect()
    }
}
/// One fixed-size callable inventory page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionFlowCatalog {
    /// Exact existing run containing the results.
    pub run_id: IndexRunId,
    /// Immutable source observation.
    pub snapshot_id: SnapshotId,
    /// At most fifty symbols matching the query.
    pub symbols: Vec<GraphSymbol>,
    /// Whether another fixed page exists.
    pub has_more: bool,
}
/// Common read use case for the desktop and harness.
#[derive(Debug, Clone)]
pub struct ExploreFunctionFlows {
    store: Arc<dyn KnowledgeIndexStore>,
}
impl ExploreFunctionFlows {
    /// Uses the existing KnowledgeIndexStore capability.
    #[must_use]
    pub fn new(store: Arc<dyn KnowledgeIndexStore>) -> Self {
        Self { store }
    }

    /// Searches callable names and repository paths without loading function bodies.
    pub async fn catalog(
        &self,
        project: &ProjectIdentity,
        term: &str,
        offset: usize,
        control: &dyn IndexPersistenceControl,
    ) -> Result<Option<FunctionFlowCatalog>, FunctionFlowReadFailure> {
        if term.len() > 512 || offset > 1_000_000 {
            return Err(FunctionFlowReadFailure::InvalidQuery);
        }
        let bounded = BoundedControl::new(control);
        let Some(index) = self.store.latest_published_index(project, &bounded).await? else {
            return Ok(None);
        };
        let term = term.to_lowercase();
        if !self
            .store
            .latest_snapshot(project)
            .await?
            .is_some_and(|snapshot| {
                snapshot.index_schema_version() >= a3_domain::IndexSchemaVersion::v6()
                    && snapshot.id() == index.run().snapshot_id()
            })
        {
            return Ok(None);
        }
        let mut matches = Vec::new();
        let mut skipped = 0usize;
        for (ordinal, symbol) in index.publication().graph().symbols().iter().enumerate() {
            if ordinal % 128 == 0 {
                bounded.check()?;
            }
            if !matches!(
                symbol.parsed().kind(),
                SymbolKind::Function | SymbolKind::Method | SymbolKind::Module
            ) {
                continue;
            }
            if symbol.parsed().kind() == SymbolKind::Module
                && (symbol.parsed().declaration_range().start_byte() != 0
                    || ![
                        b".rs".as_slice(),
                        b".py",
                        b".ts",
                        b".tsx",
                        b".js",
                        b".jsx",
                        b".mjs",
                        b".cjs",
                        b".mts",
                        b".cts",
                    ]
                    .iter()
                    .any(|ext| symbol.revision().path().as_bytes().ends_with(ext)))
            {
                continue;
            }
            if !term.is_empty()
                && !symbol
                    .parsed()
                    .name()
                    .as_str()
                    .to_lowercase()
                    .contains(&term)
                && !String::from_utf8_lossy(symbol.revision().path().as_bytes())
                    .to_lowercase()
                    .contains(&term)
            {
                continue;
            }
            if skipped < offset {
                skipped += 1;
                continue;
            }
            matches.push(symbol.clone());
            if matches.len() > FUNCTION_FLOW_PAGE_SIZE {
                break;
            }
        }
        if !self.is_current(project, &index, &bounded).await? {
            return Ok(None);
        }
        let has_more = matches.len() > FUNCTION_FLOW_PAGE_SIZE;
        matches.truncate(FUNCTION_FLOW_PAGE_SIZE);
        Ok(Some(FunctionFlowCatalog {
            run_id: index.run().id(),
            snapshot_id: index.run().snapshot_id(),
            symbols: matches,
            has_more,
        }))
    }

    /// Follows only call targets issued by the same current Fast Index graph.
    pub async fn inspect(
        &self,
        project: &ProjectIdentity,
        selection: &FunctionFlowSelection,
        control: &dyn IndexPersistenceControl,
    ) -> Result<Option<FunctionFlowInspection>, FunctionFlowReadFailure> {
        if selection.call_path.len() >= FUNCTION_FLOW_MAX_CONTEXTS {
            return Err(FunctionFlowReadFailure::InvalidQuery);
        }
        let bounded = BoundedControl::new(control);
        let Some(index) = self.store.latest_published_index(project, &bounded).await? else {
            return Ok(None);
        };
        if index.run().id() != selection.run_id {
            return Ok(None);
        }
        let owners = index
            .publication()
            .graph()
            .symbols()
            .iter()
            .map(|s| (s.id(), s))
            .collect::<BTreeMap<_, _>>();
        let mut frames = Vec::new();
        let mut target = selection.root;
        for depth in 0..=selection.call_path.len() {
            bounded.check()?;
            let Some(owner) = owners.get(&target) else {
                return Ok(None);
            };
            let Some(flow) = self
                .store
                .read_function_flow(project, selection.run_id, owner, &bounded)
                .await?
            else {
                return Ok(None);
            };
            let next = selection
                .call_path
                .get(depth)
                .and_then(|step| flow.calls().iter().find(|c| c.step == *step))
                .and_then(|c| c.target);
            frames.push(FunctionFlowFrame {
                owner: (*owner).clone(),
                flow,
            });
            if depth < selection.call_path.len() {
                let Some(next) = next else {
                    return Ok(None);
                };
                target = next;
            }
        }
        if !self.is_current(project, &index, &bounded).await? {
            return Ok(None);
        }
        // Revalidate persisted targets, not only their local IDs. A corrupted JSON body must
        // not redirect a real occurrence to an unrelated current symbol.
        a3_domain::FunctionFlowBatch::new(
            index.publication(),
            frames
                .iter()
                .map(|f| (f.flow.symbol(), f.flow.clone()))
                .collect::<std::collections::BTreeMap<_, _>>()
                .into_values()
                .collect(),
        )
        .map_err(|_| {
            FunctionFlowReadFailure::Storage(KnowledgeIndexFailure::IndexPublicationMismatch)
        })?;
        bounded.check()?;
        Ok(Some(FunctionFlowInspection {
            selection: selection.clone(),
            snapshot_id: index.run().snapshot_id(),
            frames,
        }))
    }

    async fn is_current(
        &self,
        project: &ProjectIdentity,
        index: &PublishedIndex,
        control: &BoundedControl<'_>,
    ) -> Result<bool, FunctionFlowReadFailure> {
        control.check()?;
        let snapshot = self.store.latest_snapshot(project).await?;
        let run = self.store.latest_published_index_run(project).await?;
        control.check()?;
        Ok(
            snapshot.is_some_and(|s| s.id() == index.run().snapshot_id())
                && run.is_some_and(|r| r.id() == index.run().id()),
        )
    }
}
struct BoundedControl<'a> {
    inner: &'a dyn IndexPersistenceControl,
    started: Instant,
}
impl<'a> BoundedControl<'a> {
    fn new(inner: &'a dyn IndexPersistenceControl) -> Self {
        Self {
            inner,
            started: Instant::now(),
        }
    }
    fn check(&self) -> Result<(), FunctionFlowReadFailure> {
        if self.inner.is_cancelled() {
            return Err(FunctionFlowReadFailure::Storage(
                KnowledgeIndexFailure::Cancelled,
            ));
        }
        if self.started.elapsed() > Duration::from_secs(2) {
            return Err(FunctionFlowReadFailure::Storage(
                KnowledgeIndexFailure::TimedOut,
            ));
        }
        Ok(())
    }
}
impl fmt::Debug for BoundedControl<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("BoundedFunctionFlowRead")
    }
}
impl IndexPersistenceControl for BoundedControl<'_> {
    fn is_cancelled(&self) -> bool {
        self.check().is_err()
    }
    fn report_progress(&self, _: Progress) -> Result<(), IndexPersistenceControlError> {
        Ok(())
    }
}
/// Stable, source-free read failures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FunctionFlowReadFailure {
    /// Query exceeded the fixed shape or resource contract.
    InvalidQuery,
    /// Existing persistence capability failed or the read was cancelled.
    Storage(KnowledgeIndexFailure),
}
impl From<KnowledgeIndexFailure> for FunctionFlowReadFailure {
    fn from(e: KnowledgeIndexFailure) -> Self {
        Self::Storage(e)
    }
}
impl fmt::Display for FunctionFlowReadFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("function flow read unavailable")
    }
}
impl Error for FunctionFlowReadFailure {}
