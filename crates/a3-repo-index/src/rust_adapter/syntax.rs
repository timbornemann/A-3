use super::source::{diagnostic, node_text, normalized_node_text, warning};
use crate::{TreeSitterParserPool, normalize_parse_diagnostics, source_range_for_node};
use a3_application::{
    LanguageParseControl, LanguageParseFailure, LanguageParseInput, LanguageParsePolicy,
};
use a3_domain::{
    Confidence, DiscoveredFileRole, LanguageAdapterRevision, LanguageParseArtifacts,
    LanguageParseResult, LocalSymbolId, ParseDiagnostic, ParseDiagnosticCode, ParsedSymbol,
    RepositoryPath, SourceRange, SymbolKind, SymbolName, SymbolReference, SymbolRole,
    SymbolSignature, SymbolVisibility, SyntaxProvider, SyntaxRelation, SyntaxRelationKind,
    SyntaxSource, SyntaxTarget,
};
use std::time::Instant;
use tree_sitter::{Node, Tree};

const CALL_CONFIDENCE_BASIS_POINTS: u16 = 7_500;
const EXTRACTION_POLL_INTERVAL: usize = 256;

pub(super) fn parse(
    input: LanguageParseInput<'_>,
    policy: LanguageParsePolicy,
    control: &dyn LanguageParseControl,
    revision: &LanguageAdapterRevision,
    parser_pool: &TreeSitterParserPool,
) -> Result<LanguageParseResult, LanguageParseFailure> {
    let parsed = parser_pool.parse(input.source(), policy, control)?;
    let (tree, _parser_coverage, diagnostics) = parsed.into_parts();
    let artifacts = RustSyntaxExtractor::new(input, policy, control, diagnostics).extract(&tree)?;
    let (coverage, diagnostics) = normalize_parse_diagnostics(
        input.source().len(),
        policy.max_diagnostics(),
        artifacts.diagnostics,
    )?;
    let result = LanguageParseResult::new(
        input.revision().clone(),
        revision.clone(),
        policy.contract_version(),
        coverage,
        LanguageParseArtifacts {
            diagnostics,
            ..artifacts
        },
    )
    .map_err(|_| LanguageParseFailure::InvalidResult)?;
    crate::function_flow::attach(&tree, input, result, control)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RustScope {
    Module,
    Struct,
    Enum,
    Trait,
    Implementation,
    Function,
    Other,
}

#[derive(Debug, Clone, Copy)]
struct Frame<'tree> {
    node: Node<'tree>,
    container: LocalSymbolId,
    callable: Option<LocalSymbolId>,
    scope: RustScope,
}

struct SymbolVisit {
    id: LocalSymbolId,
    scope: RustScope,
    callable: Option<LocalSymbolId>,
}

struct RustSyntaxExtractor<'a> {
    input: LanguageParseInput<'a>,
    policy: LanguageParsePolicy,
    control: &'a dyn LanguageParseControl,
    artifacts: LanguageParseArtifacts,
    next_symbol_id: u32,
    started: Instant,
}

impl<'a> RustSyntaxExtractor<'a> {
    fn new(
        input: LanguageParseInput<'a>,
        policy: LanguageParsePolicy,
        control: &'a dyn LanguageParseControl,
        diagnostics: Vec<ParseDiagnostic>,
    ) -> Self {
        Self {
            input,
            policy,
            control,
            artifacts: LanguageParseArtifacts {
                diagnostics,
                ..LanguageParseArtifacts::default()
            },
            next_symbol_id: 1,
            started: Instant::now(),
        }
    }

    fn extract(mut self, tree: &Tree) -> Result<LanguageParseArtifacts, LanguageParseFailure> {
        self.ensure_active()?;
        let root = tree.root_node();
        let root_id = self.add_root_module(root)?;
        let mut stack = Vec::new();
        self.push_children(root, root_id, None, RustScope::Module, &mut stack)?;
        let mut visited = 0usize;
        while let Some(frame) = stack.pop() {
            visited = visited
                .checked_add(1)
                .filter(|value| *value <= self.policy.max_tree_nodes())
                .ok_or(LanguageParseFailure::ResourceLimitExceeded)?;
            if visited.is_multiple_of(EXTRACTION_POLL_INTERVAL) {
                self.ensure_active()?;
            }

            if matches!(
                frame.node.kind(),
                "attribute_item" | "line_comment" | "block_comment"
            ) {
                continue;
            }
            if frame.node.kind() == "use_declaration" {
                self.add_use_relations(frame)?;
                continue;
            }

            let visit = self.add_symbol(frame)?;
            let (container, callable, scope) = match visit {
                Some(visit) => (visit.id, visit.callable, visit.scope),
                None => (frame.container, frame.callable, frame.scope),
            };

            match frame.node.kind() {
                "call_expression" => self.add_call_relation(frame)?,
                "macro_invocation" => {
                    self.add_macro_call_relation(frame)?;
                    self.mark_macro_tokens_incomplete(frame.node)?;
                }
                "macro_definition" => self.mark_macro_tokens_incomplete(frame.node)?,
                _ => {}
            }
            self.push_children(frame.node, container, callable, scope, &mut stack)?;
        }
        self.ensure_active()?;
        Ok(self.artifacts)
    }

    fn add_root_module(&mut self, root: Node<'_>) -> Result<LocalSymbolId, LanguageParseFailure> {
        let id = self.take_symbol_id()?;
        let declaration_range = source_range_for_node(root)?;
        let selection_range = super::source::range_for_offsets(self.input.source(), 0, 0)?;
        let mut symbol = ParsedSymbol::new(
            id,
            SymbolKind::Module,
            module_name(self.input.revision().path())?,
            declaration_range,
            selection_range,
        )
        .map_err(|_| LanguageParseFailure::InvalidResult)?
        .with_visibility(SymbolVisibility::Internal);
        if self
            .input
            .discovery_roles()
            .contains(DiscoveredFileRole::Test)
        {
            symbol = symbol.with_role(SymbolRole::Test);
        }
        if is_crate_entry_path(self.input.revision().path()) {
            symbol = symbol.with_role(SymbolRole::Entrypoint);
        }
        self.push_symbol(symbol)?;
        self.push_relation(SyntaxRelation::new(
            SyntaxSource::File,
            SyntaxTarget::Symbol(id),
            SyntaxRelationKind::Defines,
            SyntaxProvider::TreeSitter,
            Confidence::certain(),
            declaration_range,
        ))?;
        Ok(id)
    }

    fn add_symbol(
        &mut self,
        frame: Frame<'_>,
    ) -> Result<Option<SymbolVisit>, LanguageParseFailure> {
        let Some((kind, next_scope)) = symbol_kind(frame.node.kind(), frame.scope) else {
            return Ok(None);
        };
        let Some((name, selection_node)) = self.symbol_name(frame.node)? else {
            return Ok(None);
        };
        let id = self.take_symbol_id()?;
        let declaration_range = source_range_for_node(frame.node)?;
        let selection_range = source_range_for_node(selection_node)?;
        let mut symbol = ParsedSymbol::new(id, kind, name, declaration_range, selection_range)
            .map_err(|_| LanguageParseFailure::InvalidResult)?
            .with_visibility(self.visibility(frame.node, frame.scope));

        if let Some(signature) = self.signature(frame.node)? {
            symbol = symbol.with_signature(signature);
        }
        let decorations = self.decorations(frame.node)?;
        if let Some(range) = decorations.documentation_range {
            symbol = symbol.with_documentation_range(range);
        }
        if decorations.is_test
            || (kind == SymbolKind::Module
                && self
                    .input
                    .discovery_roles()
                    .contains(DiscoveredFileRole::Test))
        {
            symbol = symbol.with_role(SymbolRole::Test);
        }
        if kind == SymbolKind::Function
            && symbol.name().as_str() == "main"
            && is_binary_entry_path(self.input.revision().path())
        {
            symbol = symbol.with_role(SymbolRole::Entrypoint);
        }
        self.push_symbol(symbol)?;
        self.push_relation(SyntaxRelation::new(
            SyntaxSource::Symbol(frame.container),
            SyntaxTarget::Symbol(id),
            SyntaxRelationKind::Contains,
            SyntaxProvider::TreeSitter,
            Confidence::certain(),
            declaration_range,
        ))?;

        let visibility = self.visibility(frame.node, frame.scope);
        if visibility == SymbolVisibility::Public {
            self.push_relation(SyntaxRelation::new(
                SyntaxSource::Symbol(frame.container),
                SyntaxTarget::Symbol(id),
                SyntaxRelationKind::Exports,
                SyntaxProvider::TreeSitter,
                Confidence::certain(),
                selection_range,
            ))?;
        }
        if frame.node.kind() == "mod_item" && frame.node.child_by_field_name("body").is_none() {
            self.push_unresolved_relation(
                SyntaxSource::Symbol(frame.container),
                symbol_name_text(selection_node, self.input.source())?,
                SyntaxRelationKind::Imports,
                selection_range,
                Confidence::certain(),
            )?;
        }
        if frame.node.kind() == "impl_item" {
            self.add_impl_relation(frame.node, id)?;
        }
        if frame.node.kind() == "trait_item" {
            self.add_trait_bounds(frame.node, id)?;
        }

        let callable = if matches!(
            frame.node.kind(),
            "function_item" | "function_signature_item"
        ) {
            Some(id)
        } else {
            frame.callable
        };
        Ok(Some(SymbolVisit {
            id,
            scope: next_scope,
            callable,
        }))
    }

    fn symbol_name<'tree>(
        &mut self,
        node: Node<'tree>,
    ) -> Result<Option<(SymbolName, Node<'tree>)>, LanguageParseFailure> {
        let selection = if node.kind() == "impl_item" {
            node.child_by_field_name("type")
        } else {
            node.child_by_field_name("name")
        };
        let Some(selection) = selection else {
            self.push_diagnostic(diagnostic(
                ParseDiagnosticCode::UnsupportedSyntax,
                source_range_for_node(node)?,
                "Rust declaration has no supported name",
            )?)?;
            return Ok(None);
        };
        let text = if node.kind() == "impl_item" {
            let type_text = normalized_node_text(self.input.source(), selection);
            let trait_text = node
                .child_by_field_name("trait")
                .and_then(|value| normalized_node_text(self.input.source(), value));
            match (trait_text, type_text) {
                (Some(trait_name), Some(type_name)) => {
                    Some(format!("{trait_name} for {type_name}"))
                }
                (None, Some(type_name)) => Some(type_name),
                _ => None,
            }
        } else {
            normalized_node_text(self.input.source(), selection)
        };
        let Some(text) = text else {
            self.push_diagnostic(diagnostic(
                ParseDiagnosticCode::InvalidEncoding,
                source_range_for_node(selection)?,
                "Rust symbol name is not valid UTF-8",
            )?)?;
            return Ok(None);
        };
        match SymbolName::try_from_string(text) {
            Ok(name) => Ok(Some((name, selection))),
            Err(_) => {
                self.push_diagnostic(diagnostic(
                    ParseDiagnosticCode::OutputTruncated,
                    source_range_for_node(selection)?,
                    "Rust symbol name exceeds the adapter contract",
                )?)?;
                Ok(None)
            }
        }
    }

    fn signature(
        &mut self,
        node: Node<'_>,
    ) -> Result<Option<SymbolSignature>, LanguageParseFailure> {
        let end = node
            .child_by_field_name("body")
            .map_or(node.end_byte(), |body| body.start_byte());
        let Some(bytes) = self.input.source().get(node.start_byte()..end) else {
            return Err(LanguageParseFailure::InvalidResult);
        };
        let Ok(text) = std::str::from_utf8(bytes) else {
            self.push_diagnostic(diagnostic(
                ParseDiagnosticCode::InvalidEncoding,
                source_range_for_node(node)?,
                "Rust declaration signature is not valid UTF-8",
            )?)?;
            return Ok(None);
        };
        let signature = text.trim().trim_end_matches('=').trim_end().to_owned();
        if signature.is_empty() {
            return Ok(None);
        }
        match SymbolSignature::try_from_string(signature) {
            Ok(signature) => Ok(Some(signature)),
            Err(_) => {
                self.push_diagnostic(diagnostic(
                    ParseDiagnosticCode::OutputTruncated,
                    source_range_for_node(node)?,
                    "Rust declaration signature exceeds the adapter contract",
                )?)?;
                Ok(None)
            }
        }
    }

    fn visibility(&self, node: Node<'_>, parent_scope: RustScope) -> SymbolVisibility {
        if matches!(parent_scope, RustScope::Trait | RustScope::Enum) {
            return SymbolVisibility::Public;
        }
        let visibility = named_child_of_kind(node, "visibility_modifier")
            .and_then(|value| node_text(self.input.source(), value));
        match visibility.map(str::trim) {
            Some("pub") => SymbolVisibility::Public,
            Some(_) => SymbolVisibility::Internal,
            None => SymbolVisibility::Private,
        }
    }

    fn decorations(&self, node: Node<'_>) -> Result<Decorations, LanguageParseFailure> {
        let mut previous = node.prev_named_sibling();
        let mut documentation_start = None;
        let mut documentation_end = None;
        let mut is_test = false;
        while let Some(candidate) = previous {
            match candidate.kind() {
                "attribute_item" => {
                    if let Some(text) = node_text(self.input.source(), candidate) {
                        let attribute = attribute_name(text);
                        is_test |= is_test_attribute(text, attribute.as_deref());
                        if attribute.as_deref() == Some("doc") {
                            documentation_start = Some(candidate.start_byte());
                            documentation_end.get_or_insert(candidate.end_byte());
                        }
                    }
                }
                "line_comment" | "block_comment" => {
                    if node_text(self.input.source(), candidate).is_some_and(is_doc_comment) {
                        documentation_start = Some(candidate.start_byte());
                        documentation_end.get_or_insert(candidate.end_byte());
                    } else {
                        break;
                    }
                }
                _ => break,
            }
            previous = candidate.prev_named_sibling();
        }
        let documentation_range = match (documentation_start, documentation_end) {
            (Some(start), Some(end)) => Some(super::source::range_for_offsets(
                self.input.source(),
                start,
                end,
            )?),
            _ => None,
        };
        Ok(Decorations {
            documentation_range,
            is_test,
        })
    }

    fn add_use_relations(&mut self, frame: Frame<'_>) -> Result<(), LanguageParseFailure> {
        let argument = frame
            .node
            .child_by_field_name("argument")
            .ok_or(LanguageParseFailure::InvalidResult)?;
        let Some(reference) = normalized_node_text(self.input.source(), argument) else {
            self.push_diagnostic(diagnostic(
                ParseDiagnosticCode::InvalidEncoding,
                source_range_for_node(argument)?,
                "Rust use path is not valid UTF-8",
            )?)?;
            return Ok(());
        };
        let source = SyntaxSource::Symbol(frame.callable.unwrap_or(frame.container));
        let range = source_range_for_node(argument)?;
        self.push_unresolved_relation(
            source,
            reference.clone(),
            SyntaxRelationKind::Imports,
            range,
            Confidence::certain(),
        )?;
        if named_child_of_kind(frame.node, "visibility_modifier").is_some() {
            self.push_unresolved_relation(
                source,
                reference,
                SyntaxRelationKind::Exports,
                range,
                Confidence::certain(),
            )?;
        }
        Ok(())
    }

    fn add_call_relation(&mut self, frame: Frame<'_>) -> Result<(), LanguageParseFailure> {
        let function = frame
            .node
            .child_by_field_name("function")
            .ok_or(LanguageParseFailure::InvalidResult)?;
        if !matches!(
            function.kind(),
            "identifier" | "scoped_identifier" | "field_expression" | "generic_function"
        ) {
            self.push_diagnostic(warning(
                ParseDiagnosticCode::UnsupportedSyntax,
                source_range_for_node(function)?,
                "Rust call target form is not structurally supported",
            )?)?;
            return Ok(());
        }
        let Some(reference) = normalized_node_text(self.input.source(), function) else {
            self.push_diagnostic(diagnostic(
                ParseDiagnosticCode::InvalidEncoding,
                source_range_for_node(function)?,
                "Rust call target is not valid UTF-8",
            )?)?;
            return Ok(());
        };
        self.push_unresolved_relation(
            SyntaxSource::Symbol(frame.callable.unwrap_or(frame.container)),
            reference,
            SyntaxRelationKind::Calls,
            source_range_for_node(function)?,
            Confidence::from_basis_points(CALL_CONFIDENCE_BASIS_POINTS)
                .map_err(|_| LanguageParseFailure::InvalidResult)?,
        )
    }

    fn add_macro_call_relation(&mut self, frame: Frame<'_>) -> Result<(), LanguageParseFailure> {
        let Some(target) = frame.node.child_by_field_name("macro") else {
            return Ok(());
        };
        let Some(mut reference) = normalized_node_text(self.input.source(), target) else {
            return Ok(());
        };
        reference.push('!');
        self.push_unresolved_relation(
            SyntaxSource::Symbol(frame.callable.unwrap_or(frame.container)),
            reference,
            SyntaxRelationKind::Calls,
            source_range_for_node(target)?,
            Confidence::from_basis_points(CALL_CONFIDENCE_BASIS_POINTS)
                .map_err(|_| LanguageParseFailure::InvalidResult)?,
        )
    }

    fn mark_macro_tokens_incomplete(&mut self, node: Node<'_>) -> Result<(), LanguageParseFailure> {
        let range = match named_child_of_kind(node, "token_tree") {
            Some(tokens) => source_range_for_node(tokens)?,
            None => source_range_for_node(node)?,
        };
        self.push_diagnostic(warning(
            ParseDiagnosticCode::UnsupportedSyntax,
            range,
            "Rust macro token tree is not structurally expanded",
        )?)
    }

    fn add_impl_relation(
        &mut self,
        node: Node<'_>,
        implementation: LocalSymbolId,
    ) -> Result<(), LanguageParseFailure> {
        let Some(target) = node.child_by_field_name("trait") else {
            return Ok(());
        };
        let Some(reference) = normalized_node_text(self.input.source(), target) else {
            return Ok(());
        };
        self.push_unresolved_relation(
            SyntaxSource::Symbol(implementation),
            reference,
            SyntaxRelationKind::Implements,
            source_range_for_node(target)?,
            Confidence::certain(),
        )
    }

    fn add_trait_bounds(
        &mut self,
        node: Node<'_>,
        trait_id: LocalSymbolId,
    ) -> Result<(), LanguageParseFailure> {
        let Some(bounds) = node.child_by_field_name("bounds") else {
            return Ok(());
        };
        for index in 0..bounds.named_child_count() {
            let index =
                u32::try_from(index).map_err(|_| LanguageParseFailure::ResourceLimitExceeded)?;
            let bound = bounds
                .named_child(index)
                .ok_or(LanguageParseFailure::InvalidResult)?;
            if bound.kind() == "lifetime" {
                continue;
            }
            let Some(reference) = normalized_node_text(self.input.source(), bound) else {
                continue;
            };
            self.push_unresolved_relation(
                SyntaxSource::Symbol(trait_id),
                reference,
                SyntaxRelationKind::Extends,
                source_range_for_node(bound)?,
                Confidence::certain(),
            )?;
        }
        Ok(())
    }

    fn push_unresolved_relation(
        &mut self,
        source: SyntaxSource,
        reference: String,
        kind: SyntaxRelationKind,
        range: SourceRange,
        confidence: Confidence,
    ) -> Result<(), LanguageParseFailure> {
        let target = match SymbolReference::try_from_string(reference) {
            Ok(reference) => SyntaxTarget::Unresolved(reference),
            Err(_) => {
                self.push_diagnostic(diagnostic(
                    ParseDiagnosticCode::OutputTruncated,
                    range,
                    "Rust relation target exceeds the adapter contract",
                )?)?;
                return Ok(());
            }
        };
        self.push_relation(SyntaxRelation::new(
            source,
            target,
            kind,
            SyntaxProvider::TreeSitter,
            confidence,
            range,
        ))
    }

    fn push_children<'tree>(
        &self,
        node: Node<'tree>,
        container: LocalSymbolId,
        callable: Option<LocalSymbolId>,
        scope: RustScope,
        stack: &mut Vec<Frame<'tree>>,
    ) -> Result<(), LanguageParseFailure> {
        for index in (0..node.named_child_count()).rev() {
            let index =
                u32::try_from(index).map_err(|_| LanguageParseFailure::ResourceLimitExceeded)?;
            let child = node
                .named_child(index)
                .ok_or(LanguageParseFailure::InvalidResult)?;
            stack.push(Frame {
                node: child,
                container,
                callable,
                scope,
            });
        }
        Ok(())
    }

    fn take_symbol_id(&mut self) -> Result<LocalSymbolId, LanguageParseFailure> {
        let id = LocalSymbolId::new(self.next_symbol_id)
            .map_err(|_| LanguageParseFailure::ResourceLimitExceeded)?;
        self.next_symbol_id = self
            .next_symbol_id
            .checked_add(1)
            .ok_or(LanguageParseFailure::ResourceLimitExceeded)?;
        Ok(id)
    }

    fn push_symbol(&mut self, symbol: ParsedSymbol) -> Result<(), LanguageParseFailure> {
        if self.artifacts.symbols.len() >= self.policy.max_symbols() {
            return Err(LanguageParseFailure::ResourceLimitExceeded);
        }
        self.artifacts.symbols.push(symbol);
        Ok(())
    }

    fn push_relation(&mut self, relation: SyntaxRelation) -> Result<(), LanguageParseFailure> {
        if self.artifacts.relations.len() >= self.policy.max_relations() {
            return Err(LanguageParseFailure::ResourceLimitExceeded);
        }
        self.artifacts.relations.push(relation);
        Ok(())
    }

    fn push_diagnostic(&mut self, diagnostic: ParseDiagnostic) -> Result<(), LanguageParseFailure> {
        if self.artifacts.diagnostics.len() >= self.policy.max_diagnostics() {
            return Err(LanguageParseFailure::ResourceLimitExceeded);
        }
        self.artifacts.diagnostics.push(diagnostic);
        Ok(())
    }

    fn ensure_active(&self) -> Result<(), LanguageParseFailure> {
        if self.control.is_cancelled() {
            return Err(LanguageParseFailure::Cancelled);
        }
        if self.started.elapsed() >= self.policy.parse_timeout() {
            return Err(LanguageParseFailure::TimedOut);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
struct Decorations {
    documentation_range: Option<SourceRange>,
    is_test: bool,
}

fn symbol_kind(kind: &str, parent: RustScope) -> Option<(SymbolKind, RustScope)> {
    match kind {
        "mod_item" => Some((SymbolKind::Module, RustScope::Module)),
        "function_item" | "function_signature_item" => Some((
            if matches!(parent, RustScope::Trait | RustScope::Implementation) {
                SymbolKind::Method
            } else {
                SymbolKind::Function
            },
            RustScope::Function,
        )),
        "struct_item" => Some((SymbolKind::Struct, RustScope::Struct)),
        "enum_item" => Some((SymbolKind::Enum, RustScope::Enum)),
        "trait_item" => Some((SymbolKind::Trait, RustScope::Trait)),
        "impl_item" => Some((SymbolKind::Implementation, RustScope::Implementation)),
        "type_item" | "associated_type" => Some((SymbolKind::TypeAlias, RustScope::Other)),
        "const_item" => Some((SymbolKind::Constant, RustScope::Other)),
        "static_item" => Some((SymbolKind::Static, RustScope::Other)),
        "field_declaration" => Some((SymbolKind::Field, RustScope::Other)),
        "enum_variant" => Some((SymbolKind::Variant, RustScope::Other)),
        _ => None,
    }
}

fn named_child_of_kind<'tree>(node: Node<'tree>, kind: &str) -> Option<Node<'tree>> {
    (0..node.named_child_count()).find_map(|index| {
        u32::try_from(index)
            .ok()
            .and_then(|index| node.named_child(index))
            .filter(|child| child.kind() == kind)
    })
}

fn symbol_name_text(node: Node<'_>, source: &[u8]) -> Result<String, LanguageParseFailure> {
    normalized_node_text(source, node).ok_or(LanguageParseFailure::InvalidResult)
}

fn attribute_name(value: &str) -> Option<String> {
    let trimmed = value.trim();
    let content = trimmed
        .strip_prefix("#![")
        .or_else(|| trimmed.strip_prefix("#["))?
        .strip_suffix(']')?
        .trim();
    let end = content
        .find(|character: char| character == '(' || character == '=' || character.is_whitespace())
        .map_or(content.len(), |index| index);
    Some(content.get(..end)?.to_owned())
}

fn is_doc_comment(value: &str) -> bool {
    let trimmed = value.trim_start();
    (trimmed.starts_with("///") && !trimmed.starts_with("////"))
        || (trimmed.starts_with("/**") && !trimmed.starts_with("/***"))
}

fn is_test_attribute(value: &str, name: Option<&str>) -> bool {
    matches!(
        name,
        Some("test" | "tokio::test" | "async_std::test" | "rstest" | "proptest" | "test_case")
    ) || value
        .chars()
        .filter(|character| !character.is_whitespace())
        .eq("#[cfg(test)]".chars())
}

fn module_name(path: &RepositoryPath) -> Result<SymbolName, LanguageParseFailure> {
    let components = path
        .as_bytes()
        .split(|byte| *byte == b'/')
        .collect::<Vec<_>>();
    let last = components
        .last()
        .copied()
        .ok_or(LanguageParseFailure::InvalidResult)?;
    let selected = if last == b"mod.rs" && components.len() > 1 {
        components.get(components.len().saturating_sub(2)).copied()
    } else {
        last.strip_suffix(b".rs")
    };
    if let Some(name) = selected
        .and_then(|value| std::str::from_utf8(value).ok())
        .and_then(|value| SymbolName::try_from_string(value.to_owned()).ok())
    {
        return Ok(name);
    }
    let digest = blake3::hash(path.as_bytes());
    let fallback = format!("module-{}", digest.to_hex());
    SymbolName::try_from_string(fallback).map_err(|_| LanguageParseFailure::InvalidResult)
}

fn is_crate_entry_path(path: &RepositoryPath) -> bool {
    let bytes = path.as_bytes();
    bytes == b"build.rs"
        || bytes.ends_with(b"/build.rs")
        || bytes == b"src/lib.rs"
        || bytes.ends_with(b"/src/lib.rs")
        || is_binary_entry_path(path)
}

fn is_binary_entry_path(path: &RepositoryPath) -> bool {
    let bytes = path.as_bytes();
    if bytes == b"src/main.rs" || bytes.ends_with(b"/src/main.rs") {
        return true;
    }
    let marker = b"src/bin/";
    let Some(start) = bytes
        .windows(marker.len())
        .enumerate()
        .find_map(|(index, window)| {
            (window == marker && (index == 0 || bytes.get(index.saturating_sub(1)) == Some(&b'/')))
                .then_some(index)
        })
    else {
        return false;
    };
    let Some(suffix) = bytes.get(start.saturating_add(marker.len())..) else {
        return false;
    };
    if suffix.is_empty() {
        return false;
    }
    match suffix.iter().position(|byte| *byte == b'/') {
        None => suffix.ends_with(b".rs"),
        Some(separator) => {
            separator > 0
                && suffix
                    .get(separator.saturating_add(1)..)
                    .is_some_and(|tail| tail == b"main.rs")
        }
    }
}
