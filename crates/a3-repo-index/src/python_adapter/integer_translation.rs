use super::{PythonLanguageAdapter, source::node_text};
use crate::{source_range_for_node, verify_language_parse_input};
use a3_application::{
    LanguageParseControl, LanguageParseFailure, LanguageParseInput, LanguageParsePolicy,
    PythonIntegerTranslationVerifier,
};
use a3_domain::{
    DiscoveredFileRoles, FileRevision, MAX_PYTHON_TRANSLATION_SOURCE_BYTES,
    PythonIntegerTranslation, PythonIntegerTranslationWitness,
};
use tree_sitter::Node;

const MAX_EXPRESSION_DEPTH: usize = 64;

impl PythonLanguageAdapter {
    /// Proves one bounded integer translation using the complete exact Python module.
    ///
    /// An unsupported form or an expression with a different meaning yields no
    /// witness. Invalid source binding, cancellation and parser limits retain
    /// their typed failures. The check does not execute repository code.
    pub fn prove_python_integer_translation(
        &self,
        required: &PythonIntegerTranslation,
        revision: &FileRevision,
        source: &[u8],
        control: &dyn LanguageParseControl,
    ) -> Result<Option<PythonIntegerTranslationWitness>, LanguageParseFailure> {
        if control.is_cancelled() {
            return Err(LanguageParseFailure::Cancelled);
        }
        if revision.path() != required.path() {
            return Err(LanguageParseFailure::RevisionMismatch);
        }
        if source.len() > MAX_PYTHON_TRANSLATION_SOURCE_BYTES {
            return Err(LanguageParseFailure::InputTooLarge);
        }
        let policy = LanguageParsePolicy::v1();
        let input = LanguageParseInput::new(revision, source, DiscoveredFileRoles::empty());
        verify_language_parse_input(input, policy, control)?;
        if !std::str::from_utf8(source).is_ok_and(supported_source_layout) {
            return Ok(None);
        }
        let parsed = self.parser_pool.parse(source, policy, control)?;
        let root = parsed.tree().root_node();
        let proved = if parsed.diagnostics().is_empty() && !root.has_error() {
            prove_module(root, source, required)
        } else {
            None
        };
        if control.is_cancelled() {
            return Err(LanguageParseFailure::Cancelled);
        }
        let Some((declaration, expression)) = proved else {
            return Ok(None);
        };
        let witness = PythonIntegerTranslationWitness::new(
            required.clone(),
            revision.clone(),
            source_range_for_node(declaration)?,
            source_range_for_node(expression)?,
        )
        .map_err(|_| LanguageParseFailure::InvalidResult)?;
        Ok(Some(witness))
    }
}

impl PythonIntegerTranslationVerifier for PythonLanguageAdapter {
    fn prove(
        &self,
        required: &PythonIntegerTranslation,
        revision: &FileRevision,
        source: &[u8],
        control: &dyn LanguageParseControl,
    ) -> Result<Option<PythonIntegerTranslationWitness>, LanguageParseFailure> {
        self.prove_python_integer_translation(required, revision, source, control)
    }
}

fn supported_source_layout(source: &str) -> bool {
    source
        .split_inclusive('\n')
        .enumerate()
        .all(|(index, line)| {
            // Keep Python's decoding and indentation semantics within the same
            // source grammar as the proof. Comments can contain ordinary UTF-8.
            let line = line
                .strip_suffix('\n')
                .map_or(line, |line| line.strip_suffix('\r').unwrap_or(line));
            let (code, comment) = line.split_once('#').unwrap_or((line, ""));
            code.bytes()
                .all(|byte| byte == b' ' || (byte.is_ascii_graphic() && byte != b'\\'))
                && !comment
                    .bytes()
                    .any(|byte| byte.is_ascii_control() && byte != b'\t')
                && (index >= 2 || (!comment.contains("coding:") && !comment.contains("coding=")))
        })
}

fn prove_module<'tree>(
    root: Node<'tree>,
    source: &[u8],
    required: &PythonIntegerTranslation,
) -> Option<(Node<'tree>, Node<'tree>)> {
    if root.kind() != "module" {
        return None;
    }
    let function = only_content_child(root)?;
    if function.kind() != "function_definition"
        || function.start_position().column != 0
        || function.child(0)?.kind() != "def"
        || function.child_by_field_name("type_parameters").is_some()
    {
        return None;
    }
    let name = ordinary_identifier(function.child_by_field_name("name")?, source)?;
    if name != required.function().as_str() {
        return None;
    }
    let parameter = only_content_child(function.child_by_field_name("parameters")?)?;
    let parameter = match parameter.kind() {
        "identifier" => parameter,
        "typed_parameter" if is_builtin_int(parameter.child_by_field_name("type")?, source) => {
            parameter.named_child(0)?
        }
        _ => return None,
    };
    let parameter = ordinary_identifier(parameter, source)?;
    if function
        .child_by_field_name("return_type")
        .is_some_and(|annotation| !is_builtin_int(annotation, source))
    {
        return None;
    }
    let statement = only_content_child(function.child_by_field_name("body")?)?;
    if statement.kind() != "return_statement" {
        return None;
    }
    let expression = only_content_child(statement)?;
    let affine = affine_expression(expression, source, parameter, MAX_EXPRESSION_DEPTH)?;
    (affine.slope == 1 && affine.offset == i128::from(required.delta()))
        .then_some((function, expression))
}

fn only_content_child(node: Node<'_>) -> Option<Node<'_>> {
    let mut cursor = node.walk();
    let mut children = node
        .named_children(&mut cursor)
        .filter(|child| child.kind() != "comment");
    let child = children.next()?;
    children.next().is_none().then_some(child)
}

fn is_builtin_int(annotation: Node<'_>, source: &[u8]) -> bool {
    node_text(source, annotation) == Some("int")
}

fn ordinary_identifier<'source>(node: Node<'_>, source: &'source [u8]) -> Option<&'source str> {
    let name = node_text(source, node)?;
    // The grammar also recognizes some tokens as identifiers in contexts in
    // which Python 3 reserves them.
    if node.kind() != "identifier"
        || matches!(
            name,
            "__debug__"
                | "False"
                | "None"
                | "True"
                | "and"
                | "as"
                | "assert"
                | "async"
                | "await"
                | "break"
                | "class"
                | "continue"
                | "def"
                | "del"
                | "elif"
                | "else"
                | "except"
                | "finally"
                | "for"
                | "from"
                | "global"
                | "if"
                | "import"
                | "in"
                | "is"
                | "lambda"
                | "nonlocal"
                | "not"
                | "or"
                | "pass"
                | "raise"
                | "return"
                | "try"
                | "while"
                | "with"
                | "yield"
        )
    {
        return None;
    }
    Some(name)
}

struct Affine {
    slope: i128,
    offset: i128,
}

impl Affine {
    fn add(self, other: Self) -> Option<Self> {
        Some(Self {
            slope: self.slope.checked_add(other.slope)?,
            offset: self.offset.checked_add(other.offset)?,
        })
    }

    fn subtract(self, other: Self) -> Option<Self> {
        Some(Self {
            slope: self.slope.checked_sub(other.slope)?,
            offset: self.offset.checked_sub(other.offset)?,
        })
    }
}

fn affine_expression(
    node: Node<'_>,
    source: &[u8],
    parameter: &str,
    remaining_depth: usize,
) -> Option<Affine> {
    let depth = remaining_depth.checked_sub(1)?;
    let descend = |child| affine_expression(child, source, parameter, depth);
    match node.kind() {
        "identifier" if node_text(source, node)? == parameter => Some(Affine {
            slope: 1,
            offset: 0,
        }),
        "integer" => Some(Affine {
            slope: 0,
            offset: integer_literal(node_text(source, node)?)?,
        }),
        "parenthesized_expression" => descend(only_content_child(node)?),
        "unary_operator" => {
            let argument = descend(node.child_by_field_name("argument")?)?;
            match node.child_by_field_name("operator")?.kind() {
                "+" => Some(argument),
                "-" => Affine {
                    slope: 0,
                    offset: 0,
                }
                .subtract(argument),
                _ => None,
            }
        }
        "binary_operator" => {
            let left = descend(node.child_by_field_name("left")?)?;
            let right = descend(node.child_by_field_name("right")?)?;
            match node.child_by_field_name("operator")?.kind() {
                "+" => left.add(right),
                "-" => left.subtract(right),
                _ => None,
            }
        }
        _ => None,
    }
}

fn integer_literal(token: &str) -> Option<i128> {
    let (radix, digits) = match token.as_bytes() {
        [b'0', b'b' | b'B', digits @ ..] => (2, digits),
        [b'0', b'o' | b'O', digits @ ..] => (8, digits),
        [b'0', b'x' | b'X', digits @ ..] => (16, digits),
        digits => (10, digits),
    };
    // A single underscore immediately after a base prefix is valid in Python.
    let mut previous_digit = radix != 10;
    let mut value = 0_i128;
    for byte in digits {
        if *byte == b'_' && previous_digit {
            previous_digit = false;
            continue;
        }
        let digit = char::from(*byte).to_digit(radix)?;
        value = value
            .checked_mul(i128::from(radix))?
            .checked_add(i128::from(digit))?;
        previous_digit = true;
    }
    if digits.is_empty()
        || !previous_digit
        || (radix == 10 && digits.first() == Some(&b'0') && value != 0)
    {
        return None;
    }
    Some(value)
}

#[cfg(test)]
mod tests;
