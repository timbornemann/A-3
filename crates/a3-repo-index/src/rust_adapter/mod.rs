mod manifest;
mod source;
mod syntax;

use crate::{ParserPoolCreateError, ParserPoolSize, TreeSitterParserPool};
use a3_application::{
    LanguageAdapter, LanguageParseControl, LanguageParseFailure, LanguageParseInput,
    LanguageParsePolicy,
};
use a3_domain::{
    IndexLanguage, LanguageAdapterContractVersion, LanguageAdapterRevision, LanguageAdapterVersion,
    LanguageParseResult, RepositoryPath,
};
use std::error::Error;
use std::fmt;
use tree_sitter::Language;

const RUST_ADAPTER_REVISION: &str = "rust-tree-sitter-0.24.2-cargo-v1-flow-v1-contract-v1";

/// Deterministic Rust source and Cargo-manifest adapter.
#[derive(Debug)]
pub struct RustLanguageAdapter {
    revision: LanguageAdapterRevision,
    parser_pool: TreeSitterParserPool,
}

impl RustLanguageAdapter {
    /// Creates a Rust adapter with a bounded number of reusable grammar parsers.
    pub fn new(size: ParserPoolSize) -> Result<Self, RustLanguageAdapterCreateError> {
        let language: Language = tree_sitter_rust::LANGUAGE.into();
        let revision = LanguageAdapterVersion::try_from_string(RUST_ADAPTER_REVISION.to_owned())
            .map_err(|_| RustLanguageAdapterCreateError::InvalidRevision)?;
        let parser_pool = TreeSitterParserPool::new(&language, size)
            .map_err(RustLanguageAdapterCreateError::ParserPool)?;
        Ok(Self {
            revision: LanguageAdapterRevision::new(IndexLanguage::Rust, revision),
            parser_pool,
        })
    }
}

impl LanguageAdapter for RustLanguageAdapter {
    fn revision(&self) -> &LanguageAdapterRevision {
        &self.revision
    }

    fn contract_version(&self) -> LanguageAdapterContractVersion {
        LanguageAdapterContractVersion::v1()
    }

    fn supports_path(&self, path: &RepositoryPath) -> bool {
        is_rust_source(path) || is_cargo_manifest(path)
    }

    fn parse(
        &self,
        input: LanguageParseInput<'_>,
        policy: LanguageParsePolicy,
        control: &dyn LanguageParseControl,
    ) -> Result<LanguageParseResult, LanguageParseFailure> {
        if !self.supports_path(input.revision().path()) {
            return Err(LanguageParseFailure::UnsupportedPath);
        }
        if policy.contract_version() != self.contract_version() {
            return Err(LanguageParseFailure::InvalidResult);
        }
        crate::verify_language_parse_input(input, policy, control)?;
        if is_cargo_manifest(input.revision().path()) {
            return manifest::parse(input, policy, control, &self.revision);
        }
        syntax::parse(input, policy, control, &self.revision, &self.parser_pool)
    }
}

fn is_rust_source(path: &RepositoryPath) -> bool {
    path.as_bytes()
        .rsplit(|byte| *byte == b'/')
        .next()
        .is_some_and(|name| name.len() > 3 && name.ends_with(b".rs"))
}

fn is_cargo_manifest(path: &RepositoryPath) -> bool {
    path.as_bytes().rsplit(|byte| *byte == b'/').next() == Some(b"Cargo.toml".as_slice())
}

/// Failure while creating the fixed Rust grammar adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RustLanguageAdapterCreateError {
    /// The built-in stable revision identifier violated the domain bound.
    InvalidRevision,
    /// The bounded Tree-sitter parser pool could not be created.
    ParserPool(ParserPoolCreateError),
}

impl fmt::Display for RustLanguageAdapterCreateError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidRevision => formatter.write_str("Rust adapter revision is invalid"),
            Self::ParserPool(error) => {
                write!(formatter, "Rust parser pool creation failed: {error}")
            }
        }
    }
}

impl Error for RustLanguageAdapterCreateError {}

#[cfg(test)]
mod tests {
    use super::{RustLanguageAdapter, is_cargo_manifest, is_rust_source};
    use crate::ParserPoolSize;
    use a3_application::LanguageAdapter;
    use a3_domain::{IndexLanguage, RepositoryPath};
    use std::error::Error;

    #[test]
    fn detection_is_case_sensitive_and_bounded_to_rust_sources_and_cargo_manifests()
    -> Result<(), Box<dyn Error>> {
        let source = RepositoryPath::try_from_bytes(b"crates/core/src/lib.rs".to_vec())?;
        let manifest = RepositoryPath::try_from_bytes(b"crates/core/Cargo.toml".to_vec())?;
        let upper = RepositoryPath::try_from_bytes(b"src/lib.RS".to_vec())?;
        let hidden = RepositoryPath::try_from_bytes(b"src/.rs".to_vec())?;
        assert!(is_rust_source(&source));
        assert!(is_cargo_manifest(&manifest));
        assert!(!is_rust_source(&upper));
        assert!(!is_rust_source(&hidden));

        let adapter = RustLanguageAdapter::new(ParserPoolSize::new(1)?)?;
        assert_eq!(adapter.revision().language(), IndexLanguage::Rust);
        assert!(adapter.supports_path(&source));
        assert!(adapter.supports_path(&manifest));
        Ok(())
    }
}
