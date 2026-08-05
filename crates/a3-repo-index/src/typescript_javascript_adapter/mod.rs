mod package_manifest;
mod pnpm_workspace;
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
    "typescript-javascript-ts-0.23.2-js-0.25.0-json-0.24.8-package-v1-contract-v1";

/// Deterministic TypeScript, JavaScript, package, and workspace adapter.
#[derive(Debug)]
pub struct TypeScriptJavaScriptLanguageAdapter {
    revision: LanguageAdapterRevision,
    javascript_pool: TreeSitterParserPool,
    typescript_pool: TreeSitterParserPool,
    tsx_pool: TreeSitterParserPool,
    json_pool: TreeSitterParserPool,
}

impl TypeScriptJavaScriptLanguageAdapter {
    /// Creates bounded reusable parser pools for every supported grammar dialect.
    pub fn new(
        size: ParserPoolSize,
    ) -> Result<Self, TypeScriptJavaScriptLanguageAdapterCreateError> {
        let javascript: Language = tree_sitter_javascript::LANGUAGE.into();
        let typescript: Language = tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into();
        let tsx: Language = tree_sitter_typescript::LANGUAGE_TSX.into();
        let json: Language = tree_sitter_json::LANGUAGE.into();
        let revision = LanguageAdapterVersion::try_from_string(ADAPTER_REVISION.to_owned())
            .map_err(|_| TypeScriptJavaScriptLanguageAdapterCreateError::InvalidRevision)?;
        let javascript_pool = TreeSitterParserPool::new(&javascript, size)
            .map_err(TypeScriptJavaScriptLanguageAdapterCreateError::JavaScriptParserPool)?;
        let typescript_pool = TreeSitterParserPool::new(&typescript, size)
            .map_err(TypeScriptJavaScriptLanguageAdapterCreateError::TypeScriptParserPool)?;
        let tsx_pool = TreeSitterParserPool::new(&tsx, size)
            .map_err(TypeScriptJavaScriptLanguageAdapterCreateError::TsxParserPool)?;
        let json_pool = TreeSitterParserPool::new(&json, size)
            .map_err(TypeScriptJavaScriptLanguageAdapterCreateError::JsonParserPool)?;
        Ok(Self {
            revision: LanguageAdapterRevision::new(IndexLanguage::TypeScriptJavaScript, revision),
            javascript_pool,
            typescript_pool,
            tsx_pool,
            json_pool,
        })
    }
}

impl LanguageAdapter for TypeScriptJavaScriptLanguageAdapter {
    fn revision(&self) -> &LanguageAdapterRevision {
        &self.revision
    }

    fn contract_version(&self) -> LanguageAdapterContractVersion {
        LanguageAdapterContractVersion::v1()
    }

    fn supports_path(&self, path: &RepositoryPath) -> bool {
        source_dialect(path).is_some() || is_package_manifest(path) || is_pnpm_workspace(path)
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
        if is_package_manifest(input.revision().path()) {
            return package_manifest::parse(
                input,
                policy,
                control,
                &self.revision,
                &self.json_pool,
            );
        }
        if is_pnpm_workspace(input.revision().path()) {
            return pnpm_workspace::parse(input, policy, control, &self.revision);
        }
        let dialect =
            source_dialect(input.revision().path()).ok_or(LanguageParseFailure::UnsupportedPath)?;
        let pool = match dialect {
            SourceDialect::JavaScript => &self.javascript_pool,
            SourceDialect::TypeScript => &self.typescript_pool,
            SourceDialect::Tsx => &self.tsx_pool,
        };
        syntax::parse(input, policy, control, &self.revision, pool)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SourceDialect {
    JavaScript,
    TypeScript,
    Tsx,
}

fn source_dialect(path: &RepositoryPath) -> Option<SourceDialect> {
    let name = basename(path);
    if name.len() > 4 && name.ends_with(b".tsx") {
        return Some(SourceDialect::Tsx);
    }
    if [b".ts".as_slice(), b".mts", b".cts"]
        .iter()
        .any(|extension| name.len() > extension.len() && name.ends_with(extension))
    {
        return Some(SourceDialect::TypeScript);
    }
    if [b".js".as_slice(), b".jsx", b".mjs", b".cjs"]
        .iter()
        .any(|extension| name.len() > extension.len() && name.ends_with(extension))
    {
        return Some(SourceDialect::JavaScript);
    }
    None
}

fn is_package_manifest(path: &RepositoryPath) -> bool {
    basename(path) == b"package.json"
}

fn is_pnpm_workspace(path: &RepositoryPath) -> bool {
    basename(path) == b"pnpm-workspace.yaml"
}

fn basename(path: &RepositoryPath) -> &[u8] {
    path.as_bytes()
        .rsplit(|byte| *byte == b'/')
        .next()
        .unwrap_or_default()
}

/// Failure while creating the fixed TS/JS grammar adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypeScriptJavaScriptLanguageAdapterCreateError {
    /// The built-in stable revision identifier violated the domain bound.
    InvalidRevision,
    /// The JavaScript grammar pool could not be created.
    JavaScriptParserPool(ParserPoolCreateError),
    /// The TypeScript grammar pool could not be created.
    TypeScriptParserPool(ParserPoolCreateError),
    /// The TSX grammar pool could not be created.
    TsxParserPool(ParserPoolCreateError),
    /// The JSON grammar pool could not be created.
    JsonParserPool(ParserPoolCreateError),
}

impl fmt::Display for TypeScriptJavaScriptLanguageAdapterCreateError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidRevision => formatter.write_str("TS/JS adapter revision is invalid"),
            Self::JavaScriptParserPool(error) => {
                write!(formatter, "JavaScript parser pool creation failed: {error}")
            }
            Self::TypeScriptParserPool(error) => {
                write!(formatter, "TypeScript parser pool creation failed: {error}")
            }
            Self::TsxParserPool(error) => {
                write!(formatter, "TSX parser pool creation failed: {error}")
            }
            Self::JsonParserPool(error) => {
                write!(formatter, "JSON parser pool creation failed: {error}")
            }
        }
    }
}

impl Error for TypeScriptJavaScriptLanguageAdapterCreateError {}

#[cfg(test)]
mod tests {
    use super::{
        SourceDialect, TypeScriptJavaScriptLanguageAdapter, is_package_manifest, is_pnpm_workspace,
        source_dialect,
    };
    use crate::ParserPoolSize;
    use a3_application::LanguageAdapter;
    use a3_domain::{IndexLanguage, RepositoryPath};
    use std::error::Error;

    fn path(value: &[u8]) -> Result<RepositoryPath, Box<dyn Error>> {
        Ok(RepositoryPath::try_from_bytes(value.to_vec())?)
    }

    #[test]
    fn path_detection_is_case_sensitive_and_covers_all_supported_dialects()
    -> Result<(), Box<dyn Error>> {
        assert_eq!(
            source_dialect(&path(b"src/app.js")?),
            Some(SourceDialect::JavaScript)
        );
        assert_eq!(
            source_dialect(&path(b"src/app.jsx")?),
            Some(SourceDialect::JavaScript)
        );
        assert_eq!(
            source_dialect(&path(b"src/app.mjs")?),
            Some(SourceDialect::JavaScript)
        );
        assert_eq!(
            source_dialect(&path(b"src/app.cjs")?),
            Some(SourceDialect::JavaScript)
        );
        assert_eq!(
            source_dialect(&path(b"src/app.ts")?),
            Some(SourceDialect::TypeScript)
        );
        assert_eq!(
            source_dialect(&path(b"src/app.mts")?),
            Some(SourceDialect::TypeScript)
        );
        assert_eq!(
            source_dialect(&path(b"src/app.cts")?),
            Some(SourceDialect::TypeScript)
        );
        assert_eq!(
            source_dialect(&path(b"src/app.tsx")?),
            Some(SourceDialect::Tsx)
        );
        assert_eq!(source_dialect(&path(b"src/.ts")?), None);
        assert_eq!(source_dialect(&path(b"src/app.TS")?), None);
        assert!(is_package_manifest(&path(b"packages/core/package.json")?));
        assert!(is_pnpm_workspace(&path(b"pnpm-workspace.yaml")?));

        let adapter = TypeScriptJavaScriptLanguageAdapter::new(ParserPoolSize::new(1)?)?;
        assert_eq!(
            adapter.revision().language(),
            IndexLanguage::TypeScriptJavaScript
        );
        Ok(())
    }
}
