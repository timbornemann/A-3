//! Current complete-callable qualification and exact outside-header ancestry.
use crate::{
    EditPhaseFailure as E, EditPhaseState, LanguageAdapter, LanguageParseControl,
    LanguageParseControlError, LanguageParseInput, LanguageParsePolicy, WorkspacePatchControl,
};
use a3_domain::{
    AgentRunTimestamp, Confidence, ContentHash, DiscoveredFileRoles, EvidenceRef, FileRevision,
    IndexRunId, LanguageAdapterRevision, LanguageParseResult, LocalSymbolId, ParsedSymbol,
    Progress, SourceRange, SymbolKind, SyntaxProvider, SyntaxRelationKind, SyntaxSource,
    SyntaxTarget,
};
use std::{collections::BTreeMap, time::Instant};

#[derive(Debug, Clone, Copy)]
pub(crate) struct EditOwnerClock {
    started: Instant,
    origin: AgentRunTimestamp,
    deadline: AgentRunTimestamp,
}
impl EditOwnerClock {
    pub(crate) fn new(origin: AgentRunTimestamp, deadline: AgentRunTimestamp) -> Self {
        Self {
            started: Instant::now(),
            origin,
            deadline,
        }
    }
    pub(crate) fn now(self) -> Result<AgentRunTimestamp, E> {
        crate::model_wire::phase_elapsed_timestamp(self.started, self.origin)
            .ok_or(E::DeadlineReached)
    }
    pub(crate) fn control(self, owner: &dyn WorkspacePatchControl) -> ScopeControl<'_> {
        ScopeControl { clock: self, owner }
    }
    pub(crate) fn deadline(self) -> AgentRunTimestamp {
        self.deadline
    }
    pub(crate) fn check(self, control: &dyn WorkspacePatchControl) -> Result<(), E> {
        if control.is_cancelled() {
            return Err(E::Cancelled);
        }
        if self.now()? >= self.deadline {
            return Err(E::DeadlineReached);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct HeaderKey {
    start: u32,
    selection_start: u32,
    selection_end: u32,
    kind: SymbolKind,
}
#[derive(Debug, Clone, PartialEq, Eq)]
struct OutsideHeader {
    key: HeaderKey,
    bytes: ContentHash,
    parent: Option<HeaderKey>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EditScopeFrame {
    revision: FileRevision,
    range: SourceRange,
    index: IndexRunId,
    adapter: LanguageAdapterRevision,
    headers: Vec<OutsideHeader>,
    digest: ContentHash,
}

/// Whole-line envelopes use the existing page matcher; Need offerings are independent.
pub(crate) fn eligible(current: EditPhaseState<'_>, page: &EvidenceRef) -> bool {
    let Some(analysis) = current
        .published()
        .publication()
        .file_analyses()
        .iter()
        .find(|a| a.revision() == page.revision())
    else {
        return false;
    };
    if analysis.adapter_revision().is_none()
        || !analysis.coverage().is_some_and(|c| c.is_complete())
        || !analysis.diagnostics().is_empty()
    {
        return false;
    }
    geometrically_whole(current, page)
}

fn geometrically_whole(current: EditPhaseState<'_>, page: &EvidenceRef) -> bool {
    let mut matches = current
        .published()
        .publication()
        .graph()
        .symbols()
        .iter()
        .filter(|symbol| {
            symbol.revision() == page.revision()
                && matches!(
                    symbol.parsed().kind(),
                    SymbolKind::Function | SymbolKind::Method
                )
                && crate::bound_edit_source::definition_page_matches(
                    &EvidenceRef::new(
                        symbol.revision().clone(),
                        symbol.parsed().declaration_range(),
                    ),
                    page,
                )
        });
    matches.next().is_some() && matches.next().is_none()
}

impl EditScopeFrame {
    pub(crate) fn index(&self) -> IndexRunId {
        self.index
    }
    pub(crate) fn digest(&self) -> ContentHash {
        self.digest
    }
    pub(crate) fn require_current(&self, current: EditPhaseState<'_>) -> Result<(), E> {
        if self.index != current.published().run().id()
            || current
                .published()
                .publication()
                .graph()
                .files()
                .iter()
                .all(|r| r != &self.revision)
        {
            return Err(E::InactiveOrStale);
        }
        Ok(())
    }
    pub(crate) fn capture(
        current: EditPhaseState<'_>,
        page: &EvidenceRef,
        editable: SourceRange,
        source: &[u8],
        adapter: &dyn LanguageAdapter,
        clock: EditOwnerClock,
        control: &dyn WorkspacePatchControl,
    ) -> Result<Self, E> {
        clock.check(control)?;
        let analysis = current
            .published()
            .publication()
            .file_analyses()
            .iter()
            .find(|a| a.revision() == page.revision())
            .ok_or(E::ScopeFrameUnproven)?;
        if analysis.adapter_revision() != Some(adapter.revision())
            || adapter.contract_version() != LanguageParsePolicy::v1().contract_version()
            || !adapter.supports_path(page.revision().path())
        {
            return Err(E::ScopeFrameUnproven);
        }
        let parsed = parse(source, page.revision(), adapter, clock, control)?;
        // Independently reproduce the selected complete callable, not a model label.
        let mut matches = parsed.symbols().iter().filter(|symbol| {
            matches!(symbol.kind(), SymbolKind::Function | SymbolKind::Method)
                && crate::bound_edit_source::definition_page_matches(
                    &EvidenceRef::new(page.revision().clone(), symbol.declaration_range()),
                    page,
                )
        });
        let selected = matches.next().ok_or(E::ScopeFrameUnproven)?;
        if matches.next().is_some()
            || !current
                .published()
                .publication()
                .graph()
                .symbols()
                .iter()
                .any(|s| {
                    s.revision() == page.revision()
                        && s.parsed().kind() == selected.kind()
                        && s.parsed().declaration_range() == selected.declaration_range()
                        && s.parsed().selection_range() == selected.selection_range()
                })
        {
            return Err(E::ScopeFrameUnproven);
        }
        let graph = ParentGraph::build(&parsed, source, clock, control)?;
        let mut headers = Vec::new();
        let start = editable.start_byte();
        let end = editable.end_byte();
        for (i, symbol) in parsed.symbols().iter().enumerate() {
            clock.check(control)?;
            if i == graph.root {
                continue;
            }
            let key = header_key(symbol);
            if key.start >= start && key.start < end {
                continue;
            }
            if key.start < end && key.selection_end > start {
                return Err(E::ScopeFrameUnproven);
            }
            let parent = graph.parent[i].ok_or(E::ScopeFrameUnproven)?;
            let parent = if parent == graph.root {
                None
            } else {
                let parent = header_key(&parsed.symbols()[parent]);
                if parent.start >= start && parent.start < end {
                    return Err(E::ScopeFrameUnproven);
                }
                Some(parent)
            };
            headers.push(OutsideHeader {
                key,
                bytes: header_hash(source, key)?,
                parent,
            });
        }
        headers.sort_by_key(|h| h.key);
        let mut h = blake3::Hasher::new_derive_key("a3 edit lexical outside frame v1");
        h.update(&[1]); // fixed rule version; never model supplied
        field(&mut h, page.revision().path().as_bytes());
        h.update(page.revision().content_hash().as_bytes());
        h.update(current.published().run().id().as_bytes());
        h.update(current.published().run().snapshot_id().as_bytes());
        h.update(&current.published().run().sequence().get().to_le_bytes());
        h.update(&page.range().start_byte().to_le_bytes());
        h.update(&page.range().end_byte().to_le_bytes());
        h.update(&start.to_le_bytes());
        h.update(&end.to_le_bytes());
        field(
            &mut h,
            format!("{:?}", adapter.revision().language()).as_bytes(),
        );
        field(&mut h, adapter.revision().version().as_str().as_bytes());
        h.update(&adapter.contract_version().get().to_le_bytes());
        h.update(&(headers.len() as u64).to_le_bytes());
        for header in &headers {
            hash_key(&mut h, header.key);
            h.update(header.bytes.as_bytes());
            h.update(&[u8::from(header.parent.is_some())]);
            if let Some(parent) = header.parent {
                hash_key(&mut h, parent);
            }
        }
        clock.check(control)?;
        Ok(Self {
            revision: page.revision().clone(),
            range: editable,
            index: current.published().run().id(),
            adapter: adapter.revision().clone(),
            headers,
            digest: ContentHash::from_bytes(*h.finalize().as_bytes()),
        })
    }
    pub(crate) fn verify(
        &self,
        current: EditPhaseState<'_>,
        base_len: usize,
        candidate: &[u8],
        adapter: &dyn LanguageAdapter,
        clock: EditOwnerClock,
        control: &dyn WorkspacePatchControl,
    ) -> Result<(), E> {
        self.require_current(current)?;
        clock.check(control)?;
        if adapter.revision() != &self.adapter {
            return Err(E::ScopeFrameUnproven);
        }
        let revision = FileRevision::new(
            self.revision.path().clone(),
            ContentHash::from_bytes(*blake3::hash(candidate).as_bytes()),
        );
        let parsed = parse(candidate, &revision, adapter, clock, control)?;
        let graph = ParentGraph::build(&parsed, candidate, clock, control)?;
        let mut candidates = BTreeMap::new();
        for (i, symbol) in parsed.symbols().iter().enumerate() {
            clock.check(control)?;
            if i != graph.root && candidates.insert(header_key(symbol), i).is_some() {
                return Err(E::ScopeFrameUnproven);
            }
        }
        let delta = i64::try_from(candidate.len()).map_err(|_| E::ScopeFrameUnproven)?
            - i64::try_from(base_len).map_err(|_| E::ScopeFrameUnproven)?;
        for old in &self.headers {
            clock.check(control)?;
            let key = mapped(old.key, self.range, delta)?;
            let i = *candidates.get(&key).ok_or(E::ScopeFrameMismatch)?;
            if header_hash(candidate, key)? != old.bytes {
                return Err(E::ScopeFrameMismatch);
            }
            let parent = graph.parent[i].ok_or(E::ScopeFrameUnproven)?;
            let actual = (parent != graph.root).then(|| header_key(&parsed.symbols()[parent]));
            let expected = old
                .parent
                .map(|k| mapped(k, self.range, delta))
                .transpose()?;
            if actual != expected {
                return Err(E::ScopeFrameMismatch);
            }
        }
        clock.check(control)
    }
}

fn mapped(key: HeaderKey, range: SourceRange, delta: i64) -> Result<HeaderKey, E> {
    let shift = |position: u32| -> Result<u32, E> {
        let delta = if key.start >= range.end_byte() {
            delta
        } else {
            0
        };
        u32::try_from(
            i64::from(position)
                .checked_add(delta)
                .ok_or(E::ScopeFrameUnproven)?,
        )
        .map_err(|_| E::ScopeFrameUnproven)
    };
    Ok(HeaderKey {
        start: shift(key.start)?,
        selection_start: shift(key.selection_start)?,
        selection_end: shift(key.selection_end)?,
        kind: key.kind,
    })
}
fn header_key(s: &ParsedSymbol) -> HeaderKey {
    HeaderKey {
        start: s.declaration_range().start_byte(),
        selection_start: s.selection_range().start_byte(),
        selection_end: s.selection_range().end_byte(),
        kind: s.kind(),
    }
}
fn header_hash(source: &[u8], key: HeaderKey) -> Result<ContentHash, E> {
    let bytes = source
        .get(key.start as usize..key.selection_end as usize)
        .ok_or(E::ScopeFrameUnproven)?;
    Ok(ContentHash::from_bytes(*blake3::hash(bytes).as_bytes()))
}
fn field(h: &mut blake3::Hasher, bytes: &[u8]) {
    h.update(&(bytes.len() as u64).to_le_bytes());
    h.update(bytes);
}
fn hash_key(h: &mut blake3::Hasher, key: HeaderKey) {
    h.update(&key.start.to_le_bytes());
    h.update(&key.selection_start.to_le_bytes());
    h.update(&key.selection_end.to_le_bytes());
    field(h, format!("{:?}", key.kind).as_bytes());
}

pub(crate) struct ScopeControl<'a> {
    clock: EditOwnerClock,
    owner: &'a dyn WorkspacePatchControl,
}
impl std::fmt::Debug for ScopeControl<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("EditScopeControl")
    }
}
impl LanguageParseControl for ScopeControl<'_> {
    fn is_cancelled(&self) -> bool {
        self.clock.check(self.owner).is_err()
    }
    fn report_progress(&self, p: Progress) -> Result<(), LanguageParseControlError> {
        WorkspacePatchControl::report_progress(self.owner, p)
            .map_err(|_| LanguageParseControlError::Unavailable)
    }
}
fn parse(
    source: &[u8],
    revision: &FileRevision,
    adapter: &dyn LanguageAdapter,
    clock: EditOwnerClock,
    control: &dyn WorkspacePatchControl,
) -> Result<LanguageParseResult, E> {
    clock.check(control)?;
    let policy = LanguageParsePolicy::v1();
    if source.len() > policy.max_source_bytes() {
        return Err(E::ScopeFrameUnproven);
    }
    let result = adapter.parse(
        LanguageParseInput::new(revision, source, DiscoveredFileRoles::empty()),
        policy,
        &ScopeControl {
            clock,
            owner: control,
        },
    );
    clock.check(control)?; // distinguish a real owner deadline/cancellation from parser limits
    let p = result.map_err(|_| E::ScopeFrameUnproven)?;
    if p.revision() != revision
        || p.adapter_revision() != adapter.revision()
        || p.contract_version() != policy.contract_version()
        || !p.coverage().is_complete()
        || p.coverage().total_bytes() as usize != source.len()
        || !p.diagnostics().is_empty()
        || p.symbols().len() > policy.max_symbols()
        || p.relations().len() > policy.max_relations()
    {
        return Err(E::ScopeFrameUnproven);
    }
    Ok(p)
}

struct ParentGraph {
    parent: Vec<Option<usize>>,
    root: usize,
}
impl ParentGraph {
    fn build(
        p: &LanguageParseResult,
        bytes: &[u8],
        clock: EditOwnerClock,
        control: &dyn WorkspacePatchControl,
    ) -> Result<Self, E> {
        let ids: BTreeMap<LocalSymbolId, usize> = p
            .symbols()
            .iter()
            .enumerate()
            .map(|(i, s)| (s.id(), i))
            .collect();
        let mut root = None;
        for r in p.relations() {
            clock.check(control)?;
            if r.source() == SyntaxSource::File
                && r.kind() == SyntaxRelationKind::Defines
                && r.provider() == SyntaxProvider::TreeSitter
                && r.confidence() == Confidence::certain()
                && let SyntaxTarget::Symbol(id) = r.target()
            {
                let i = *ids.get(id).ok_or(E::ScopeFrameUnproven)?;
                let s = &p.symbols()[i];
                if s.kind() == SymbolKind::Module
                    && s.selection_range().is_empty()
                    && s.declaration_range().end_byte() as usize == bytes.len()
                    && root.replace(i).is_some()
                {
                    return Err(E::ScopeFrameUnproven);
                }
            }
        }
        let root = root.ok_or(E::ScopeFrameUnproven)?;
        let mut parent = vec![None; p.symbols().len()];
        for r in p.relations() {
            clock.check(control)?;
            if r.kind() != SyntaxRelationKind::Contains
                || r.provider() != SyntaxProvider::TreeSitter
                || r.confidence() != Confidence::certain()
            {
                continue;
            }
            let (SyntaxSource::Symbol(from), SyntaxTarget::Symbol(to)) = (r.source(), r.target())
            else {
                return Err(E::ScopeFrameUnproven);
            };
            let from = *ids.get(&from).ok_or(E::ScopeFrameUnproven)?;
            let to = *ids.get(to).ok_or(E::ScopeFrameUnproven)?;
            if from == to
                || to == root
                || parent[to].replace(from).is_some()
                || !p.symbols()[from]
                    .declaration_range()
                    .contains(p.symbols()[to].declaration_range())
            {
                return Err(E::ScopeFrameUnproven);
            }
        }
        let mut color = vec![0_u8; parent.len()];
        color[root] = 2;
        for start in 0..parent.len() {
            let mut path = Vec::new();
            let mut at = start;
            while color[at] != 2 {
                clock.check(control)?;
                if path.len() >= LanguageParsePolicy::v1().max_tree_depth() || color[at] == 1 {
                    return Err(E::ScopeFrameUnproven);
                }
                color[at] = 1;
                path.push(at);
                at = parent[at].ok_or(E::ScopeFrameUnproven)?;
            }
            for i in path {
                color[i] = 2;
            }
        }
        Ok(Self { parent, root })
    }
}

impl WorkspacePatchControl for ScopeControl<'_> {
    fn is_cancelled(&self) -> bool {
        self.clock.check(self.owner).is_err()
    }
    fn report_progress(
        &self,
        progress: Progress,
    ) -> Result<(), crate::WorkspacePatchProgressError> {
        self.owner.report_progress(progress)
    }
}

#[cfg(test)]
#[path = "edit_scope_frame_tests.rs"]
mod tests;
