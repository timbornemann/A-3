mod manifest;
mod pyproject;
mod requirements;
mod setup_cfg;
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

const ADAPTER_REVISION: &str =
    "python-tree-sitter-0.25.0-pyproject-setup-requirements-v1-contract-v1";

/// Deterministic Python source and packaging-metadata adapter.
#[derive(Debug)]
pub struct PythonLanguageAdapter {
    revision: LanguageAdapterRevision,
    parser_pool: TreeSitterParserPool,
}

impl PythonLanguageAdapter {
    /// Creates a bounded reusable parser pool for the pinned Python grammar.
    pub fn new(size: ParserPoolSize) -> Result<Self, PythonLanguageAdapterCreateError> {
        let language: Language = tree_sitter_python::LANGUAGE.into();
        let revision = LanguageAdapterVersion::try_from_string(ADAPTER_REVISION.to_owned())
            .map_err(|_| PythonLanguageAdapterCreateError::InvalidRevision)?;
        let parser_pool = TreeSitterParserPool::new(&language, size)
            .map_err(PythonLanguageAdapterCreateError::ParserPool)?;
        Ok(Self {
            revision: LanguageAdapterRevision::new(IndexLanguage::Python, revision),
            parser_pool,
        })
    }
}

impl LanguageAdapter for PythonLanguageAdapter {
    fn revision(&self) -> &LanguageAdapterRevision {
        &self.revision
    }

    fn contract_version(&self) -> LanguageAdapterContractVersion {
        LanguageAdapterContractVersion::v1()
    }

    fn supports_path(&self, path: &RepositoryPath) -> bool {
        is_python_source(path) || is_pyproject(path) || is_setup_cfg(path) || is_requirements(path)
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
        if is_pyproject(input.revision().path()) {
            return pyproject::parse(input, policy, control, &self.revision);
        }
        if is_setup_cfg(input.revision().path()) {
            return setup_cfg::parse(input, policy, control, &self.revision);
        }
        if is_requirements(input.revision().path()) {
            return requirements::parse(input, policy, control, &self.revision);
        }
        syntax::parse(input, policy, control, &self.revision, &self.parser_pool)
    }
}

fn is_python_source(path: &RepositoryPath) -> bool {
    let name = basename(path);
    [b".py".as_slice(), b".pyi"]
        .iter()
        .any(|extension| name.len() > extension.len() && name.ends_with(extension))
}

fn is_pyproject(path: &RepositoryPath) -> bool {
    basename(path) == b"pyproject.toml"
}

fn is_setup_cfg(path: &RepositoryPath) -> bool {
    basename(path) == b"setup.cfg"
}

fn is_setup_py(path: &RepositoryPath) -> bool {
    basename(path) == b"setup.py"
}

fn is_requirements(path: &RepositoryPath) -> bool {
    let name = basename(path);
    let supported_extension = name.ends_with(b".txt") || name.ends_with(b".in");
    supported_extension
        && (name.starts_with(b"requirements")
            || path
                .as_bytes()
                .split(|byte| *byte == b'/')
                .any(|component| component == b"requirements"))
}

fn basename(path: &RepositoryPath) -> &[u8] {
    path.as_bytes()
        .rsplit(|byte| *byte == b'/')
        .next()
        .unwrap_or_default()
}

/// Failure while creating the pinned Python grammar adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PythonLanguageAdapterCreateError {
    /// The built-in stable revision identifier violated the domain bound.
    InvalidRevision,
    /// The bounded Tree-sitter parser pool could not be created.
    ParserPool(ParserPoolCreateError),
}

impl fmt::Display for PythonLanguageAdapterCreateError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidRevision => formatter.write_str("Python adapter revision is invalid"),
            Self::ParserPool(error) => {
                write!(formatter, "Python parser pool creation failed: {error}")
            }
        }
    }
}

impl Error for PythonLanguageAdapterCreateError {}

#[cfg(test)]
mod tests {
    use super::{
        PythonLanguageAdapter, is_pyproject, is_python_source, is_requirements, is_setup_cfg,
    };
    use crate::ParserPoolSize;
    use a3_application::LanguageAdapter;
    use a3_domain::{IndexLanguage, RepositoryPath};
    use std::error::Error;

    fn path(value: &[u8]) -> Result<RepositoryPath, Box<dyn Error>> {
        Ok(RepositoryPath::try_from_bytes(value.to_vec())?)
    }

    #[test]
    fn detection_is_case_sensitive_and_covers_sources_and_packaging_metadata()
    -> Result<(), Box<dyn Error>> {
        assert!(is_python_source(&path(b"src/package/module.py")?));
        assert!(is_python_source(&path(b"src/package/types.pyi")?));
        assert!(!is_python_source(&path(b"src/package/.py")?));
        assert!(!is_python_source(&path(b"src/package/module.PY")?));
        assert!(is_pyproject(&path(b"pyproject.toml")?));
        assert!(is_setup_cfg(&path(b"setup.cfg")?));
        assert!(is_requirements(&path(b"requirements-dev.txt")?));
        assert!(is_requirements(&path(b"requirements/base.in")?));
        assert!(!is_requirements(&path(b"notes.txt")?));

        let adapter = PythonLanguageAdapter::new(ParserPoolSize::new(1)?)?;
        assert_eq!(adapter.revision().language(), IndexLanguage::Python);
        Ok(())
    }
}
