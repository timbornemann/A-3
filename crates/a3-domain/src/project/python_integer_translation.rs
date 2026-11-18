use super::{FileRevision, RepositoryPath, SourceRange, SymbolName};
use std::{error::Error, fmt};

/// Maximum complete Python module size admitted by the closed integer-translation recipe.
pub const MAX_PYTHON_TRANSLATION_SOURCE_BYTES: usize = 8 * 1024;

/// Original-bound requirement for a pure Python integer function of the form `x + delta`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PythonIntegerTranslation {
    path: RepositoryPath,
    function: SymbolName,
    delta: i64,
}

impl PythonIntegerTranslation {
    /// Validates the closed recipe's module path and top-level function identifier.
    pub fn new(
        path: RepositoryPath,
        function: SymbolName,
        delta: i64,
    ) -> Result<Self, PythonIntegerTranslationError> {
        let mut bytes = function.as_str().bytes();
        let identifier = bytes
            .next()
            .is_some_and(|byte| byte.is_ascii_alphabetic() || byte == b'_')
            && bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_');
        if !path.as_bytes().ends_with(b".py") || !identifier {
            return Err(PythonIntegerTranslationError::InvalidTarget);
        }
        Ok(Self {
            path,
            function,
            delta,
        })
    }

    /// Returns the exact repository-relative module path.
    #[must_use]
    pub const fn path(&self) -> &RepositoryPath {
        &self.path
    }

    /// Returns the top-level function identifier.
    #[must_use]
    pub const fn function(&self) -> &SymbolName {
        &self.function
    }

    /// Returns the exact signed displacement required by the original request.
    #[must_use]
    pub const fn delta(&self) -> i64 {
        self.delta
    }
}

/// Content-free result of proving the required expression against one complete live revision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PythonIntegerTranslationWitness {
    requirement: PythonIntegerTranslation,
    revision: FileRevision,
    declaration: SourceRange,
    expression: SourceRange,
}

impl PythonIntegerTranslationWitness {
    /// Identifies the closed parser and expression-semantics contract in the durable proof.
    pub const VERSION: u8 = 1;

    /// Binds adapter-verified source semantics to exact declaration and expression ranges.
    pub fn new(
        requirement: PythonIntegerTranslation,
        revision: FileRevision,
        declaration: SourceRange,
        expression: SourceRange,
    ) -> Result<Self, PythonIntegerTranslationError> {
        if revision.path() != requirement.path()
            || expression.is_empty()
            || declaration == expression
            || !declaration.contains(expression)
            || declaration.end_byte() as usize > MAX_PYTHON_TRANSLATION_SOURCE_BYTES
        {
            return Err(PythonIntegerTranslationError::InvalidWitness);
        }
        Ok(Self {
            requirement,
            revision,
            declaration,
            expression,
        })
    }

    /// Returns the exact original-bound predicate proved by the adapter.
    #[must_use]
    pub const fn requirement(&self) -> &PythonIntegerTranslation {
        &self.requirement
    }

    /// Returns the complete source revision on which the proof depends.
    #[must_use]
    pub const fn revision(&self) -> &FileRevision {
        &self.revision
    }

    /// Returns the proved function's exact parser range.
    #[must_use]
    pub const fn declaration(&self) -> SourceRange {
        self.declaration
    }

    /// Returns the exact returned expression's parser range.
    #[must_use]
    pub const fn expression(&self) -> SourceRange {
        self.expression
    }
}

/// A Python integer-translation target or witness violated its closed representation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PythonIntegerTranslationError {
    /// The path or function identifier is outside the recipe's scope.
    InvalidTarget,
    /// The revision or coordinates do not bind the proof to its exact target.
    InvalidWitness,
}

impl fmt::Display for PythonIntegerTranslationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidTarget => "invalid Python integer-translation target",
            Self::InvalidWitness => "invalid Python integer-translation witness",
        })
    }
}

impl Error for PythonIntegerTranslationError {}
