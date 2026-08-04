use super::SourceRange;
use std::error::Error;
use std::fmt;

const MAX_SYMBOL_NAME_BYTES: usize = 1_024;
const MAX_SYMBOL_SIGNATURE_BYTES: usize = 16 * 1_024;
const TEST_ROLE: u8 = 1 << 0;
const ENTRYPOINT_ROLE: u8 = 1 << 1;

/// File-local symbol identity emitted deterministically by one language adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct LocalSymbolId(u32);

impl LocalSymbolId {
    /// Creates a positive file-local identity.
    pub fn new(value: u32) -> Result<Self, LocalSymbolIdError> {
        if value == 0 {
            return Err(LocalSymbolIdError);
        }
        Ok(Self(value))
    }

    /// Returns the stable primitive representation.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// File-local symbol identity zero is invalid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalSymbolIdError;

impl fmt::Display for LocalSymbolIdError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("local symbol ID must be positive")
    }
}

impl Error for LocalSymbolIdError {}

/// Bounded source-derived symbol name.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SymbolName(String);

impl SymbolName {
    /// Validates a non-empty name without control characters.
    pub fn try_from_string(value: String) -> Result<Self, SymbolTextError> {
        validate_text(&value, MAX_SYMBOL_NAME_BYTES, TextWhitespace::SingleLine)?;
        Ok(Self(value))
    }

    /// Returns the source-derived name.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Bounded source-derived declaration signature.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SymbolSignature(String);

impl SymbolSignature {
    /// Validates a non-empty, bounded signature while retaining layout whitespace.
    pub fn try_from_string(value: String) -> Result<Self, SymbolTextError> {
        validate_text(&value, MAX_SYMBOL_SIGNATURE_BYTES, TextWhitespace::Layout)?;
        Ok(Self(value))
    }

    /// Returns the exact bounded signature projection.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Copy)]
enum TextWhitespace {
    SingleLine,
    Layout,
}

fn validate_text(
    value: &str,
    maximum: usize,
    whitespace: TextWhitespace,
) -> Result<(), SymbolTextError> {
    if value.is_empty() || value.len() > maximum {
        return Err(SymbolTextError::InvalidLength(value.len()));
    }
    if value.chars().any(|character| {
        character == '\0'
            || (character.is_control() && !matches!(whitespace, TextWhitespace::Layout))
            || (matches!(whitespace, TextWhitespace::Layout)
                && character.is_control()
                && !matches!(character, '\n' | '\r' | '\t'))
    }) {
        return Err(SymbolTextError::InvalidCharacter);
    }
    Ok(())
}

/// Invalid symbol name or signature.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymbolTextError {
    /// Text was empty or exceeded its fixed byte limit.
    InvalidLength(usize),
    /// Text contained NUL or a disallowed control character.
    InvalidCharacter,
}

impl fmt::Display for SymbolTextError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLength(length) => {
                write!(formatter, "symbol text has invalid length {length}")
            }
            Self::InvalidCharacter => {
                formatter.write_str("symbol text contains an invalid character")
            }
        }
    }
}

impl Error for SymbolTextError {}

/// Language-neutral structural symbol category.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SymbolKind {
    /// File or language module.
    Module,
    /// Namespace or package scope.
    Namespace,
    /// Free function.
    Function,
    /// Type-associated function or method.
    Method,
    /// Rust-style struct or equivalent record.
    Struct,
    /// Enumeration type.
    Enum,
    /// Trait or protocol.
    Trait,
    /// Interface declaration.
    Interface,
    /// Class declaration.
    Class,
    /// Type alias.
    TypeAlias,
    /// Constant declaration.
    Constant,
    /// Static storage declaration.
    Static,
    /// Variable declaration.
    Variable,
    /// Field or property.
    Field,
    /// Enumeration variant.
    Variant,
    /// Function or method parameter.
    Parameter,
}

/// Visibility directly observable in source syntax.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SymbolVisibility {
    /// Exported or public API.
    Public,
    /// Protected member visibility.
    Protected,
    /// Private visibility.
    Private,
    /// Package-, crate-, or module-internal visibility.
    Internal,
    /// Local lexical binding.
    Local,
    /// The syntax does not determine visibility.
    Unknown,
}

/// Overlapping semantic role of a parsed symbol.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SymbolRole {
    /// Test definition.
    Test,
    /// Program, library, or script entrypoint.
    Entrypoint,
}

/// Compact validated symbol-role set.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SymbolRoles(u8);

impl SymbolRoles {
    /// Returns an empty role set.
    #[must_use]
    pub const fn empty() -> Self {
        Self(0)
    }

    /// Adds one role.
    #[must_use]
    pub const fn with(mut self, role: SymbolRole) -> Self {
        self.0 |= match role {
            SymbolRole::Test => TEST_ROLE,
            SymbolRole::Entrypoint => ENTRYPOINT_ROLE,
        };
        self
    }

    /// Returns whether the set contains one role.
    #[must_use]
    pub const fn contains(self, role: SymbolRole) -> bool {
        let mask = match role {
            SymbolRole::Test => TEST_ROLE,
            SymbolRole::Entrypoint => ENTRYPOINT_ROLE,
        };
        self.0 & mask != 0
    }
}

/// One language-neutral symbol extracted from source syntax.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedSymbol {
    id: LocalSymbolId,
    kind: SymbolKind,
    name: SymbolName,
    signature: Option<SymbolSignature>,
    declaration_range: SourceRange,
    selection_range: SourceRange,
    documentation_range: Option<SourceRange>,
    visibility: SymbolVisibility,
    roles: SymbolRoles,
}

impl ParsedSymbol {
    /// Creates the required symbol core and validates its selection range.
    pub fn new(
        id: LocalSymbolId,
        kind: SymbolKind,
        name: SymbolName,
        declaration_range: SourceRange,
        selection_range: SourceRange,
    ) -> Result<Self, ParsedSymbolError> {
        if !declaration_range.contains(selection_range) {
            return Err(ParsedSymbolError::SelectionOutsideDeclaration);
        }
        Ok(Self {
            id,
            kind,
            name,
            signature: None,
            declaration_range,
            selection_range,
            documentation_range: None,
            visibility: SymbolVisibility::Unknown,
            roles: SymbolRoles::empty(),
        })
    }

    /// Attaches a bounded declaration signature.
    #[must_use]
    pub fn with_signature(mut self, signature: SymbolSignature) -> Self {
        self.signature = Some(signature);
        self
    }

    /// Attaches syntactically observed visibility.
    #[must_use]
    pub const fn with_visibility(mut self, visibility: SymbolVisibility) -> Self {
        self.visibility = visibility;
        self
    }

    /// Adds a semantic role.
    #[must_use]
    pub const fn with_role(mut self, role: SymbolRole) -> Self {
        self.roles = self.roles.with(role);
        self
    }

    /// Attaches a documentation range, validated against the file by the result aggregate.
    #[must_use]
    pub const fn with_documentation_range(mut self, range: SourceRange) -> Self {
        self.documentation_range = Some(range);
        self
    }

    /// Returns the file-local symbol ID.
    #[must_use]
    pub const fn id(&self) -> LocalSymbolId {
        self.id
    }

    /// Returns the language-neutral kind.
    #[must_use]
    pub const fn kind(&self) -> SymbolKind {
        self.kind
    }

    /// Returns the symbol name.
    #[must_use]
    pub const fn name(&self) -> &SymbolName {
        &self.name
    }

    /// Returns the declaration signature when provided.
    #[must_use]
    pub const fn signature(&self) -> Option<&SymbolSignature> {
        self.signature.as_ref()
    }

    /// Returns the full declaration range.
    #[must_use]
    pub const fn declaration_range(&self) -> SourceRange {
        self.declaration_range
    }

    /// Returns the name selection range.
    #[must_use]
    pub const fn selection_range(&self) -> SourceRange {
        self.selection_range
    }

    /// Returns the associated documentation range when observed.
    #[must_use]
    pub const fn documentation_range(&self) -> Option<SourceRange> {
        self.documentation_range
    }

    /// Returns syntactically observed visibility.
    #[must_use]
    pub const fn visibility(&self) -> SymbolVisibility {
        self.visibility
    }

    /// Returns overlapping semantic roles.
    #[must_use]
    pub const fn roles(&self) -> SymbolRoles {
        self.roles
    }
}

/// Invalid relationship between symbol source ranges.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParsedSymbolError {
    /// The name selection range was not inside its declaration.
    SelectionOutsideDeclaration,
}

impl fmt::Display for ParsedSymbolError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("symbol selection is outside its declaration")
    }
}

impl Error for ParsedSymbolError {}
