use super::source::{diagnostic, node_text, normalized_node_text, range_for_offsets, warning};
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
const DYNAMIC_CALL_CONFIDENCE_BASIS_POINTS: u16 = 5_000;
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
    let artifacts =
        TypeScriptJavaScriptExtractor::new(input, policy, control, diagnostics).extract(&tree)?;
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
enum JavaScriptScope {
    Module,
    Namespace,
    Class,
    Interface,
    Enum,
    Function,
    Other,
}

#[derive(Debug, Clone, Copy)]
struct Frame<'tree> {
    node: Node<'tree>,
    container: LocalSymbolId,
    callable: Option<LocalSymbolId>,
    scope: JavaScriptScope,
    exported: bool,
}

#[derive(Debug, Clone, Copy)]
struct SymbolVisit {
    id: LocalSymbolId,
    scope: JavaScriptScope,
    callable: Option<LocalSymbolId>,
}

struct TypeScriptJavaScriptExtractor<'a> {
    input: LanguageParseInput<'a>,
    policy: LanguageParsePolicy,
    control: &'a dyn LanguageParseControl,
    artifacts: LanguageParseArtifacts,
    next_symbol_id: u32,
    started: Instant,
}

impl<'a> TypeScriptJavaScriptExtractor<'a> {
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
        self.push_children(
            root,
            root_id,
            None,
            JavaScriptScope::Module,
            false,
            &mut stack,
        )?;
        let mut visited = 0usize;
        while let Some(frame) = stack.pop() {
            visited = visited
                .checked_add(1)
                .filter(|value| *value <= self.policy.max_tree_nodes())
                .ok_or(LanguageParseFailure::ResourceLimitExceeded)?;
            if visited.is_multiple_of(EXTRACTION_POLL_INTERVAL) {
                self.ensure_active()?;
            }

            if frame.node.kind() == "comment" {
                continue;
            }
            if frame.node.kind() == "import_statement" {
                self.add_import_statement(frame)?;
                continue;
            }
            if frame.node.kind() == "export_statement" {
                self.add_export_statement_relations(frame)?;
                let visit = self.add_anonymous_default_export(frame)?;
                let exported = visit.is_none();
                let (container, callable, scope) = visit
                    .map_or((frame.container, frame.callable, frame.scope), |visit| {
                        (visit.id, visit.callable, visit.scope)
                    });
                self.push_children(frame.node, container, callable, scope, exported, &mut stack)?;
                continue;
            }

            let visit = if frame.node.kind() == "call_expression" {
                self.add_test_framework_symbol(frame)?
            } else {
                self.add_symbol(frame)?
            };
            let (container, callable, scope, exported) = match visit {
                Some(visit) => (visit.id, visit.callable, visit.scope, false),
                None => (frame.container, frame.callable, frame.scope, frame.exported),
            };

            match frame.node.kind() {
                "call_expression" => {
                    self.add_call_relation(frame)?;
                    self.add_module_loader_relation(frame)?;
                }
                "new_expression" => self.add_constructor_relation(frame)?,
                "assignment_expression" => self.add_commonjs_export_relation(frame)?,
                _ => {}
            }
            self.push_children(frame.node, container, callable, scope, exported, &mut stack)?;
        }
        self.ensure_active()?;
        Ok(self.artifacts)
    }

    fn add_root_module(&mut self, root: Node<'_>) -> Result<LocalSymbolId, LanguageParseFailure> {
        let id = self.take_symbol_id()?;
        let declaration_range = source_range_for_node(root)?;
        let selection_range = range_for_offsets(self.input.source(), 0, 0)?;
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
        if is_source_entry_path(self.input.revision().path()) {
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
        let Some(descriptor) = self.symbol_descriptor(frame)? else {
            return Ok(None);
        };
        let id = self.take_symbol_id()?;
        let declaration_range = source_range_for_node(frame.node)?;
        let selection_range = source_range_for_node(descriptor.selection)?;
        let mut symbol = ParsedSymbol::new(
            id,
            descriptor.kind,
            descriptor.name,
            declaration_range,
            selection_range,
        )
        .map_err(|_| LanguageParseFailure::InvalidResult)?
        .with_visibility(self.visibility(frame));
        if let Some(signature) = self.signature(frame.node)? {
            symbol = symbol.with_signature(signature);
        }
        if let Some(documentation) = self.documentation_range(frame.node)? {
            symbol = symbol.with_documentation_range(documentation);
        }
        if self.is_test_declaration(frame.node, descriptor.kind) {
            symbol = symbol.with_role(SymbolRole::Test);
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
        if frame.exported {
            self.push_relation(SyntaxRelation::new(
                SyntaxSource::Symbol(frame.container),
                SyntaxTarget::Symbol(id),
                SyntaxRelationKind::Exports,
                SyntaxProvider::TreeSitter,
                Confidence::certain(),
                selection_range,
            ))?;
        }
        self.add_heritage_relations(frame.node, id)?;

        let callable = if matches!(descriptor.kind, SymbolKind::Function | SymbolKind::Method) {
            Some(id)
        } else {
            frame.callable
        };
        Ok(Some(SymbolVisit {
            id,
            scope: descriptor.scope,
            callable,
        }))
    }

    fn symbol_descriptor<'tree>(
        &mut self,
        frame: Frame<'tree>,
    ) -> Result<Option<SymbolDescriptor<'tree>>, LanguageParseFailure> {
        if frame.scope == JavaScriptScope::Enum
            && frame
                .node
                .parent()
                .is_some_and(|parent| parent.kind() == "enum_body")
            && matches!(
                frame.node.kind(),
                "identifier" | "property_identifier" | "string" | "number"
            )
        {
            return self.descriptor_from_selection(
                frame.node,
                SymbolKind::Variant,
                JavaScriptScope::Other,
            );
        }
        let (kind, scope, selection) = match frame.node.kind() {
            "function_declaration" | "generator_function_declaration" | "function_signature" => (
                SymbolKind::Function,
                JavaScriptScope::Function,
                frame.node.child_by_field_name("name"),
            ),
            "function_expression" | "generator_function" => {
                if frame.node.parent().is_some_and(|parent| {
                    matches!(
                        parent.kind(),
                        "variable_declarator" | "pair" | "export_statement"
                    )
                }) {
                    return Ok(None);
                }
                (
                    SymbolKind::Function,
                    JavaScriptScope::Function,
                    frame.node.child_by_field_name("name"),
                )
            }
            "class_declaration" | "abstract_class_declaration" => (
                SymbolKind::Class,
                JavaScriptScope::Class,
                frame.node.child_by_field_name("name"),
            ),
            "class" => {
                if frame.node.parent().is_some_and(|parent| {
                    matches!(parent.kind(), "variable_declarator" | "export_statement")
                }) {
                    return Ok(None);
                }
                (
                    SymbolKind::Class,
                    JavaScriptScope::Class,
                    frame.node.child_by_field_name("name"),
                )
            }
            "interface_declaration" => (
                SymbolKind::Interface,
                JavaScriptScope::Interface,
                frame.node.child_by_field_name("name"),
            ),
            "type_alias_declaration" => (
                SymbolKind::TypeAlias,
                JavaScriptScope::Other,
                frame.node.child_by_field_name("name"),
            ),
            "enum_declaration" => (
                SymbolKind::Enum,
                JavaScriptScope::Enum,
                frame.node.child_by_field_name("name"),
            ),
            "enum_assignment" => (
                SymbolKind::Variant,
                JavaScriptScope::Other,
                frame.node.child_by_field_name("name"),
            ),
            "method_definition" | "method_signature" | "abstract_method_signature" => (
                SymbolKind::Method,
                JavaScriptScope::Function,
                frame.node.child_by_field_name("name"),
            ),
            "public_field_definition" | "property_signature" => (
                SymbolKind::Field,
                JavaScriptScope::Other,
                frame.node.child_by_field_name("name"),
            ),
            "internal_module" | "module" => (
                SymbolKind::Namespace,
                JavaScriptScope::Namespace,
                frame.node.child_by_field_name("name"),
            ),
            "variable_declarator" => {
                let selection = frame.node.child_by_field_name("name");
                let value = frame.node.child_by_field_name("value");
                let (kind, scope) = match value.map(|value| value.kind()) {
                    Some("arrow_function" | "function_expression" | "generator_function") => {
                        (SymbolKind::Function, JavaScriptScope::Function)
                    }
                    Some("class") => (SymbolKind::Class, JavaScriptScope::Class),
                    _ if variable_is_const(frame.node) => {
                        (SymbolKind::Constant, JavaScriptScope::Other)
                    }
                    _ => (SymbolKind::Variable, JavaScriptScope::Other),
                };
                (kind, scope, selection)
            }
            "pair" => {
                let Some(value) = frame.node.child_by_field_name("value") else {
                    return Ok(None);
                };
                if !matches!(
                    value.kind(),
                    "arrow_function" | "function_expression" | "generator_function"
                ) {
                    return Ok(None);
                }
                (
                    SymbolKind::Method,
                    JavaScriptScope::Function,
                    frame.node.child_by_field_name("key"),
                )
            }
            _ => return Ok(None),
        };
        let Some(selection) = selection else {
            if frame
                .node
                .parent()
                .is_some_and(|parent| parent.kind() == "export_statement")
            {
                return Ok(None);
            }
            self.push_diagnostic(warning(
                ParseDiagnosticCode::UnsupportedSyntax,
                source_range_for_node(frame.node)?,
                "TS/JS declaration has no stable supported name",
            )?)?;
            return Ok(None);
        };
        self.descriptor_from_selection(selection, kind, scope)
    }

    fn descriptor_from_selection<'tree>(
        &mut self,
        selection: Node<'tree>,
        kind: SymbolKind,
        scope: JavaScriptScope,
    ) -> Result<Option<SymbolDescriptor<'tree>>, LanguageParseFailure> {
        if selection.kind() == "computed_property_name" {
            self.push_diagnostic(warning(
                ParseDiagnosticCode::UnsupportedSyntax,
                source_range_for_node(selection)?,
                "Computed TS/JS declaration name is not statically stable",
            )?)?;
            return Ok(None);
        }
        if !matches!(
            selection.kind(),
            "identifier"
                | "property_identifier"
                | "private_property_identifier"
                | "type_identifier"
                | "string"
                | "number"
        ) {
            self.push_diagnostic(warning(
                ParseDiagnosticCode::UnsupportedSyntax,
                source_range_for_node(selection)?,
                "Destructured TS/JS declaration has no single stable symbol name",
            )?)?;
            return Ok(None);
        }
        let Some(name) = source_symbol_name(self.input.source(), selection) else {
            self.push_diagnostic(diagnostic(
                ParseDiagnosticCode::InvalidEncoding,
                source_range_for_node(selection)?,
                "TS/JS symbol name is not valid UTF-8",
            )?)?;
            return Ok(None);
        };
        match SymbolName::try_from_string(name) {
            Ok(name) => Ok(Some(SymbolDescriptor {
                kind,
                scope,
                name,
                selection,
            })),
            Err(_) => {
                self.push_diagnostic(warning(
                    ParseDiagnosticCode::OutputTruncated,
                    source_range_for_node(selection)?,
                    "TS/JS symbol name exceeds the adapter contract",
                )?)?;
                Ok(None)
            }
        }
    }

    fn add_test_framework_symbol(
        &mut self,
        frame: Frame<'_>,
    ) -> Result<Option<SymbolVisit>, LanguageParseFailure> {
        let function = frame
            .node
            .child_by_field_name("function")
            .ok_or(LanguageParseFailure::InvalidResult)?;
        let Some(function_text) = normalized_node_text(self.input.source(), function) else {
            return Ok(None);
        };
        let Some(test_kind) = test_framework_kind(&function_text) else {
            return Ok(None);
        };
        let Some(arguments) = frame.node.child_by_field_name("arguments") else {
            return Ok(None);
        };
        let Some(title) = arguments.named_child(0) else {
            return Ok(None);
        };
        let Some((name, selection_range)) = test_title(self.input.source(), title)? else {
            return Ok(None);
        };
        let id = self.take_symbol_id()?;
        let declaration_range = source_range_for_node(frame.node)?;
        let symbol = ParsedSymbol::new(
            id,
            test_kind.symbol_kind(),
            name,
            declaration_range,
            selection_range,
        )
        .map_err(|_| LanguageParseFailure::InvalidResult)?
        .with_visibility(SymbolVisibility::Local)
        .with_role(SymbolRole::Test);
        self.push_symbol(symbol)?;
        self.push_relation(SyntaxRelation::new(
            SyntaxSource::Symbol(frame.container),
            SyntaxTarget::Symbol(id),
            SyntaxRelationKind::Contains,
            SyntaxProvider::LanguageHeuristic,
            test_kind.confidence()?,
            declaration_range,
        ))?;
        Ok(Some(SymbolVisit {
            id,
            scope: test_kind.scope(),
            callable: if test_kind == TestFrameworkKind::Case {
                Some(id)
            } else {
                frame.callable
            },
        }))
    }

    fn add_anonymous_default_export(
        &mut self,
        frame: Frame<'_>,
    ) -> Result<Option<SymbolVisit>, LanguageParseFailure> {
        let Some(value) = frame.node.child_by_field_name("value") else {
            return Ok(None);
        };
        let (kind, scope) = match value.kind() {
            "arrow_function" | "function_expression" | "generator_function" => {
                (SymbolKind::Function, JavaScriptScope::Function)
            }
            "class" => (SymbolKind::Class, JavaScriptScope::Class),
            _ => return Ok(None),
        };
        if value.child_by_field_name("name").is_some() {
            return Ok(None);
        }
        let selection = child_of_kind(frame.node, "default").unwrap_or(value);
        let id = self.take_symbol_id()?;
        let declaration_range = source_range_for_node(frame.node)?;
        let selection_range = source_range_for_node(selection)?;
        let mut symbol = ParsedSymbol::new(
            id,
            kind,
            SymbolName::try_from_string("default".to_owned())
                .map_err(|_| LanguageParseFailure::InvalidResult)?,
            declaration_range,
            selection_range,
        )
        .map_err(|_| LanguageParseFailure::InvalidResult)?
        .with_visibility(SymbolVisibility::Public);
        if let Some(signature) = self.signature(value)? {
            symbol = symbol.with_signature(signature);
        }
        if let Some(documentation) = self.documentation_range(frame.node)? {
            symbol = symbol.with_documentation_range(documentation);
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
        self.push_relation(SyntaxRelation::new(
            SyntaxSource::Symbol(frame.container),
            SyntaxTarget::Symbol(id),
            SyntaxRelationKind::Exports,
            SyntaxProvider::TreeSitter,
            Confidence::certain(),
            selection_range,
        ))?;
        Ok(Some(SymbolVisit {
            id,
            scope,
            callable: if kind == SymbolKind::Function {
                Some(id)
            } else {
                frame.callable
            },
        }))
    }

    fn signature(
        &mut self,
        node: Node<'_>,
    ) -> Result<Option<SymbolSignature>, LanguageParseFailure> {
        let end = node
            .child_by_field_name("body")
            .or_else(|| {
                matches!(node.kind(), "enum_declaration" | "interface_declaration")
                    .then(|| named_child_of_kind(node, body_kind(node.kind())))
                    .flatten()
            })
            .or_else(|| {
                matches!(
                    node.kind(),
                    "variable_declarator" | "public_field_definition" | "enum_assignment" | "pair"
                )
                .then(|| node.child_by_field_name("value"))
                .flatten()
            })
            .map_or(node.end_byte(), |body| body.start_byte());
        let Some(bytes) = self.input.source().get(node.start_byte()..end) else {
            return Err(LanguageParseFailure::InvalidResult);
        };
        let Ok(text) = std::str::from_utf8(bytes) else {
            self.push_diagnostic(diagnostic(
                ParseDiagnosticCode::InvalidEncoding,
                source_range_for_node(node)?,
                "TS/JS declaration signature is not valid UTF-8",
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
                self.push_diagnostic(warning(
                    ParseDiagnosticCode::OutputTruncated,
                    source_range_for_node(node)?,
                    "TS/JS declaration signature exceeds the adapter contract",
                )?)?;
                Ok(None)
            }
        }
    }

    fn visibility(&self, frame: Frame<'_>) -> SymbolVisibility {
        if frame.exported {
            return SymbolVisibility::Public;
        }
        if matches!(
            frame.scope,
            JavaScriptScope::Interface | JavaScriptScope::Enum
        ) {
            return SymbolVisibility::Public;
        }
        if frame.scope == JavaScriptScope::Class {
            if frame
                .node
                .child_by_field_name("name")
                .is_some_and(|name| name.kind() == "private_property_identifier")
            {
                return SymbolVisibility::Private;
            }
            let modifier = named_child_of_kind(frame.node, "accessibility_modifier")
                .and_then(|value| node_text(self.input.source(), value));
            return match modifier.map(str::trim) {
                Some("private") => SymbolVisibility::Private,
                Some("protected") => SymbolVisibility::Protected,
                _ => SymbolVisibility::Public,
            };
        }
        if frame.scope == JavaScriptScope::Function {
            return SymbolVisibility::Local;
        }
        SymbolVisibility::Internal
    }

    fn documentation_range(
        &self,
        node: Node<'_>,
    ) -> Result<Option<SourceRange>, LanguageParseFailure> {
        let anchor = decoration_anchor(node);
        let mut previous = anchor.prev_named_sibling();
        while let Some(candidate) = previous {
            if candidate.kind() != "comment" {
                break;
            }
            let Some(text) = node_text(self.input.source(), candidate) else {
                break;
            };
            if is_jsdoc_comment(text) {
                return Ok(Some(source_range_for_node(candidate)?));
            }
            previous = candidate.prev_named_sibling();
        }
        Ok(None)
    }

    fn is_test_declaration(&self, node: Node<'_>, kind: SymbolKind) -> bool {
        if !matches!(kind, SymbolKind::Function | SymbolKind::Method) {
            return false;
        }
        let Some(name) = node
            .child_by_field_name("name")
            .and_then(|selection| source_symbol_name(self.input.source(), selection))
        else {
            return false;
        };
        self.input
            .discovery_roles()
            .contains(DiscoveredFileRole::Test)
            && (name == "test" || name.starts_with("test_") || name.ends_with("Test"))
    }

    fn add_import_statement(&mut self, frame: Frame<'_>) -> Result<(), LanguageParseFailure> {
        let source = frame
            .node
            .child_by_field_name("source")
            .or_else(|| {
                named_child_of_kind(frame.node, "import_require_clause")
                    .and_then(|clause| clause.child_by_field_name("source"))
            })
            .ok_or(LanguageParseFailure::InvalidResult)?;
        let Some(reference) = source_literal(self.input.source(), source) else {
            let range = source_range_for_node(source)?;
            let diagnostic = if node_text(self.input.source(), source).is_some() {
                warning(
                    ParseDiagnosticCode::UnsupportedSyntax,
                    range,
                    "TS/JS import specifier is not a supported string literal",
                )?
            } else {
                diagnostic(
                    ParseDiagnosticCode::InvalidEncoding,
                    range,
                    "TS/JS import specifier is not valid UTF-8",
                )?
            };
            self.push_diagnostic(diagnostic)?;
            return Ok(());
        };
        self.push_unresolved_relation(
            SyntaxSource::Symbol(frame.callable.unwrap_or(frame.container)),
            reference,
            SyntaxRelationKind::Imports,
            source_range_for_node(source)?,
            SyntaxProvider::TreeSitter,
            Confidence::certain(),
        )
    }

    fn add_export_statement_relations(
        &mut self,
        frame: Frame<'_>,
    ) -> Result<(), LanguageParseFailure> {
        if let Some(source) = frame.node.child_by_field_name("source")
            && let Some(reference) = source_literal(self.input.source(), source)
        {
            let range = source_range_for_node(source)?;
            self.push_unresolved_relation(
                SyntaxSource::Symbol(frame.container),
                reference.clone(),
                SyntaxRelationKind::Imports,
                range,
                SyntaxProvider::TreeSitter,
                Confidence::certain(),
            )?;
            self.push_unresolved_relation(
                SyntaxSource::Symbol(frame.container),
                reference,
                SyntaxRelationKind::Exports,
                range,
                SyntaxProvider::TreeSitter,
                Confidence::certain(),
            )?;
        }
        if let Some(clause) = named_child_of_kind(frame.node, "export_clause") {
            for index in 0..clause.named_child_count() {
                let index = u32::try_from(index)
                    .map_err(|_| LanguageParseFailure::ResourceLimitExceeded)?;
                let specifier = clause
                    .named_child(index)
                    .ok_or(LanguageParseFailure::InvalidResult)?;
                if specifier.kind() != "export_specifier" {
                    continue;
                }
                let selection = specifier
                    .child_by_field_name("alias")
                    .or_else(|| specifier.child_by_field_name("name"))
                    .ok_or(LanguageParseFailure::InvalidResult)?;
                if let Some(reference) = normalized_node_text(self.input.source(), selection) {
                    self.push_unresolved_relation(
                        SyntaxSource::Symbol(frame.container),
                        reference,
                        SyntaxRelationKind::Exports,
                        source_range_for_node(selection)?,
                        SyntaxProvider::TreeSitter,
                        Confidence::certain(),
                    )?;
                }
            }
        }
        if let Some(namespace) = named_child_of_kind(frame.node, "namespace_export")
            && let Some(name) = namespace.named_child(0)
            && let Some(reference) = normalized_node_text(self.input.source(), name)
        {
            self.push_unresolved_relation(
                SyntaxSource::Symbol(frame.container),
                reference,
                SyntaxRelationKind::Exports,
                source_range_for_node(name)?,
                SyntaxProvider::TreeSitter,
                Confidence::certain(),
            )?;
        }
        if let Some(value) = frame.node.child_by_field_name("value")
            && !matches!(
                value.kind(),
                "arrow_function" | "function_expression" | "generator_function" | "class"
            )
            && let Some(reference) = normalized_node_text(self.input.source(), value)
        {
            self.push_unresolved_relation(
                SyntaxSource::Symbol(frame.container),
                reference,
                SyntaxRelationKind::Exports,
                source_range_for_node(value)?,
                SyntaxProvider::TreeSitter,
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
        let Some((reference, confidence)) = call_reference(self.input.source(), function)? else {
            self.push_diagnostic(warning(
                ParseDiagnosticCode::UnsupportedSyntax,
                source_range_for_node(function)?,
                "Dynamic TS/JS call target is not structurally stable",
            )?)?;
            return Ok(());
        };
        self.push_unresolved_relation(
            SyntaxSource::Symbol(frame.callable.unwrap_or(frame.container)),
            reference,
            SyntaxRelationKind::Calls,
            source_range_for_node(function)?,
            SyntaxProvider::TreeSitter,
            confidence,
        )
    }

    fn add_constructor_relation(&mut self, frame: Frame<'_>) -> Result<(), LanguageParseFailure> {
        let constructor = frame
            .node
            .child_by_field_name("constructor")
            .ok_or(LanguageParseFailure::InvalidResult)?;
        let Some((reference, confidence)) = call_reference(self.input.source(), constructor)?
        else {
            self.push_diagnostic(warning(
                ParseDiagnosticCode::UnsupportedSyntax,
                source_range_for_node(constructor)?,
                "Dynamic TS/JS constructor target is not structurally stable",
            )?)?;
            return Ok(());
        };
        self.push_unresolved_relation(
            SyntaxSource::Symbol(frame.callable.unwrap_or(frame.container)),
            reference,
            SyntaxRelationKind::Calls,
            source_range_for_node(constructor)?,
            SyntaxProvider::TreeSitter,
            confidence,
        )
    }

    fn add_module_loader_relation(&mut self, frame: Frame<'_>) -> Result<(), LanguageParseFailure> {
        let function = frame
            .node
            .child_by_field_name("function")
            .ok_or(LanguageParseFailure::InvalidResult)?;
        let Some(name) = normalized_node_text(self.input.source(), function) else {
            return Ok(());
        };
        if !matches!(name.as_str(), "require" | "import") {
            return Ok(());
        }
        let Some(arguments) = frame.node.child_by_field_name("arguments") else {
            return Ok(());
        };
        let Some(source) = arguments.named_child(0) else {
            return Ok(());
        };
        let Some(reference) = source_literal(self.input.source(), source) else {
            self.push_diagnostic(warning(
                ParseDiagnosticCode::UnsupportedSyntax,
                source_range_for_node(source)?,
                "Dynamic TS/JS module loader specifier is not structurally stable",
            )?)?;
            return Ok(());
        };
        self.push_unresolved_relation(
            SyntaxSource::Symbol(frame.callable.unwrap_or(frame.container)),
            reference,
            SyntaxRelationKind::Imports,
            source_range_for_node(source)?,
            SyntaxProvider::LanguageHeuristic,
            Confidence::certain(),
        )
    }

    fn add_commonjs_export_relation(
        &mut self,
        frame: Frame<'_>,
    ) -> Result<(), LanguageParseFailure> {
        let left = frame
            .node
            .child_by_field_name("left")
            .ok_or(LanguageParseFailure::InvalidResult)?;
        let Some(reference) = normalized_node_text(self.input.source(), left) else {
            return Ok(());
        };
        if reference != "module.exports"
            && !reference.starts_with("module.exports.")
            && !reference.starts_with("module.exports[")
            && !reference.starts_with("exports.")
            && !reference.starts_with("exports[")
        {
            return Ok(());
        }
        self.push_unresolved_relation(
            SyntaxSource::Symbol(frame.container),
            reference,
            SyntaxRelationKind::Exports,
            source_range_for_node(left)?,
            SyntaxProvider::LanguageHeuristic,
            Confidence::certain(),
        )
    }

    fn add_heritage_relations(
        &mut self,
        node: Node<'_>,
        symbol: LocalSymbolId,
    ) -> Result<(), LanguageParseFailure> {
        if matches!(
            node.kind(),
            "class_declaration" | "abstract_class_declaration" | "class"
        ) && let Some(heritage) = named_child_of_kind(node, "class_heritage")
        {
            for index in 0..heritage.named_child_count() {
                let index = u32::try_from(index)
                    .map_err(|_| LanguageParseFailure::ResourceLimitExceeded)?;
                let child = heritage
                    .named_child(index)
                    .ok_or(LanguageParseFailure::InvalidResult)?;
                match child.kind() {
                    "extends_clause" => {
                        if let Some(target) = child.child_by_field_name("value") {
                            self.add_heritage_target(symbol, target, SyntaxRelationKind::Extends)?;
                        }
                    }
                    "implements_clause" => {
                        self.add_named_children_as_heritage(
                            symbol,
                            child,
                            SyntaxRelationKind::Implements,
                        )?;
                    }
                    _ => self.add_heritage_target(symbol, child, SyntaxRelationKind::Extends)?,
                }
            }
        }
        if node.kind() == "interface_declaration"
            && let Some(extends) = named_child_of_kind(node, "extends_type_clause")
        {
            self.add_named_children_as_heritage(symbol, extends, SyntaxRelationKind::Extends)?;
        }
        Ok(())
    }

    fn add_named_children_as_heritage(
        &mut self,
        symbol: LocalSymbolId,
        node: Node<'_>,
        kind: SyntaxRelationKind,
    ) -> Result<(), LanguageParseFailure> {
        for index in 0..node.named_child_count() {
            let index =
                u32::try_from(index).map_err(|_| LanguageParseFailure::ResourceLimitExceeded)?;
            let child = node
                .named_child(index)
                .ok_or(LanguageParseFailure::InvalidResult)?;
            self.add_heritage_target(symbol, child, kind)?;
        }
        Ok(())
    }

    fn add_heritage_target(
        &mut self,
        symbol: LocalSymbolId,
        target: Node<'_>,
        kind: SyntaxRelationKind,
    ) -> Result<(), LanguageParseFailure> {
        let Some(reference) = normalized_node_text(self.input.source(), target) else {
            return Ok(());
        };
        self.push_unresolved_relation(
            SyntaxSource::Symbol(symbol),
            reference,
            kind,
            source_range_for_node(target)?,
            SyntaxProvider::TreeSitter,
            Confidence::certain(),
        )
    }

    fn push_unresolved_relation(
        &mut self,
        source: SyntaxSource,
        reference: String,
        kind: SyntaxRelationKind,
        range: SourceRange,
        provider: SyntaxProvider,
        confidence: Confidence,
    ) -> Result<(), LanguageParseFailure> {
        let target = match SymbolReference::try_from_string(reference) {
            Ok(reference) => SyntaxTarget::Unresolved(reference),
            Err(_) => {
                self.push_diagnostic(warning(
                    ParseDiagnosticCode::OutputTruncated,
                    range,
                    "TS/JS relation target exceeds the adapter contract",
                )?)?;
                return Ok(());
            }
        };
        self.push_relation(SyntaxRelation::new(
            source, target, kind, provider, confidence, range,
        ))
    }

    fn push_children<'tree>(
        &self,
        node: Node<'tree>,
        container: LocalSymbolId,
        callable: Option<LocalSymbolId>,
        scope: JavaScriptScope,
        exported: bool,
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
                exported,
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

struct SymbolDescriptor<'tree> {
    kind: SymbolKind,
    scope: JavaScriptScope,
    name: SymbolName,
    selection: Node<'tree>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TestFrameworkKind {
    Group,
    Case,
}

impl TestFrameworkKind {
    const fn symbol_kind(self) -> SymbolKind {
        match self {
            Self::Group => SymbolKind::Module,
            Self::Case => SymbolKind::Function,
        }
    }

    const fn scope(self) -> JavaScriptScope {
        match self {
            Self::Group => JavaScriptScope::Namespace,
            Self::Case => JavaScriptScope::Function,
        }
    }

    fn confidence(self) -> Result<Confidence, LanguageParseFailure> {
        Confidence::from_basis_points(match self {
            Self::Group => 8_500,
            Self::Case => 9_000,
        })
        .map_err(|_| LanguageParseFailure::InvalidResult)
    }
}

fn test_framework_kind(function: &str) -> Option<TestFrameworkKind> {
    let compact = function.replace("?.", ".");
    if compact == "Deno.test"
        || compact.starts_with("Deno.test.")
        || compact == "Bun.test"
        || compact.starts_with("Bun.test.")
    {
        return Some(TestFrameworkKind::Case);
    }
    let base = compact.split(['.', '(']).next().unwrap_or_default();
    match base {
        "describe" | "suite" | "context" => Some(TestFrameworkKind::Group),
        "test" | "it" | "specify" => Some(TestFrameworkKind::Case),
        _ => None,
    }
}

fn test_title(
    source: &[u8],
    node: Node<'_>,
) -> Result<Option<(SymbolName, SourceRange)>, LanguageParseFailure> {
    if !matches!(node.kind(), "string" | "template_string") {
        return Ok(None);
    }
    let Some(bytes) = source.get(node.byte_range()) else {
        return Err(LanguageParseFailure::InvalidResult);
    };
    if bytes.len() < 2
        || (node.kind() == "template_string"
            && (0..node.named_child_count()).any(|index| {
                u32::try_from(index)
                    .ok()
                    .and_then(|index| node.named_child(index))
                    .is_some_and(|child| child.kind() == "template_substitution")
            }))
    {
        return Ok(None);
    }
    let Some(content) = bytes.get(1..bytes.len().saturating_sub(1)) else {
        return Ok(None);
    };
    let content = if node.kind() == "string" {
        source_literal(source, node)
    } else {
        std::str::from_utf8(content).ok().map(str::to_owned)
    };
    let Some(content) = content else {
        return Ok(None);
    };
    let name = match SymbolName::try_from_string(content) {
        Ok(name) => name,
        Err(_) => return Ok(None),
    };
    let range = range_for_offsets(
        source,
        node.start_byte().saturating_add(1),
        node.end_byte().saturating_sub(1),
    )?;
    Ok(Some((name, range)))
}

fn call_reference(
    source: &[u8],
    node: Node<'_>,
) -> Result<Option<(String, Confidence)>, LanguageParseFailure> {
    let confidence = if node.kind() == "subscript_expression" {
        DYNAMIC_CALL_CONFIDENCE_BASIS_POINTS
    } else {
        CALL_CONFIDENCE_BASIS_POINTS
    };
    if matches!(
        node.kind(),
        "identifier" | "member_expression" | "subscript_expression" | "super" | "import" | "this"
    ) {
        let confidence = Confidence::from_basis_points(confidence)
            .map_err(|_| LanguageParseFailure::InvalidResult)?;
        return Ok(normalized_node_text(source, node).map(|reference| (reference, confidence)));
    }
    if node.kind() == "parenthesized_expression" && node.named_child_count() == 1 {
        let child = node
            .named_child(0)
            .ok_or(LanguageParseFailure::InvalidResult)?;
        return call_reference(source, child);
    }
    Ok(None)
}

fn variable_is_const(node: Node<'_>) -> bool {
    node.parent()
        .filter(|parent| parent.kind() == "lexical_declaration")
        .and_then(|parent| parent.child_by_field_name("kind"))
        .is_some_and(|kind| kind.kind() == "const")
}

fn decoration_anchor(mut node: Node<'_>) -> Node<'_> {
    while let Some(parent) = node.parent() {
        if matches!(
            parent.kind(),
            "export_statement" | "lexical_declaration" | "variable_declaration"
        ) {
            node = parent;
        } else {
            break;
        }
    }
    node
}

fn is_jsdoc_comment(value: &str) -> bool {
    let trimmed = value.trim_start();
    trimmed.starts_with("/**") && !trimmed.starts_with("/***")
}

fn body_kind(kind: &str) -> &str {
    match kind {
        "enum_declaration" => "enum_body",
        "interface_declaration" => "interface_body",
        _ => "",
    }
}

fn source_symbol_name(source: &[u8], node: Node<'_>) -> Option<String> {
    if node.kind() == "string" {
        return source_literal(source, node);
    }
    normalized_node_text(source, node)
}

fn source_literal(source: &[u8], node: Node<'_>) -> Option<String> {
    let bytes = source.get(node.byte_range())?;
    if node.kind() == "string" && bytes.first() == Some(&b'\"') {
        return serde_json::from_slice::<String>(bytes).ok();
    }
    if node.kind() == "string" && bytes.first() == Some(&b'\'') && bytes.last() == Some(&b'\'') {
        return decode_single_quoted_string(std::str::from_utf8(bytes).ok()?);
    }
    None
}

fn decode_single_quoted_string(value: &str) -> Option<String> {
    let inner = value.strip_prefix('\'')?.strip_suffix('\'')?;
    let mut json = String::with_capacity(inner.len().saturating_add(2));
    json.push('"');
    let mut characters = inner.chars().peekable();
    while let Some(character) = characters.next() {
        match character {
            '"' => json.push_str("\\\""),
            '\\' => {
                let next = characters.next()?;
                match next {
                    '\'' => json.push('\''),
                    '"' => json.push_str("\\\""),
                    _ => {
                        json.push('\\');
                        json.push(next);
                    }
                }
            }
            _ => json.push(character),
        }
    }
    json.push('"');
    serde_json::from_str(&json).ok()
}

fn named_child_of_kind<'tree>(node: Node<'tree>, kind: &str) -> Option<Node<'tree>> {
    (0..node.named_child_count()).find_map(|index| {
        u32::try_from(index)
            .ok()
            .and_then(|index| node.named_child(index))
            .filter(|child| child.kind() == kind)
    })
}

fn child_of_kind<'tree>(node: Node<'tree>, kind: &str) -> Option<Node<'tree>> {
    (0..node.child_count()).find_map(|index| {
        u32::try_from(index)
            .ok()
            .and_then(|index| node.child(index))
            .filter(|child| child.kind() == kind)
    })
}

fn module_name(path: &RepositoryPath) -> Result<SymbolName, LanguageParseFailure> {
    let name = path
        .as_bytes()
        .rsplit(|byte| *byte == b'/')
        .next()
        .ok_or(LanguageParseFailure::InvalidResult)?;
    let selected = [
        b".tsx".as_slice(),
        b".jsx",
        b".mts",
        b".cts",
        b".mjs",
        b".cjs",
        b".ts",
        b".js",
    ]
    .iter()
    .find_map(|extension| name.strip_suffix(*extension));
    if let Some(name) = selected
        .and_then(|value| std::str::from_utf8(value).ok())
        .and_then(|value| SymbolName::try_from_string(value.to_owned()).ok())
    {
        return Ok(name);
    }
    let digest = blake3::hash(path.as_bytes());
    SymbolName::try_from_string(format!("module-{}", digest.to_hex()))
        .map_err(|_| LanguageParseFailure::InvalidResult)
}

fn is_source_entry_path(path: &RepositoryPath) -> bool {
    let name = path
        .as_bytes()
        .rsplit(|byte| *byte == b'/')
        .next()
        .unwrap_or_default();
    [
        b"index.ts".as_slice(),
        b"index.tsx",
        b"index.js",
        b"index.jsx",
        b"index.mts",
        b"index.cts",
        b"index.mjs",
        b"index.cjs",
        b"main.ts",
        b"main.tsx",
        b"main.js",
        b"main.jsx",
        b"main.mts",
        b"main.cts",
        b"main.mjs",
        b"main.cjs",
    ]
    .contains(&name)
}
