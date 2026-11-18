//! Pure defensive graph/anchor boundaries; real adapter/executor evidence lives in native tests.
use super::*;
use a3_domain::{
    IndexLanguage, LanguageAdapterContractVersion, LanguageAdapterVersion, LanguageParseArtifacts,
    ParseCoverage, SourcePosition, SymbolName, SyntaxRelation,
};
use std::{
    error::Error,
    sync::atomic::{AtomicBool, Ordering},
};
type TestResult<T = ()> = Result<T, Box<dyn Error>>;

#[derive(Debug, Default)]
struct Control(AtomicBool);
impl WorkspacePatchControl for Control {
    fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
    fn report_progress(&self, _: Progress) -> Result<(), crate::WorkspacePatchProgressError> {
        Ok(())
    }
}
#[derive(Clone, Copy)]
enum Shape {
    Valid,
    Heuristic,
    MultipleParents,
    Cycle,
}
fn range(start: usize, end: usize) -> TestResult<SourceRange> {
    Ok(SourceRange::new(
        start,
        end,
        SourcePosition::new(0, u32::try_from(start)?),
        SourcePosition::new(0, u32::try_from(end)?),
    )?)
}
fn parsed(ids: [u32; 4], shape: Shape) -> TestResult<(LanguageParseResult, Vec<u8>)> {
    let bytes = vec![b' '; 64];
    let revision = FileRevision::new(
        a3_domain::RepositoryPath::try_from_bytes(b"neutral.fixture".to_vec())?,
        ContentHash::from_bytes(*blake3::hash(&bytes).as_bytes()),
    );
    let ids = ids
        .map(LocalSymbolId::new)
        .into_iter()
        .collect::<Result<Vec<_>, _>>()?;
    let mut symbols = Vec::new();
    for (i, kind, start, end, selection_start, selection_end) in [
        (0, SymbolKind::Module, 0, 64, 0, 0),
        (1, SymbolKind::Class, 0, 60, 1, 2),
        (
            2,
            SymbolKind::Method,
            if matches!(shape, Shape::Cycle) { 0 } else { 8 },
            if matches!(shape, Shape::Cycle) {
                60
            } else {
                20
            },
            12,
            13,
        ),
        (3, SymbolKind::Method, 24, 40, 28, 29),
    ] {
        symbols.push(ParsedSymbol::new(
            ids[i],
            kind,
            SymbolName::try_from_string("same".to_owned())?,
            range(start, end)?,
            range(selection_start, selection_end)?,
        )?);
    }
    let mut relations = vec![SyntaxRelation::new(
        SyntaxSource::File,
        SyntaxTarget::Symbol(ids[0]),
        SyntaxRelationKind::Defines,
        SyntaxProvider::TreeSitter,
        Confidence::certain(),
        range(0, 64)?,
    )];
    for (parent, child, provider) in [
        (
            if matches!(shape, Shape::Cycle) { 2 } else { 0 },
            1,
            SyntaxProvider::TreeSitter,
        ),
        (1, 2, SyntaxProvider::TreeSitter),
        (
            1,
            3,
            if matches!(shape, Shape::Heuristic) {
                SyntaxProvider::LanguageHeuristic
            } else {
                SyntaxProvider::TreeSitter
            },
        ),
    ] {
        relations.push(SyntaxRelation::new(
            SyntaxSource::Symbol(ids[parent]),
            SyntaxTarget::Symbol(ids[child]),
            SyntaxRelationKind::Contains,
            provider,
            Confidence::certain(),
            symbols[child].declaration_range(),
        ));
    }
    if matches!(shape, Shape::MultipleParents) {
        relations.push(SyntaxRelation::new(
            SyntaxSource::Symbol(ids[0]),
            SyntaxTarget::Symbol(ids[3]),
            SyntaxRelationKind::Contains,
            SyntaxProvider::TreeSitter,
            Confidence::certain(),
            range(24, 40)?,
        ));
    }
    Ok((
        LanguageParseResult::new(
            revision,
            LanguageAdapterRevision::new(
                IndexLanguage::Python,
                LanguageAdapterVersion::try_from_string("neutral-test-v1".to_owned())?,
            ),
            LanguageAdapterContractVersion::v1(),
            ParseCoverage::complete(bytes.len())?,
            LanguageParseArtifacts {
                symbols,
                relations,
                diagnostics: Vec::new(),
            },
        )?,
        bytes,
    ))
}
fn clock() -> TestResult<EditOwnerClock> {
    Ok(EditOwnerClock::new(
        AgentRunTimestamp::from_unix_millis(0)?,
        AgentRunTimestamp::from_unix_millis(10000)?,
    ))
}

#[test]
fn parent_frames_use_current_local_ids_only_within_each_parse() -> TestResult {
    let owner = Control::default();
    let mut observed = Vec::new();
    for ids in [[7, 17, 41, 99], [900, 3, 4, 1]] {
        let (p, bytes) = parsed(ids, Shape::Valid)?;
        let graph = ParentGraph::build(&p, &bytes, clock()?, &owner)?;
        let i = p
            .symbols()
            .iter()
            .position(|s| s.selection_range().start_byte() == 28)
            .ok_or("suffix")?;
        let parent = graph.parent[i].ok_or("parent")?;
        observed.push((
            header_key(&p.symbols()[i]),
            header_key(&p.symbols()[parent]),
            graph.root == parent,
        ));
    }
    assert_eq!(observed[0], observed[1]);
    assert!(!observed[0].2);
    Ok(())
}
#[test]
fn heuristic_multiple_or_cyclic_parent_relations_cannot_establish_a_frame() -> TestResult {
    for shape in [Shape::Heuristic, Shape::MultipleParents, Shape::Cycle] {
        let (p, bytes) = parsed([7, 17, 41, 99], shape)?;
        assert!(matches!(
            ParentGraph::build(&p, &bytes, clock()?, &Control::default()),
            Err(E::ScopeFrameUnproven)
        ));
    }
    Ok(())
}
#[test]
fn outside_header_mapping_uses_only_checked_final_byte_delta() -> TestResult {
    let window = range(8, 20)?;
    let prefix = HeaderKey {
        start: 0,
        selection_start: 1,
        selection_end: 2,
        kind: SymbolKind::Class,
    };
    let suffix = HeaderKey {
        start: 24,
        selection_start: 28,
        selection_end: 29,
        kind: SymbolKind::Method,
    };
    assert_eq!(mapped(prefix, window, 1300)?, prefix);
    assert_eq!(
        mapped(suffix, window, 1300)?,
        HeaderKey {
            start: 1324,
            selection_start: 1328,
            selection_end: 1329,
            kind: SymbolKind::Method
        }
    );
    assert!(matches!(
        mapped(suffix, window, -25),
        Err(E::ScopeFrameUnproven)
    ));
    assert!(matches!(
        mapped(suffix, window, i64::from(u32::MAX)),
        Err(E::ScopeFrameUnproven)
    ));
    Ok(())
}
#[test]
fn original_deadline_and_cancellation_are_distinct_control_failures() -> TestResult {
    let owner = Control::default();
    let expired = EditOwnerClock::new(
        AgentRunTimestamp::from_unix_millis(100)?,
        AgentRunTimestamp::from_unix_millis(100)?,
    );
    assert!(matches!(expired.check(&owner), Err(E::DeadlineReached)));
    assert!(LanguageParseControl::is_cancelled(&expired.control(&owner)));
    owner.0.store(true, Ordering::SeqCst);
    assert!(matches!(clock()?.check(&owner), Err(E::Cancelled)));
    Ok(())
}
