//! Reusable golden contracts for every A^3 structural language adapter.
//!
//! This dev-only crate depends only on application and domain contracts. Each
//! concrete adapter supplies a representative valid and invalid source fixture.

use a3_application::{
    LanguageAdapter, LanguageParseControl, LanguageParseControlError, LanguageParseFailure,
    LanguageParseInput, LanguageParsePolicy, SnapshotCompatibility,
};
use a3_domain::{
    ContentHash, DiscoveredFileRoles, FileRevision, IndexSchemaVersion, LanguageParseResult,
    Progress, RepositoryPath, SourceRange, SymbolRole,
};
use std::error::Error;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

/// Boxed error used by the shared language-adapter harness.
pub type ContractError = Box<dyn Error + Send + Sync + 'static>;

/// Result returned by the shared contract harness.
pub type ContractResult<T> = Result<T, ContractError>;

/// Representative source files and the exact normalized valid result.
#[derive(Debug, Clone, Copy)]
pub struct LanguageAdapterContractFixture {
    /// Repository path recognized by the adapter.
    pub supported_path: &'static [u8],
    /// Repository path the adapter must reject.
    pub unsupported_path: &'static [u8],
    /// Fully valid source with deterministic structural output.
    pub valid_source: &'static [u8],
    /// Invalid syntax which must produce partial output instead of a parse failure.
    pub invalid_source: &'static [u8],
    /// Canonical rendering of the valid parse output.
    pub expected_golden: &'static str,
}

/// Runs the common V1 contract against one concrete language adapter.
pub fn verify_language_adapter_contract(
    adapter: &dyn LanguageAdapter,
    fixture: LanguageAdapterContractFixture,
) -> ContractResult<()> {
    let policy = LanguageParsePolicy::v1();
    assert_eq!(adapter.contract_version(), policy.contract_version());

    let supported_path = RepositoryPath::try_from_bytes(fixture.supported_path.to_vec())?;
    let unsupported_path = RepositoryPath::try_from_bytes(fixture.unsupported_path.to_vec())?;
    assert!(adapter.supports_path(&supported_path));
    assert!(!adapter.supports_path(&unsupported_path));

    let valid_revision = revision(supported_path.clone(), fixture.valid_source);
    let valid_control = RecordingControl::default();
    let valid = adapter.parse(
        LanguageParseInput::new(
            &valid_revision,
            fixture.valid_source,
            DiscoveredFileRoles::empty(),
        ),
        policy,
        &valid_control,
    )?;
    assert_eq!(valid.revision(), &valid_revision);
    assert_eq!(valid.adapter_revision(), adapter.revision());
    assert_eq!(valid.contract_version(), policy.contract_version());
    assert!(valid.coverage().is_complete());
    assert_eq!(
        render_language_parse_result(&valid),
        fixture.expected_golden
    );
    verify_progress(&valid_control, policy)?;

    let repeated = adapter.parse(
        LanguageParseInput::new(
            &valid_revision,
            fixture.valid_source,
            DiscoveredFileRoles::empty(),
        ),
        policy,
        &RecordingControl::default(),
    )?;
    assert_eq!(repeated, valid);

    let invalid_revision = revision(supported_path.clone(), fixture.invalid_source);
    let invalid = adapter.parse(
        LanguageParseInput::new(
            &invalid_revision,
            fixture.invalid_source,
            DiscoveredFileRoles::empty(),
        ),
        policy,
        &RecordingControl::default(),
    )?;
    assert!(!invalid.coverage().is_complete());
    assert!(!invalid.diagnostics().is_empty());

    let after_invalid = adapter.parse(
        LanguageParseInput::new(
            &valid_revision,
            fixture.valid_source,
            DiscoveredFileRoles::empty(),
        ),
        policy,
        &RecordingControl::default(),
    )?;
    assert_eq!(after_invalid, valid);

    let cancelled = RecordingControl::cancelled();
    assert_eq!(
        adapter.parse(
            LanguageParseInput::new(
                &valid_revision,
                fixture.valid_source,
                DiscoveredFileRoles::empty(),
            ),
            policy,
            &cancelled,
        ),
        Err(LanguageParseFailure::Cancelled)
    );

    let oversized_source = vec![b' '; policy.max_source_bytes().saturating_add(1)];
    let oversized_revision = revision(supported_path.clone(), &oversized_source);
    assert_eq!(
        adapter.parse(
            LanguageParseInput::new(
                &oversized_revision,
                &oversized_source,
                DiscoveredFileRoles::empty(),
            ),
            policy,
            &RecordingControl::default(),
        ),
        Err(LanguageParseFailure::InputTooLarge)
    );

    let mismatched_revision = FileRevision::new(supported_path, ContentHash::from_bytes([0; 32]));
    assert_eq!(
        adapter.parse(
            LanguageParseInput::new(
                &mismatched_revision,
                fixture.valid_source,
                DiscoveredFileRoles::empty(),
            ),
            policy,
            &RecordingControl::default(),
        ),
        Err(LanguageParseFailure::RevisionMismatch)
    );

    let unsupported_revision = revision(unsupported_path, fixture.valid_source);
    assert_eq!(
        adapter.parse(
            LanguageParseInput::new(
                &unsupported_revision,
                fixture.valid_source,
                DiscoveredFileRoles::empty(),
            ),
            policy,
            &RecordingControl::default(),
        ),
        Err(LanguageParseFailure::UnsupportedPath)
    );

    let compatibility = SnapshotCompatibility::new(
        IndexSchemaVersion::new(1)?,
        vec![adapter.revision().clone()],
    )?;
    assert_eq!(
        compatibility.adapter_revisions(),
        [adapter.revision().clone()]
    );
    Ok(())
}

/// Produces a stable, exhaustive-enough textual golden for adapter outputs.
#[must_use]
pub fn render_language_parse_result(result: &LanguageParseResult) -> String {
    let coverage = result.coverage();
    let mut rendered = format!(
        "path={} hash={} language={} adapter={} contract={} coverage={}/{}/{}\n",
        hex(result.revision().path().as_bytes()),
        hex(result.revision().content_hash().as_bytes()),
        result.language().as_str(),
        result.adapter_revision().version().as_str(),
        result.contract_version().get(),
        coverage.covered_bytes(),
        coverage.total_bytes(),
        coverage.incomplete_regions(),
    );
    for symbol in result.symbols() {
        rendered.push_str(&format!(
            "symbol id={} kind={:?} name={:?} signature={:?} declaration={} selection={} documentation={} visibility={:?} test={} entrypoint={}\n",
            symbol.id().get(),
            symbol.kind(),
            symbol.name().as_str(),
            symbol.signature().map(|value| value.as_str()),
            render_range(symbol.declaration_range()),
            render_range(symbol.selection_range()),
            symbol
                .documentation_range()
                .map_or_else(|| "-".to_owned(), render_range),
            symbol.visibility(),
            symbol.roles().contains(SymbolRole::Test),
            symbol.roles().contains(SymbolRole::Entrypoint),
        ));
    }
    for relation in result.relations() {
        rendered.push_str(&format!(
            "relation source={:?} target={:?} kind={:?} provider={:?} confidence={} evidence={}\n",
            relation.source(),
            relation.target(),
            relation.kind(),
            relation.provider(),
            relation.confidence().basis_points(),
            render_range(relation.evidence_range()),
        ));
    }
    for diagnostic in result.diagnostics() {
        rendered.push_str(&format!(
            "diagnostic code={:?} severity={:?} range={} message={:?}\n",
            diagnostic.code(),
            diagnostic.severity(),
            render_range(diagnostic.range()),
            diagnostic.message().as_str(),
        ));
    }
    rendered
}

fn revision(path: RepositoryPath, source: &[u8]) -> FileRevision {
    FileRevision::new(
        path,
        ContentHash::from_bytes(*blake3::hash(source).as_bytes()),
    )
}

fn render_range(range: SourceRange) -> String {
    format!(
        "{}..{}@{}:{}..{}:{}",
        range.start_byte(),
        range.end_byte(),
        range.start_position().row(),
        range.start_position().column(),
        range.end_position().row(),
        range.end_position().column(),
    )
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len().saturating_mul(2));
    for byte in bytes {
        output.push(char::from(DIGITS[usize::from(byte >> 4)]));
        output.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
    }
    output
}

#[derive(Debug, Default)]
struct RecordingControl {
    cancelled: AtomicBool,
    progress: Mutex<Vec<Progress>>,
}

impl RecordingControl {
    fn cancelled() -> Self {
        Self {
            cancelled: AtomicBool::new(true),
            progress: Mutex::new(Vec::new()),
        }
    }

    fn progress(&self) -> ContractResult<Vec<Progress>> {
        self.progress
            .lock()
            .map(|values| values.clone())
            .map_err(|_| ContractHarnessError.into())
    }
}

impl LanguageParseControl for RecordingControl {
    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }

    fn report_progress(&self, progress: Progress) -> Result<(), LanguageParseControlError> {
        let mut values = self
            .progress
            .lock()
            .map_err(|_| LanguageParseControlError::Unavailable)?;
        if let Some(previous) = values.last() {
            progress
                .validate_after(*previous)
                .map_err(|_| LanguageParseControlError::Unavailable)?;
        }
        values.push(progress);
        Ok(())
    }
}

fn verify_progress(control: &RecordingControl, policy: LanguageParsePolicy) -> ContractResult<()> {
    let progress = control.progress()?;
    assert!(!progress.is_empty());
    assert!(progress.len() <= policy.max_progress_events());
    assert_eq!(
        progress.first().and_then(|value| value.completed()),
        Some(0)
    );
    assert_eq!(progress.last().map(|value| value.is_complete()), Some(true));
    Ok(())
}

#[derive(Debug, Clone, Copy)]
struct ContractHarnessError;

impl std::fmt::Display for ContractHarnessError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("language-adapter contract control was poisoned")
    }
}

impl Error for ContractHarnessError {}
