mod diagnostic;
mod relation;
mod result;
mod source;
mod symbol;

pub use diagnostic::{
    DiagnosticMessage, DiagnosticMessageError, ParseDiagnostic, ParseDiagnosticCode,
    ParseDiagnosticSeverity,
};
pub use relation::{
    Confidence, ConfidenceError, SymbolReference, SymbolReferenceError, SyntaxProvider,
    SyntaxRelation, SyntaxRelationKind, SyntaxSource, SyntaxTarget,
};
pub use result::{
    LanguageAdapterContractVersion, LanguageAdapterContractVersionError, LanguageParseArtifacts,
    LanguageParseResult, LanguageParseResultError, ParseCoverage, ParseCoverageError,
};
pub use source::{SourcePosition, SourceRange, SourceRangeError};
pub use symbol::{
    LocalSymbolId, LocalSymbolIdError, ParsedSymbol, ParsedSymbolError, SymbolKind, SymbolName,
    SymbolRole, SymbolRoles, SymbolSignature, SymbolTextError, SymbolVisibility,
};

#[cfg(test)]
mod tests {
    use super::{
        Confidence, DiagnosticMessage, LanguageAdapterContractVersion, LanguageParseArtifacts,
        LanguageParseResult, LanguageParseResultError, LocalSymbolId, ParseCoverage,
        ParseDiagnostic, ParseDiagnosticCode, ParseDiagnosticSeverity, ParsedSymbol,
        SourcePosition, SourceRange, SymbolKind, SymbolName, SymbolReference, SyntaxProvider,
        SyntaxRelation, SyntaxRelationKind, SyntaxSource, SyntaxTarget,
    };
    use crate::{
        ContentHash, FileRevision, IndexLanguage, LanguageAdapterRevision, LanguageAdapterVersion,
        RepositoryPath,
    };

    #[test]
    fn source_and_symbol_ranges_reject_inverted_or_uncontained_coordinates()
    -> Result<(), Box<dyn std::error::Error>> {
        let start = SourcePosition::new(0, 0);
        let end = SourcePosition::new(0, 5);
        assert!(SourceRange::new(5, 1, start, end).is_err());
        let declaration = SourceRange::new(0, 5, start, end)?;
        let outside = SourceRange::new(5, 6, end, SourcePosition::new(0, 6))?;
        assert!(
            ParsedSymbol::new(
                LocalSymbolId::new(1)?,
                SymbolKind::Function,
                SymbolName::try_from_string("f".to_owned())?,
                declaration,
                outside,
            )
            .is_err()
        );
        let inconsistent_position =
            SourceRange::new(1, 4, SourcePosition::new(1, 0), SourcePosition::new(1, 3))?;
        assert!(
            ParsedSymbol::new(
                LocalSymbolId::new(1)?,
                SymbolKind::Function,
                SymbolName::try_from_string("f".to_owned())?,
                declaration,
                inconsistent_position,
            )
            .is_err()
        );
        Ok(())
    }

    #[test]
    fn parse_result_canonicalizes_artifacts_and_rejects_dangling_symbols()
    -> Result<(), Box<dyn std::error::Error>> {
        let range = SourceRange::new(0, 4, SourcePosition::new(0, 0), SourcePosition::new(0, 4))?;
        let symbol = ParsedSymbol::new(
            LocalSymbolId::new(1)?,
            SymbolKind::Field,
            SymbolName::try_from_string("name".to_owned())?,
            range,
            range,
        )?;
        let artifacts = LanguageParseArtifacts {
            symbols: vec![symbol],
            relations: vec![SyntaxRelation::new(
                SyntaxSource::File,
                SyntaxTarget::Symbol(LocalSymbolId::new(1)?),
                SyntaxRelationKind::Defines,
                SyntaxProvider::TreeSitter,
                Confidence::certain(),
                range,
            )],
            diagnostics: Vec::new(),
        };
        let result = LanguageParseResult::new(
            revision()?,
            adapter_revision()?,
            LanguageAdapterContractVersion::v1(),
            ParseCoverage::complete(4)?,
            artifacts,
        )?;
        assert_eq!(result.symbols()[0].name().as_str(), "name");

        let dangling = LanguageParseArtifacts {
            symbols: Vec::new(),
            relations: vec![SyntaxRelation::new(
                SyntaxSource::File,
                SyntaxTarget::Symbol(LocalSymbolId::new(2)?),
                SyntaxRelationKind::Calls,
                SyntaxProvider::TreeSitter,
                Confidence::from_basis_points(7_500)?,
                range,
            )],
            diagnostics: Vec::new(),
        };
        assert_eq!(
            LanguageParseResult::new(
                revision()?,
                adapter_revision()?,
                LanguageAdapterContractVersion::v1(),
                ParseCoverage::complete(4)?,
                dangling,
            ),
            Err(LanguageParseResultError::UnknownSymbolReference)
        );
        Ok(())
    }

    #[test]
    fn partial_coverage_is_visible_and_complete_coverage_rejects_diagnostics()
    -> Result<(), Box<dyn std::error::Error>> {
        let range = SourceRange::new(1, 2, SourcePosition::new(0, 1), SourcePosition::new(0, 2))?;
        let diagnostic = ParseDiagnostic::new(
            ParseDiagnosticCode::SyntaxError,
            ParseDiagnosticSeverity::Error,
            range,
            DiagnosticMessage::try_from_string("syntax error".to_owned())?,
        );
        let artifacts = LanguageParseArtifacts {
            symbols: Vec::new(),
            relations: vec![SyntaxRelation::new(
                SyntaxSource::File,
                SyntaxTarget::Unresolved(SymbolReference::try_from_string("target".to_owned())?),
                SyntaxRelationKind::Imports,
                SyntaxProvider::TreeSitter,
                Confidence::certain(),
                range,
            )],
            diagnostics: vec![diagnostic.clone()],
        };
        let partial = LanguageParseResult::new(
            revision()?,
            adapter_revision()?,
            LanguageAdapterContractVersion::v1(),
            ParseCoverage::new(4, 3, 1)?,
            artifacts.clone(),
        )?;
        assert!(!partial.coverage().is_complete());
        assert_eq!(partial.coverage().basis_points(), 7_500);
        assert_eq!(
            LanguageParseResult::new(
                revision()?,
                adapter_revision()?,
                LanguageAdapterContractVersion::v1(),
                ParseCoverage::complete(4)?,
                artifacts,
            ),
            Err(LanguageParseResultError::DiagnosticsWithCompleteCoverage)
        );
        Ok(())
    }

    fn revision() -> Result<FileRevision, Box<dyn std::error::Error>> {
        Ok(FileRevision::new(
            RepositoryPath::try_from_bytes(b"fixture.json".to_vec())?,
            ContentHash::from_bytes([1; 32]),
        ))
    }

    fn adapter_revision() -> Result<LanguageAdapterRevision, Box<dyn std::error::Error>> {
        Ok(LanguageAdapterRevision::new(
            IndexLanguage::Generic,
            LanguageAdapterVersion::try_from_string("contract-probe-v1".to_owned())?,
        ))
    }
}
