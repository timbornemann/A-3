use super::{
    FileRevision, IndexLanguage, LanguageAdapterRevision, LanguageParseResult, ParseCoverage,
    ParseDiagnostic,
};
use std::error::Error;
use std::fmt;

/// Published structural-analysis evidence for one exact repository file revision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexedFileAnalysis {
    revision: FileRevision,
    adapter_revision: Option<LanguageAdapterRevision>,
    coverage: Option<ParseCoverage>,
    diagnostics: Vec<ParseDiagnostic>,
}

impl IndexedFileAnalysis {
    /// Creates the explicit no-structural-adapter projection for a generic text file.
    #[must_use]
    pub const fn generic(revision: FileRevision) -> Self {
        Self {
            revision,
            adapter_revision: None,
            coverage: None,
            diagnostics: Vec::new(),
        }
    }

    /// Projects the bounded analysis evidence already validated by a language adapter.
    #[must_use]
    pub fn from_parse(parse: &LanguageParseResult) -> Self {
        Self {
            revision: parse.revision().clone(),
            adapter_revision: Some(parse.adapter_revision().clone()),
            coverage: Some(parse.coverage()),
            diagnostics: parse.diagnostics().to_vec(),
        }
    }

    /// Reconstructs persisted parsed-file evidence while rechecking all cross-field invariants.
    pub fn parsed(
        revision: FileRevision,
        adapter_revision: LanguageAdapterRevision,
        coverage: ParseCoverage,
        mut diagnostics: Vec<ParseDiagnostic>,
    ) -> Result<Self, IndexedFileAnalysisError> {
        if adapter_revision.language() == IndexLanguage::Generic {
            return Err(IndexedFileAnalysisError::GenericAdapter);
        }
        diagnostics.sort();
        if diagnostics.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(IndexedFileAnalysisError::DuplicateDiagnostic);
        }
        if diagnostics
            .iter()
            .any(|diagnostic| diagnostic.range().end_byte() > coverage.total_bytes())
        {
            return Err(IndexedFileAnalysisError::DiagnosticOutsideSource);
        }
        if coverage.is_complete() && !diagnostics.is_empty() {
            return Err(IndexedFileAnalysisError::DiagnosticsWithCompleteCoverage);
        }
        Ok(Self {
            revision,
            adapter_revision: Some(adapter_revision),
            coverage: Some(coverage),
            diagnostics,
        })
    }

    /// Returns the exact published file revision.
    #[must_use]
    pub const fn revision(&self) -> &FileRevision {
        &self.revision
    }

    /// Returns the structural language, or Generic when no adapter parsed the file.
    #[must_use]
    pub const fn language(&self) -> IndexLanguage {
        match &self.adapter_revision {
            Some(revision) => revision.language(),
            None => IndexLanguage::Generic,
        }
    }

    /// Returns the exact adapter/grammar revision for structurally parsed files.
    #[must_use]
    pub const fn adapter_revision(&self) -> Option<&LanguageAdapterRevision> {
        self.adapter_revision.as_ref()
    }

    /// Returns structural byte coverage, or None for generic files.
    #[must_use]
    pub const fn coverage(&self) -> Option<ParseCoverage> {
        self.coverage
    }

    /// Returns canonical safe per-file diagnostics.
    #[must_use]
    pub fn diagnostics(&self) -> &[ParseDiagnostic] {
        &self.diagnostics
    }
}

/// Invalid persisted relationship in one published file-analysis projection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndexedFileAnalysisError {
    /// Generic files cannot claim a structural parser result.
    GenericAdapter,
    /// The same diagnostic appeared more than once.
    DuplicateDiagnostic,
    /// A diagnostic range exceeded the covered source length.
    DiagnosticOutsideSource,
    /// Complete coverage contradicted a non-empty diagnostic set.
    DiagnosticsWithCompleteCoverage,
}

impl fmt::Display for IndexedFileAnalysisError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::GenericAdapter => "generic file analysis cannot claim a structural adapter",
            Self::DuplicateDiagnostic => "file analysis contains a duplicate diagnostic",
            Self::DiagnosticOutsideSource => "file diagnostic is outside the analyzed source",
            Self::DiagnosticsWithCompleteCoverage => {
                "complete file analysis cannot contain diagnostics"
            }
        })
    }
}

impl Error for IndexedFileAnalysisError {}

#[cfg(test)]
mod tests {
    use super::{IndexedFileAnalysis, IndexedFileAnalysisError};
    use crate::{
        ContentHash, DiagnosticMessage, FileRevision, IndexLanguage, LanguageAdapterRevision,
        LanguageAdapterVersion, ParseCoverage, ParseDiagnostic, ParseDiagnosticCode,
        ParseDiagnosticSeverity, RepositoryPath, SourcePosition, SourceRange,
    };
    use std::error::Error;

    #[test]
    fn parsed_analysis_rejects_false_complete_coverage() -> Result<(), Box<dyn Error>> {
        let diagnostic = ParseDiagnostic::new(
            ParseDiagnosticCode::SyntaxError,
            ParseDiagnosticSeverity::Error,
            SourceRange::new(0, 1, SourcePosition::new(0, 0), SourcePosition::new(0, 1))?,
            DiagnosticMessage::try_from_string("syntax error".to_owned())?,
        );

        assert_eq!(
            IndexedFileAnalysis::parsed(
                revision()?,
                adapter()?,
                ParseCoverage::complete(1)?,
                vec![diagnostic]
            ),
            Err(IndexedFileAnalysisError::DiagnosticsWithCompleteCoverage)
        );
        Ok(())
    }

    fn revision() -> Result<FileRevision, Box<dyn Error>> {
        Ok(FileRevision::new(
            RepositoryPath::try_from_bytes(b"src/lib.rs".to_vec())?,
            ContentHash::from_bytes([1; 32]),
        ))
    }

    fn adapter() -> Result<LanguageAdapterRevision, Box<dyn Error>> {
        Ok(LanguageAdapterRevision::new(
            IndexLanguage::Rust,
            LanguageAdapterVersion::try_from_string("rust-tree-sitter-v1".to_owned())?,
        ))
    }
}
