use crate::{IndexPersistenceControl, KnowledgeIndexFailure, KnowledgeIndexStore};
use a3_domain::{
    IndexLanguage, ParseDiagnosticCode, ParseDiagnosticSeverity, ProjectIdentity, PublishedIndex,
    RepositoryPath, SnapshotId,
};
use std::error::Error;
use std::fmt;
use std::sync::Arc;

const MAX_VISIBLE_DIAGNOSTIC_FILES: usize = 64;
const MAX_VISIBLE_DIAGNOSTICS_PER_FILE: usize = 8;
const MAX_PATH_DISPLAY_CHARS: usize = 512;

/// Read-only use case for a bounded projection of the last atomically published index.
#[derive(Debug)]
pub struct GetPublishedIndexOverview {
    store: Arc<dyn KnowledgeIndexStore>,
}

impl GetPublishedIndexOverview {
    /// Wires the existing published-index capability without adding repository access.
    #[must_use]
    pub fn new(store: Arc<dyn KnowledgeIndexStore>) -> Self {
        Self { store }
    }

    /// Loads and projects only the latest complete publication for a validated project identity.
    pub async fn execute(
        &self,
        project: &ProjectIdentity,
        control: &dyn IndexPersistenceControl,
    ) -> Result<Option<PublishedIndexOverview>, GetPublishedIndexOverviewError> {
        self.store
            .latest_published_index(project, control)
            .await
            .map_err(GetPublishedIndexOverviewError::Storage)?
            .as_ref()
            .map(build_overview)
            .transpose()
    }
}

/// Bounded counts, coverage, and file-local diagnostic summaries for one publication.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublishedIndexOverview {
    snapshot_id: SnapshotId,
    file_count: u64,
    symbol_count: u64,
    diagnostic_count: u64,
    parsed_file_count: u64,
    coverage_basis_points: Option<u16>,
    diagnostic_file_count: u64,
    diagnostic_files: Vec<PublishedFileDiagnostics>,
    diagnostic_files_truncated: bool,
}

impl PublishedIndexOverview {
    /// Returns the immutable snapshot underlying every projected value.
    #[must_use]
    pub const fn snapshot_id(&self) -> SnapshotId {
        self.snapshot_id
    }

    /// Returns all exact file revisions in the published graph.
    #[must_use]
    pub const fn file_count(&self) -> u64 {
        self.file_count
    }

    /// Returns all structural symbols in the published graph.
    #[must_use]
    pub const fn symbol_count(&self) -> u64 {
        self.symbol_count
    }

    /// Returns all safe parser diagnostics across published files.
    #[must_use]
    pub const fn diagnostic_count(&self) -> u64 {
        self.diagnostic_count
    }

    /// Returns files that have structural parser evidence rather than Generic fallback.
    #[must_use]
    pub const fn parsed_file_count(&self) -> u64 {
        self.parsed_file_count
    }

    /// Returns byte-weighted structural coverage, or None when no file was parsed structurally.
    #[must_use]
    pub const fn coverage_basis_points(&self) -> Option<u16> {
        self.coverage_basis_points
    }

    /// Returns the total number of files that contain at least one diagnostic.
    #[must_use]
    pub const fn diagnostic_file_count(&self) -> u64 {
        self.diagnostic_file_count
    }

    /// Returns at most 64 canonical file-local diagnostic projections.
    #[must_use]
    pub fn diagnostic_files(&self) -> &[PublishedFileDiagnostics] {
        &self.diagnostic_files
    }

    /// Returns whether additional diagnostic files were omitted from this UI projection.
    #[must_use]
    pub const fn diagnostic_files_truncated(&self) -> bool {
        self.diagnostic_files_truncated
    }
}

/// Non-authoritative display projection and bounded diagnostics for one exact file revision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublishedFileDiagnostics {
    path_display: RepositoryPathDisplay,
    language: IndexLanguage,
    coverage_basis_points: Option<u16>,
    diagnostic_count: u64,
    diagnostics: Vec<PublishedDiagnostic>,
    diagnostics_truncated: bool,
}

impl PublishedFileDiagnostics {
    /// Returns the bounded, sanitized repository-relative display text.
    #[must_use]
    pub const fn path_display(&self) -> &RepositoryPathDisplay {
        &self.path_display
    }

    /// Returns the parser language or Generic fallback.
    #[must_use]
    pub const fn language(&self) -> IndexLanguage {
        self.language
    }

    /// Returns structural coverage for this file, when a parser handled it.
    #[must_use]
    pub const fn coverage_basis_points(&self) -> Option<u16> {
        self.coverage_basis_points
    }

    /// Returns the complete diagnostic count for this file.
    #[must_use]
    pub const fn diagnostic_count(&self) -> u64 {
        self.diagnostic_count
    }

    /// Returns at most eight safe diagnostic summaries.
    #[must_use]
    pub fn diagnostics(&self) -> &[PublishedDiagnostic] {
        &self.diagnostics
    }

    /// Returns whether additional diagnostics for this file were omitted.
    #[must_use]
    pub const fn diagnostics_truncated(&self) -> bool {
        self.diagnostics_truncated
    }
}

/// Bounded, non-authoritative repository path intended only for display.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepositoryPathDisplay {
    value: String,
    truncated: bool,
}

impl RepositoryPathDisplay {
    /// Returns sanitized display text that is never accepted back as an authoritative path.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.value
    }

    /// Returns whether the source path exceeded the fixed 512-character UI budget.
    #[must_use]
    pub const fn is_truncated(&self) -> bool {
        self.truncated
    }
}

/// Safe parser diagnostic without source contents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublishedDiagnostic {
    code: ParseDiagnosticCode,
    severity: ParseDiagnosticSeverity,
    message: String,
    start_byte: u32,
    end_byte: u32,
}

impl PublishedDiagnostic {
    /// Returns the stable parser-independent category.
    #[must_use]
    pub const fn code(&self) -> ParseDiagnosticCode {
        self.code
    }

    /// Returns user-visible diagnostic severity.
    #[must_use]
    pub const fn severity(&self) -> ParseDiagnosticSeverity {
        self.severity
    }

    /// Returns the already validated single-line safe message.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Returns the inclusive start byte in the exact published file revision.
    #[must_use]
    pub const fn start_byte(&self) -> u32 {
        self.start_byte
    }

    /// Returns the exclusive end byte in the exact published file revision.
    #[must_use]
    pub const fn end_byte(&self) -> u32 {
        self.end_byte
    }
}

fn build_overview(
    published: &PublishedIndex,
) -> Result<PublishedIndexOverview, GetPublishedIndexOverviewError> {
    let publication = published.publication();
    let file_count = count(publication.graph().files().len())?;
    let symbol_count = count(publication.graph().symbols().len())?;
    let mut diagnostic_count = 0_u64;
    let mut diagnostic_file_count = 0_u64;
    let mut parsed_file_count = 0_u64;
    let mut total_bytes = 0_u64;
    let mut covered_bytes = 0_u64;
    let mut diagnostic_files = Vec::new();

    for analysis in publication.file_analyses() {
        if let Some(coverage) = analysis.coverage() {
            parsed_file_count = parsed_file_count
                .checked_add(1)
                .ok_or(GetPublishedIndexOverviewError::ProjectionTooLarge)?;
            total_bytes = total_bytes
                .checked_add(u64::from(coverage.total_bytes()))
                .ok_or(GetPublishedIndexOverviewError::ProjectionTooLarge)?;
            covered_bytes = covered_bytes
                .checked_add(u64::from(coverage.covered_bytes()))
                .ok_or(GetPublishedIndexOverviewError::ProjectionTooLarge)?;
        }
        let file_diagnostic_count = count(analysis.diagnostics().len())?;
        diagnostic_count = diagnostic_count
            .checked_add(file_diagnostic_count)
            .ok_or(GetPublishedIndexOverviewError::ProjectionTooLarge)?;
        if analysis.diagnostics().is_empty() {
            continue;
        }
        diagnostic_file_count = diagnostic_file_count
            .checked_add(1)
            .ok_or(GetPublishedIndexOverviewError::ProjectionTooLarge)?;
        if diagnostic_files.len() >= MAX_VISIBLE_DIAGNOSTIC_FILES {
            continue;
        }
        diagnostic_files.push(PublishedFileDiagnostics {
            path_display: path_display(analysis.revision().path()),
            language: analysis.language(),
            coverage_basis_points: analysis.coverage().map(|coverage| coverage.basis_points()),
            diagnostic_count: file_diagnostic_count,
            diagnostics: analysis
                .diagnostics()
                .iter()
                .take(MAX_VISIBLE_DIAGNOSTICS_PER_FILE)
                .map(|diagnostic| PublishedDiagnostic {
                    code: diagnostic.code(),
                    severity: diagnostic.severity(),
                    message: diagnostic.message().as_str().to_owned(),
                    start_byte: diagnostic.range().start_byte(),
                    end_byte: diagnostic.range().end_byte(),
                })
                .collect(),
            diagnostics_truncated: analysis.diagnostics().len() > MAX_VISIBLE_DIAGNOSTICS_PER_FILE,
        });
    }

    let visible_diagnostic_files = count(diagnostic_files.len())?;
    Ok(PublishedIndexOverview {
        snapshot_id: published.run().snapshot_id(),
        file_count,
        symbol_count,
        diagnostic_count,
        parsed_file_count,
        coverage_basis_points: aggregate_coverage(parsed_file_count, total_bytes, covered_bytes)?,
        diagnostic_file_count,
        diagnostic_files,
        diagnostic_files_truncated: diagnostic_file_count > visible_diagnostic_files,
    })
}

fn aggregate_coverage(
    parsed_file_count: u64,
    total_bytes: u64,
    covered_bytes: u64,
) -> Result<Option<u16>, GetPublishedIndexOverviewError> {
    if parsed_file_count == 0 {
        return Ok(None);
    }
    if total_bytes == 0 {
        return Ok(Some(10_000));
    }
    let scaled = covered_bytes
        .checked_mul(10_000)
        .ok_or(GetPublishedIndexOverviewError::ProjectionTooLarge)?;
    u16::try_from(scaled / total_bytes)
        .map(Some)
        .map_err(|_| GetPublishedIndexOverviewError::ProjectionTooLarge)
}

fn path_display(path: &RepositoryPath) -> RepositoryPathDisplay {
    let source = String::from_utf8_lossy(path.as_bytes());
    let mut characters = source.chars();
    let value = characters
        .by_ref()
        .take(MAX_PATH_DISPLAY_CHARS)
        .map(|character| {
            if character.is_control() {
                '\u{fffd}'
            } else {
                character
            }
        })
        .collect();
    RepositoryPathDisplay {
        value,
        truncated: characters.next().is_some(),
    }
}

fn count(value: usize) -> Result<u64, GetPublishedIndexOverviewError> {
    u64::try_from(value).map_err(|_| GetPublishedIndexOverviewError::ProjectionTooLarge)
}

/// Failure while reading or bounding the published-index projection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GetPublishedIndexOverviewError {
    /// The published-index persistence boundary could not be read safely.
    Storage(KnowledgeIndexFailure),
    /// Aggregate counts exceeded the fixed portable projection representation.
    ProjectionTooLarge,
}

impl fmt::Display for GetPublishedIndexOverviewError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Storage(error) => write!(formatter, "published index overview failed: {error}"),
            Self::ProjectionTooLarge => {
                formatter.write_str("published index overview exceeds its fixed bounds")
            }
        }
    }
}

impl Error for GetPublishedIndexOverviewError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Storage(error) => Some(error),
            Self::ProjectionTooLarge => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{aggregate_coverage, build_overview, path_display};
    use a3_domain::{
        ContentHash, DiagnosticMessage, FileRevision, IndexLanguage, IndexPublication, IndexRunId,
        IndexRunRecord, IndexRunSequence, IndexRunStatus, IndexedFileAnalysis,
        LanguageAdapterRevision, LanguageAdapterVersion, LinkedGraph, ModulePolicyVersion,
        ModuleProjection, ModuleSymbolSet, ParseCoverage, ParseDiagnostic, ParseDiagnosticCode,
        ParseDiagnosticSeverity, PublishedIndex, RankProjection, RankingPolicyVersion,
        RepositoryCard, RepositoryPath, SnapshotId, SourcePosition, SourceRange,
    };
    use std::error::Error;

    #[test]
    fn path_display_is_bounded_sanitized_and_non_authoritative() -> Result<(), Box<dyn Error>> {
        let path =
            RepositoryPath::try_from_bytes(format!("src/\n{}", "a".repeat(600)).into_bytes())?;

        let display = path_display(&path);

        assert_eq!(display.as_str().chars().count(), 512);
        assert!(!display.as_str().chars().any(char::is_control));
        assert!(display.is_truncated());
        Ok(())
    }

    #[test]
    fn coverage_is_byte_weighted_and_absent_without_structural_parses() -> Result<(), Box<dyn Error>>
    {
        assert_eq!(aggregate_coverage(0, 0, 0)?, None);
        assert_eq!(aggregate_coverage(1, 0, 0)?, Some(10_000));
        assert_eq!(aggregate_coverage(2, 10, 7)?, Some(7_000));
        Ok(())
    }

    #[test]
    fn overview_projects_real_publication_counts_coverage_and_file_error()
    -> Result<(), Box<dyn Error>> {
        let snapshot_id = SnapshotId::from_bytes([1; 32]);
        let revision = FileRevision::new(
            RepositoryPath::try_from_bytes(b"src/lib.rs".to_vec())?,
            ContentHash::from_bytes([2; 32]),
        );
        let graph = LinkedGraph::new(
            snapshot_id,
            vec![revision.clone()],
            Vec::new(),
            Vec::new(),
            Vec::new(),
        )?;
        let ranking = RankProjection::new(snapshot_id, RankingPolicyVersion::v1(), Vec::new())?;
        let repository_card = RepositoryCard::new(
            snapshot_id,
            ModulePolicyVersion::v1(),
            Vec::new(),
            vec![IndexLanguage::Rust],
            ModuleSymbolSet::empty(),
            1,
            0,
        )?;
        let modules = ModuleProjection::new(
            snapshot_id,
            ModulePolicyVersion::v1(),
            Vec::new(),
            Vec::new(),
            repository_card,
        )?;
        let diagnostic = ParseDiagnostic::new(
            ParseDiagnosticCode::SyntaxError,
            ParseDiagnosticSeverity::Error,
            SourceRange::new(8, 10, SourcePosition::new(0, 8), SourcePosition::new(0, 10))?,
            DiagnosticMessage::try_from_string("syntax error".to_owned())?,
        );
        let analysis = IndexedFileAnalysis::parsed(
            revision,
            LanguageAdapterRevision::new(
                IndexLanguage::Rust,
                LanguageAdapterVersion::try_from_string("tree-sitter-rust-1".to_owned())?,
            ),
            ParseCoverage::new(10, 8, 1)?,
            vec![diagnostic],
        )?;
        let publication = IndexPublication::new_with_file_analyses(
            graph,
            ranking,
            Vec::new(),
            modules,
            vec![analysis],
        )?;
        let run = IndexRunRecord::new(
            IndexRunId::from_bytes([3; 32]),
            snapshot_id,
            RankingPolicyVersion::v1(),
            IndexRunSequence::new(1)?,
            IndexRunStatus::Published,
        );
        let published = PublishedIndex::new(run, publication)?;

        let overview = build_overview(&published)?;

        assert_eq!(overview.file_count(), 1);
        assert_eq!(overview.symbol_count(), 0);
        assert_eq!(overview.diagnostic_count(), 1);
        assert_eq!(overview.coverage_basis_points(), Some(8_000));
        assert_eq!(overview.diagnostic_files().len(), 1);
        assert_eq!(overview.diagnostic_files()[0].diagnostics().len(), 1);
        Ok(())
    }
}
