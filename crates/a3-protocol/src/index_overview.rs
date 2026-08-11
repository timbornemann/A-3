use crate::ProtocolVersion;
use serde::{Deserialize, Serialize};

/// Strict pathless input for the bounded V1 published-index overview.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct QueryIndexOverviewRequestV1 {
    protocol_version: ProtocolVersion,
}

impl QueryIndexOverviewRequestV1 {
    /// Creates a request for a specific protocol version.
    #[must_use]
    pub const fn new(protocol_version: ProtocolVersion) -> Self {
        Self { protocol_version }
    }

    /// Creates a request for the protocol version emitted by this build.
    #[must_use]
    pub const fn current() -> Self {
        Self::new(ProtocolVersion::CURRENT)
    }

    /// Returns the requested protocol version.
    #[must_use]
    pub const fn protocol_version(self) -> ProtocolVersion {
        self.protocol_version
    }
}

/// Versioned bounded result selected from the Core-owned active project.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct IndexOverviewResponseV1 {
    protocol_version: ProtocolVersion,
    result: IndexOverviewResultV1,
}

impl IndexOverviewResponseV1 {
    /// Creates the response used before a project is active.
    #[must_use]
    pub const fn no_project() -> Self {
        Self {
            protocol_version: ProtocolVersion::CURRENT,
            result: IndexOverviewResultV1::NoProject,
        }
    }

    /// Creates the response used before the first atomic publication.
    #[must_use]
    pub const fn no_published_index() -> Self {
        Self {
            protocol_version: ProtocolVersion::CURRENT,
            result: IndexOverviewResultV1::NoPublishedIndex,
        }
    }

    /// Creates a response containing one bounded immutable publication projection.
    #[must_use]
    pub fn published(overview: IndexOverviewV1) -> Self {
        Self {
            protocol_version: ProtocolVersion::CURRENT,
            result: IndexOverviewResultV1::Published {
                overview: Box::new(overview),
            },
        }
    }

    /// Returns the mutually exclusive active-project/publication result.
    #[must_use]
    pub const fn result(&self) -> &IndexOverviewResultV1 {
        &self.result
    }
}

/// Whether an active project and complete published index exist.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase", tag = "status")]
pub enum IndexOverviewResultV1 {
    /// No project is active in this desktop process.
    NoProject,
    /// A project is active but no index has crossed the atomic publish boundary.
    NoPublishedIndex,
    /// The latest complete publication is available.
    Published {
        /// Bounded counts, coverage, and diagnostic-file summaries.
        overview: Box<IndexOverviewV1>,
    },
}

/// Bounded WebView-safe view of one published index snapshot.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct IndexOverviewV1 {
    snapshot_id: String,
    counts: IndexOverviewCountsV1,
    coverage_basis_points: Option<u16>,
    diagnostic_files: Vec<IndexFileDiagnosticsV1>,
    diagnostic_files_truncated: bool,
}

impl IndexOverviewV1 {
    /// Creates a projection from application-validated bounded values.
    #[must_use]
    pub const fn new(
        snapshot_id: String,
        counts: IndexOverviewCountsV1,
        coverage_basis_points: Option<u16>,
        diagnostic_files: Vec<IndexFileDiagnosticsV1>,
        diagnostic_files_truncated: bool,
    ) -> Self {
        Self {
            snapshot_id,
            counts,
            coverage_basis_points,
            diagnostic_files,
            diagnostic_files_truncated,
        }
    }

    /// Returns the immutable publication snapshot digest.
    #[must_use]
    pub fn snapshot_id(&self) -> &str {
        &self.snapshot_id
    }

    /// Returns exact aggregate counts encoded as lossless decimal text.
    #[must_use]
    pub const fn counts(&self) -> &IndexOverviewCountsV1 {
        &self.counts
    }

    /// Returns byte-weighted structural coverage, if any file was structurally parsed.
    #[must_use]
    pub const fn coverage_basis_points(&self) -> Option<u16> {
        self.coverage_basis_points
    }

    /// Returns at most 64 canonical files that contain parser diagnostics.
    #[must_use]
    pub fn diagnostic_files(&self) -> &[IndexFileDiagnosticsV1] {
        &self.diagnostic_files
    }

    /// Returns whether further diagnostic files were omitted.
    #[must_use]
    pub const fn diagnostic_files_truncated(&self) -> bool {
        self.diagnostic_files_truncated
    }
}

/// Exact publication counters represented without JavaScript integer loss.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct IndexOverviewCountsV1 {
    file_count: String,
    symbol_count: String,
    diagnostic_count: String,
    parsed_file_count: String,
    diagnostic_file_count: String,
}

impl IndexOverviewCountsV1 {
    /// Creates a group of canonical decimal counters.
    #[must_use]
    pub const fn new(
        file_count: String,
        symbol_count: String,
        diagnostic_count: String,
        parsed_file_count: String,
        diagnostic_file_count: String,
    ) -> Self {
        Self {
            file_count,
            symbol_count,
            diagnostic_count,
            parsed_file_count,
            diagnostic_file_count,
        }
    }

    /// Returns the complete published file count.
    #[must_use]
    pub fn file_count(&self) -> &str {
        &self.file_count
    }

    /// Returns the complete published symbol count.
    #[must_use]
    pub fn symbol_count(&self) -> &str {
        &self.symbol_count
    }

    /// Returns the complete published parser-diagnostic count.
    #[must_use]
    pub fn diagnostic_count(&self) -> &str {
        &self.diagnostic_count
    }

    /// Returns the number of structurally parsed files.
    #[must_use]
    pub fn parsed_file_count(&self) -> &str {
        &self.parsed_file_count
    }

    /// Returns the number of files containing at least one diagnostic.
    #[must_use]
    pub fn diagnostic_file_count(&self) -> &str {
        &self.diagnostic_file_count
    }
}

/// Safe file-local parser result surfaced without an authoritative path capability.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct IndexFileDiagnosticsV1 {
    path_display: String,
    path_display_truncated: bool,
    language: IndexLanguageV1,
    coverage_basis_points: Option<u16>,
    diagnostic_count: String,
    diagnostics: Vec<IndexDiagnosticV1>,
    diagnostics_truncated: bool,
}

impl IndexFileDiagnosticsV1 {
    /// Creates one bounded file display and its safe diagnostic subset.
    #[must_use]
    pub const fn new(
        path_display: String,
        path_display_truncated: bool,
        language: IndexLanguageV1,
        coverage_basis_points: Option<u16>,
        diagnostic_count: String,
        diagnostics: Vec<IndexDiagnosticV1>,
        diagnostics_truncated: bool,
    ) -> Self {
        Self {
            path_display,
            path_display_truncated,
            language,
            coverage_basis_points,
            diagnostic_count,
            diagnostics,
            diagnostics_truncated,
        }
    }
}

/// Stable language names understood by the V1 index UI.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum IndexLanguageV1 {
    /// No structural parser handled this file.
    Generic,
    /// Rust source parsed by the Rust adapter.
    Rust,
    /// TypeScript or JavaScript source parsed by the shared adapter.
    TypeScriptJavaScript,
    /// Python source parsed by the Python adapter.
    Python,
}

/// Safe parser diagnostic without source text.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct IndexDiagnosticV1 {
    code: IndexDiagnosticCodeV1,
    severity: IndexDiagnosticSeverityV1,
    message: String,
    start_byte: u32,
    end_byte: u32,
}

impl IndexDiagnosticV1 {
    /// Creates a diagnostic from already validated application evidence.
    #[must_use]
    pub const fn new(
        code: IndexDiagnosticCodeV1,
        severity: IndexDiagnosticSeverityV1,
        message: String,
        start_byte: u32,
        end_byte: u32,
    ) -> Self {
        Self {
            code,
            severity,
            message,
            start_byte,
            end_byte,
        }
    }
}

/// Stable parser-independent diagnostic code.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum IndexDiagnosticCodeV1 {
    /// Parser emitted an explicit error node.
    SyntaxError,
    /// Parser inserted missing syntax.
    MissingSyntax,
    /// Required source text was not valid UTF-8.
    InvalidEncoding,
    /// Syntax exceeded the adapter's structural contract.
    UnsupportedSyntax,
    /// A bounded parser collection omitted additional artifacts.
    OutputTruncated,
}

/// User-visible parser-diagnostic severity.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum IndexDiagnosticSeverityV1 {
    /// Structural output is incomplete for this region.
    Error,
    /// Output remains useful with reduced precision.
    Warning,
    /// Informational parser observation.
    Information,
}

#[cfg(test)]
mod tests {
    use super::{
        IndexDiagnosticCodeV1, IndexDiagnosticSeverityV1, IndexDiagnosticV1,
        IndexFileDiagnosticsV1, IndexLanguageV1, IndexOverviewCountsV1, IndexOverviewResponseV1,
        IndexOverviewV1,
    };
    use serde_json::json;

    #[test]
    fn published_overview_has_a_strict_bounded_shape() -> Result<(), serde_json::Error> {
        let response = IndexOverviewResponseV1::published(IndexOverviewV1::new(
            "11".repeat(32),
            IndexOverviewCountsV1::new(
                "2".to_owned(),
                "3".to_owned(),
                "1".to_owned(),
                "1".to_owned(),
                "1".to_owned(),
            ),
            Some(8_000),
            vec![IndexFileDiagnosticsV1::new(
                "src/lib.rs".to_owned(),
                false,
                IndexLanguageV1::Rust,
                Some(8_000),
                "1".to_owned(),
                vec![IndexDiagnosticV1::new(
                    IndexDiagnosticCodeV1::SyntaxError,
                    IndexDiagnosticSeverityV1::Error,
                    "syntax error".to_owned(),
                    8,
                    10,
                )],
                false,
            )],
            false,
        ));

        assert_eq!(
            serde_json::to_value(response)?,
            json!({
                "protocolVersion": 1,
                "result": {
                    "status": "published",
                    "overview": {
                        "snapshotId": "11".repeat(32),
                        "counts": {
                            "fileCount": "2",
                            "symbolCount": "3",
                            "diagnosticCount": "1",
                            "parsedFileCount": "1",
                            "diagnosticFileCount": "1"
                        },
                        "coverageBasisPoints": 8000,
                        "diagnosticFiles": [{
                            "pathDisplay": "src/lib.rs",
                            "pathDisplayTruncated": false,
                            "language": "rust",
                            "coverageBasisPoints": 8000,
                            "diagnosticCount": "1",
                            "diagnostics": [{
                                "code": "syntaxError",
                                "severity": "error",
                                "message": "syntax error",
                                "startByte": 8,
                                "endByte": 10
                            }],
                            "diagnosticsTruncated": false
                        }],
                        "diagnosticFilesTruncated": false
                    }
                }
            })
        );
        Ok(())
    }
}
