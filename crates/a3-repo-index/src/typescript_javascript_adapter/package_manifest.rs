use super::source::{diagnostic, range_for_offsets, warning};
use crate::{TreeSitterParserPool, normalize_parse_diagnostics, source_range_for_node};
use a3_application::{
    LanguageParseControl, LanguageParseFailure, LanguageParseInput, LanguageParsePolicy,
};
use a3_domain::{
    Confidence, LanguageAdapterRevision, LanguageParseArtifacts, LanguageParseResult,
    LocalSymbolId, ParseDiagnostic, ParseDiagnosticCode, ParsedSymbol, RepositoryPath, SourceRange,
    SymbolKind, SymbolName, SymbolReference, SymbolRole, SymbolSignature, SymbolVisibility,
    SyntaxProvider, SyntaxRelation, SyntaxRelationKind, SyntaxSource, SyntaxTarget,
};
use std::collections::BTreeSet;
use std::time::Instant;
use tree_sitter::{Node, Tree};

const MAX_PACKAGE_MANIFEST_BYTES: usize = 512 * 1024;
const EXTRACTION_POLL_INTERVAL: usize = 128;

pub(super) fn parse(
    input: LanguageParseInput<'_>,
    policy: LanguageParsePolicy,
    control: &dyn LanguageParseControl,
    revision: &LanguageAdapterRevision,
    parser_pool: &TreeSitterParserPool,
) -> Result<LanguageParseResult, LanguageParseFailure> {
    if input.source().len() > MAX_PACKAGE_MANIFEST_BYTES {
        return Err(LanguageParseFailure::InputTooLarge);
    }
    let parsed = parser_pool.parse(input.source(), policy, control)?;
    let (tree, _parser_coverage, diagnostics) = parsed.into_parts();
    let (artifacts, flows) =
        PackageManifestExtractor::new(input, policy, control, diagnostics).extract(&tree)?;
    let (coverage, diagnostics) = normalize_parse_diagnostics(
        input.source().len(),
        policy.max_diagnostics(),
        artifacts.diagnostics,
    )?;
    LanguageParseResult::new(
        input.revision().clone(),
        revision.clone(),
        policy.contract_version(),
        coverage,
        LanguageParseArtifacts {
            diagnostics,
            ..artifacts
        },
    )
    .map_err(|_| LanguageParseFailure::InvalidResult)?
    .with_function_flows(flows)
    .map_err(|_| LanguageParseFailure::InvalidResult)
}

#[derive(Debug, Clone)]
struct JsonField<'tree> {
    name: String,
    pair: Node<'tree>,
    key: Node<'tree>,
    value: Node<'tree>,
}

struct PackageManifestExtractor<'a> {
    input: LanguageParseInput<'a>,
    policy: LanguageParsePolicy,
    control: &'a dyn LanguageParseControl,
    artifacts: LanguageParseArtifacts,
    flows: Vec<a3_domain::FunctionFlow>,
    next_symbol_id: u32,
    started: Instant,
    visited: usize,
}

impl<'a> PackageManifestExtractor<'a> {
    fn new(
        input: LanguageParseInput<'a>,
        policy: LanguageParsePolicy,
        control: &'a dyn LanguageParseControl,
        diagnostics: Vec<ParseDiagnostic>,
    ) -> Self {
        Self {
            input,
            flows: Vec::new(),
            policy,
            control,
            artifacts: LanguageParseArtifacts {
                diagnostics,
                ..LanguageParseArtifacts::default()
            },
            next_symbol_id: 1,
            started: Instant::now(),
            visited: 0,
        }
    }

    fn extract(
        mut self,
        tree: &Tree,
    ) -> Result<(LanguageParseArtifacts, Vec<a3_domain::FunctionFlow>), LanguageParseFailure> {
        self.ensure_active()?;
        let root = tree.root_node();
        let Some(object) = root.named_child(0).filter(|node| node.kind() == "object") else {
            if self.artifacts.diagnostics.is_empty() {
                self.push_diagnostic(warning(
                    ParseDiagnosticCode::UnsupportedSyntax,
                    source_range_for_node(root)?,
                    "package.json root must be a JSON object",
                )?)?;
            }
            return Ok((self.artifacts, self.flows));
        };
        let fields = self.object_fields(object)?;
        let name_field = fields.iter().find(|field| field.name == "name");
        let package_name = match name_field {
            Some(field) => match decode_json_string(self.input.source(), field.value) {
                Some(name) => name,
                None => {
                    let code =
                        if super::source::node_text(self.input.source(), field.value).is_some() {
                            ParseDiagnosticCode::UnsupportedSyntax
                        } else {
                            ParseDiagnosticCode::InvalidEncoding
                        };
                    self.push_diagnostic(warning(
                        code,
                        source_range_for_node(field.value)?,
                        "package.json name must be a supported JSON string",
                    )?)?;
                    "package".to_owned()
                }
            },
            None => "package".to_owned(),
        };
        let entrypoint = fields.iter().any(|field| match field.name.as_str() {
            "main" | "module" | "types" | "typings" => {
                decode_json_string(self.input.source(), field.value).is_some()
            }
            "browser" | "bin" => matches!(field.value.kind(), "string" | "object"),
            "exports" => matches!(field.value.kind(), "string" | "object" | "array"),
            _ => false,
        });
        let root_id = self.add_root_module(object, name_field, &package_name, entrypoint)?;

        let mut seen = BTreeSet::new();
        for field in fields {
            self.poll()?;
            if !seen.insert(field.name.clone()) {
                self.push_diagnostic(warning(
                    ParseDiagnosticCode::UnsupportedSyntax,
                    source_range_for_node(field.key)?,
                    "Duplicate package.json field has ambiguous semantics",
                )?)?;
            }
            match field.name.as_str() {
                "dependencies" | "peerDependencies" | "optionalDependencies" => {
                    self.add_dependency_object(root_id, field.value, SyntaxRelationKind::Imports)?;
                }
                "devDependencies" => {
                    self.add_dependency_object(root_id, field.value, SyntaxRelationKind::Tests)?;
                }
                "workspaces" => self.add_workspaces(root_id, field.value)?,
                "scripts" => self.add_scripts(root_id, field.value)?,
                "main" | "module" | "types" | "typings" => {
                    self.add_entry_path(root_id, field.value)?;
                }
                "browser" => self.add_browser_paths(root_id, field.value)?,
                "exports" => self.add_export_paths(root_id, field.value)?,
                "bin" => self.add_bins(root_id, field.value, &package_name)?,
                _ => {}
            }
        }
        self.ensure_active()?;
        Ok((self.artifacts, self.flows))
    }

    fn add_root_module(
        &mut self,
        object: Node<'_>,
        name_field: Option<&JsonField<'_>>,
        package_name: &str,
        entrypoint: bool,
    ) -> Result<LocalSymbolId, LanguageParseFailure> {
        let id = self.take_symbol_id()?;
        let declaration_range = source_range_for_node(object)?;
        let (name, selection_range) = match SymbolName::try_from_string(package_name.to_owned()) {
            Ok(name) => {
                let selection = name_field
                    .and_then(|field| {
                        json_string_content_range(self.input.source(), field.value).ok()
                    })
                    .unwrap_or(range_for_offsets(self.input.source(), 0, 0)?);
                (name, selection)
            }
            Err(_) => {
                if let Some(field) = name_field {
                    self.push_diagnostic(warning(
                        ParseDiagnosticCode::OutputTruncated,
                        source_range_for_node(field.value)?,
                        "package.json name exceeds the adapter contract",
                    )?)?;
                }
                (
                    SymbolName::try_from_string("package".to_owned())
                        .map_err(|_| LanguageParseFailure::InvalidResult)?,
                    range_for_offsets(self.input.source(), 0, 0)?,
                )
            }
        };
        let mut symbol = ParsedSymbol::new(
            id,
            SymbolKind::Module,
            name,
            declaration_range,
            selection_range,
        )
        .map_err(|_| LanguageParseFailure::InvalidResult)?
        .with_visibility(SymbolVisibility::Internal)
        .with_signature(
            SymbolSignature::try_from_string("package.json package".to_owned())
                .map_err(|_| LanguageParseFailure::InvalidResult)?,
        );
        if entrypoint {
            symbol = symbol.with_role(SymbolRole::Entrypoint);
        }
        self.push_symbol(symbol)?;
        self.push_relation(SyntaxRelation::new(
            SyntaxSource::File,
            SyntaxTarget::Symbol(id),
            SyntaxRelationKind::Defines,
            SyntaxProvider::Manifest,
            Confidence::certain(),
            declaration_range,
        ))?;
        Ok(id)
    }

    fn add_dependency_object(
        &mut self,
        root: LocalSymbolId,
        object: Node<'_>,
        kind: SyntaxRelationKind,
    ) -> Result<(), LanguageParseFailure> {
        if object.kind() != "object" {
            return self.unsupported_value(object, "package dependency map must be a JSON object");
        }
        for field in self.object_fields(object)? {
            self.poll()?;
            if decode_json_string(self.input.source(), field.value).is_none() {
                self.unsupported_value(
                    field.value,
                    "package dependency version must be a JSON string",
                )?;
                continue;
            }
            self.push_unresolved_relation(
                SyntaxSource::Symbol(root),
                field.name,
                kind,
                source_range_for_node(field.key)?,
            )?;
        }
        Ok(())
    }

    fn add_workspaces(
        &mut self,
        root: LocalSymbolId,
        value: Node<'_>,
    ) -> Result<(), LanguageParseFailure> {
        let packages = if value.kind() == "array" {
            Some(value)
        } else if value.kind() == "object" {
            self.object_fields(value)?
                .into_iter()
                .find(|field| field.name == "packages")
                .map(|field| field.value)
        } else {
            None
        };
        let Some(packages) = packages else {
            return self.unsupported_value(
                value,
                "package workspaces must be an array or packages object",
            );
        };
        if packages.kind() != "array" {
            return self.unsupported_value(packages, "workspace packages must be a JSON array");
        }
        for index in 0..packages.named_child_count() {
            self.poll()?;
            let index =
                u32::try_from(index).map_err(|_| LanguageParseFailure::ResourceLimitExceeded)?;
            let item = packages
                .named_child(index)
                .ok_or(LanguageParseFailure::InvalidResult)?;
            let Some(pattern) = decode_json_string(self.input.source(), item) else {
                self.unsupported_value(item, "workspace package pattern must be a string")?;
                continue;
            };
            self.push_unresolved_relation(
                SyntaxSource::Symbol(root),
                pattern,
                SyntaxRelationKind::Builds,
                source_range_for_node(item)?,
            )?;
        }
        Ok(())
    }

    fn add_scripts(
        &mut self,
        root: LocalSymbolId,
        value: Node<'_>,
    ) -> Result<(), LanguageParseFailure> {
        if value.kind() != "object" {
            return self.unsupported_value(value, "package scripts must be a JSON object");
        }
        for field in self.object_fields(value)? {
            self.poll()?;
            let Some(command) = decode_json_string(self.input.source(), field.value) else {
                self.unsupported_value(field.value, "package script command must be a string")?;
                continue;
            };
            let id = self.take_symbol_id()?;
            let range = source_range_for_node(field.pair)?;
            let name = SymbolName::try_from_string(format!("scripts:{}", field.name))
                .map_err(|_| LanguageParseFailure::InvalidResult)?;
            let role = if field.name == "test" || field.name.starts_with("test:") {
                SymbolRole::Test
            } else {
                SymbolRole::Entrypoint
            };
            self.push_symbol(
                ParsedSymbol::new(
                    id,
                    SymbolKind::Function,
                    name,
                    range,
                    source_range_for_node(field.key)?,
                )
                .map_err(|_| LanguageParseFailure::InvalidResult)?
                .with_visibility(SymbolVisibility::Internal)
                .with_role(role),
            )?;
            self.push_relation(SyntaxRelation::new(
                SyntaxSource::Symbol(root),
                SyntaxTarget::Symbol(id),
                SyntaxRelationKind::Defines,
                SyntaxProvider::Manifest,
                Confidence::certain(),
                range,
            ))?;
            self.flows.push(
                crate::function_flow::package_script(
                    id,
                    self.input.revision().path(),
                    range,
                    &command,
                )
                .map_err(|_| LanguageParseFailure::InvalidResult)?,
            );
            let kind = if field.name == "test" || field.name.starts_with("test:") {
                SyntaxRelationKind::Tests
            } else if field.name == "build"
                || field.name.starts_with("build:")
                || field.name.contains("compile")
                || field.name.contains("bundle")
            {
                SyntaxRelationKind::Builds
            } else {
                SyntaxRelationKind::Configures
            };
            self.push_unresolved_relation(
                SyntaxSource::Symbol(root),
                format!("script:{}", field.name),
                kind,
                source_range_for_node(field.key)?,
            )?;
        }
        Ok(())
    }

    fn add_entry_path(
        &mut self,
        root: LocalSymbolId,
        value: Node<'_>,
    ) -> Result<(), LanguageParseFailure> {
        if value.kind() == "object" {
            return Ok(());
        }
        let Some(path) = decode_json_string(self.input.source(), value) else {
            return self.unsupported_value(value, "package entry path must be a string");
        };
        self.push_file_relation(root, value, &path)
    }

    fn add_browser_paths(
        &mut self,
        root: LocalSymbolId,
        value: Node<'_>,
    ) -> Result<(), LanguageParseFailure> {
        if value.kind() == "string" {
            return self.add_entry_path(root, value);
        }
        if value.kind() != "object" {
            return self.unsupported_value(
                value,
                "package browser target must be a string or JSON object",
            );
        }
        for field in self.object_fields(value)? {
            self.poll()?;
            if field.value.kind() == "false" {
                continue;
            }
            let Some(path) = decode_json_string(self.input.source(), field.value) else {
                self.unsupported_value(
                    field.value,
                    "package browser mapping must target a string or false",
                )?;
                continue;
            };
            self.push_file_relation(root, field.value, &path)?;
        }
        Ok(())
    }

    fn add_export_paths(
        &mut self,
        root: LocalSymbolId,
        value: Node<'_>,
    ) -> Result<(), LanguageParseFailure> {
        let mut stack = vec![value];
        while let Some(node) = stack.pop() {
            self.poll()?;
            match node.kind() {
                "string" => {
                    if let Some(path) = decode_json_string(self.input.source(), node)
                        && path.starts_with('.')
                    {
                        self.push_export_relation(root, node, &path)?;
                    }
                }
                "object" | "array" => {
                    for index in (0..node.named_child_count()).rev() {
                        let index = u32::try_from(index)
                            .map_err(|_| LanguageParseFailure::ResourceLimitExceeded)?;
                        let child = node
                            .named_child(index)
                            .ok_or(LanguageParseFailure::InvalidResult)?;
                        if child.kind() == "pair" {
                            if let Some(value) = child.child_by_field_name("value") {
                                stack.push(value);
                            }
                        } else {
                            stack.push(child);
                        }
                    }
                }
                "null" => {}
                _ => self.unsupported_value(
                    node,
                    "package export target must be a string, object, array, or null",
                )?,
            }
        }
        Ok(())
    }

    fn push_export_relation(
        &mut self,
        source: LocalSymbolId,
        evidence: Node<'_>,
        path: &str,
    ) -> Result<(), LanguageParseFailure> {
        if !path.contains('*') {
            return self.push_file_relation(source, evidence, path);
        }
        let range = source_range_for_node(evidence)?;
        let probe = path.replace('*', "pattern");
        if resolve_manifest_path(self.input.revision().path(), probe.as_bytes()).is_none() {
            self.push_diagnostic(warning(
                ParseDiagnosticCode::UnsupportedSyntax,
                range,
                "package export pattern is not a safe repository-relative target",
            )?)?;
            return Ok(());
        }
        self.push_unresolved_relation(
            SyntaxSource::Symbol(source),
            path.to_owned(),
            SyntaxRelationKind::Builds,
            range,
        )
    }

    fn add_bins(
        &mut self,
        root: LocalSymbolId,
        value: Node<'_>,
        package_name: &str,
    ) -> Result<(), LanguageParseFailure> {
        if value.kind() == "string" {
            return self.add_bin(root, package_name, value, value, value);
        }
        if value.kind() != "object" {
            return self.unsupported_value(value, "package bin must be a string or JSON object");
        }
        for field in self.object_fields(value)? {
            self.poll()?;
            self.add_bin(root, &field.name, field.pair, field.key, field.value)?;
        }
        Ok(())
    }

    fn add_bin(
        &mut self,
        root: LocalSymbolId,
        name: &str,
        declaration: Node<'_>,
        selection: Node<'_>,
        value: Node<'_>,
    ) -> Result<(), LanguageParseFailure> {
        let Some(path) = decode_json_string(self.input.source(), value) else {
            return self.unsupported_value(value, "package bin path must be a string");
        };
        let name = match SymbolName::try_from_string(name.to_owned()) {
            Ok(name) => name,
            Err(_) => {
                self.push_diagnostic(warning(
                    ParseDiagnosticCode::OutputTruncated,
                    source_range_for_node(selection)?,
                    "package bin name exceeds the adapter contract",
                )?)?;
                return Ok(());
            }
        };
        let id = self.take_symbol_id()?;
        let declaration_range = source_range_for_node(declaration)?;
        let selection_range = json_string_content_range(self.input.source(), selection)
            .unwrap_or(source_range_for_node(selection)?);
        let symbol = ParsedSymbol::new(
            id,
            SymbolKind::Module,
            name,
            declaration_range,
            selection_range,
        )
        .map_err(|_| LanguageParseFailure::InvalidResult)?
        .with_visibility(SymbolVisibility::Public)
        .with_role(SymbolRole::Entrypoint);
        self.push_symbol(symbol)?;
        self.push_relation(SyntaxRelation::new(
            SyntaxSource::Symbol(root),
            SyntaxTarget::Symbol(id),
            SyntaxRelationKind::Contains,
            SyntaxProvider::Manifest,
            Confidence::certain(),
            declaration_range,
        ))?;
        self.push_file_relation(id, value, &path)
    }

    fn push_file_relation(
        &mut self,
        source: LocalSymbolId,
        evidence: Node<'_>,
        path: &str,
    ) -> Result<(), LanguageParseFailure> {
        let range = source_range_for_node(evidence)?;
        let Some(path) = resolve_manifest_path(self.input.revision().path(), path.as_bytes())
        else {
            self.push_diagnostic(warning(
                ParseDiagnosticCode::UnsupportedSyntax,
                range,
                "package path is not a safe repository-relative file target",
            )?)?;
            return Ok(());
        };
        self.push_relation(SyntaxRelation::new(
            SyntaxSource::Symbol(source),
            SyntaxTarget::File(path),
            SyntaxRelationKind::Builds,
            SyntaxProvider::Manifest,
            Confidence::certain(),
            range,
        ))
    }

    fn object_fields<'tree>(
        &mut self,
        object: Node<'tree>,
    ) -> Result<Vec<JsonField<'tree>>, LanguageParseFailure> {
        let mut fields = Vec::new();
        for index in 0..object.named_child_count() {
            self.poll()?;
            let index =
                u32::try_from(index).map_err(|_| LanguageParseFailure::ResourceLimitExceeded)?;
            let pair = object
                .named_child(index)
                .ok_or(LanguageParseFailure::InvalidResult)?;
            if pair.kind() != "pair" {
                continue;
            }
            let key = pair
                .child_by_field_name("key")
                .ok_or(LanguageParseFailure::InvalidResult)?;
            let value = pair
                .child_by_field_name("value")
                .ok_or(LanguageParseFailure::InvalidResult)?;
            let Some(name) = decode_json_string(self.input.source(), key) else {
                let code = if super::source::node_text(self.input.source(), key).is_some() {
                    ParseDiagnosticCode::UnsupportedSyntax
                } else {
                    ParseDiagnosticCode::InvalidEncoding
                };
                self.push_diagnostic(diagnostic(
                    code,
                    source_range_for_node(key)?,
                    "package.json field name is not a supported JSON string",
                )?)?;
                continue;
            };
            fields.push(JsonField {
                name,
                pair,
                key,
                value,
            });
        }
        Ok(fields)
    }

    fn unsupported_value(
        &mut self,
        node: Node<'_>,
        message: &'static str,
    ) -> Result<(), LanguageParseFailure> {
        let diagnostic = warning(
            ParseDiagnosticCode::UnsupportedSyntax,
            source_range_for_node(node)?,
            message,
        )?;
        self.push_diagnostic(diagnostic)
    }

    fn push_unresolved_relation(
        &mut self,
        source: SyntaxSource,
        reference: String,
        kind: SyntaxRelationKind,
        range: SourceRange,
    ) -> Result<(), LanguageParseFailure> {
        let target = match SymbolReference::try_from_string(reference) {
            Ok(reference) => SyntaxTarget::Unresolved(reference),
            Err(_) => {
                self.push_diagnostic(warning(
                    ParseDiagnosticCode::OutputTruncated,
                    range,
                    "package relation target exceeds the adapter contract",
                )?)?;
                return Ok(());
            }
        };
        self.push_relation(SyntaxRelation::new(
            source,
            target,
            kind,
            SyntaxProvider::Manifest,
            Confidence::certain(),
            range,
        ))
    }

    fn poll(&mut self) -> Result<(), LanguageParseFailure> {
        self.visited = self
            .visited
            .checked_add(1)
            .filter(|value| *value <= self.policy.max_tree_nodes())
            .ok_or(LanguageParseFailure::ResourceLimitExceeded)?;
        if self.visited.is_multiple_of(EXTRACTION_POLL_INTERVAL) {
            self.ensure_active()?;
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
        if self.artifacts.relations.contains(&relation) {
            return Ok(());
        }
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

fn decode_json_string(source: &[u8], node: Node<'_>) -> Option<String> {
    if node.kind() != "string" {
        return None;
    }
    source
        .get(node.byte_range())
        .and_then(|bytes| serde_json::from_slice::<String>(bytes).ok())
}

fn json_string_content_range(
    source: &[u8],
    node: Node<'_>,
) -> Result<SourceRange, LanguageParseFailure> {
    if node.kind() != "string" || node.end_byte().saturating_sub(node.start_byte()) < 2 {
        return Err(LanguageParseFailure::InvalidResult);
    }
    range_for_offsets(
        source,
        node.start_byte().saturating_add(1),
        node.end_byte().saturating_sub(1),
    )
}

fn resolve_manifest_path(manifest: &RepositoryPath, relative: &[u8]) -> Option<RepositoryPath> {
    let relative = relative.strip_prefix(b"./").unwrap_or(relative);
    if relative.is_empty()
        || relative.first() == Some(&b'/')
        || relative.contains(&b'\\')
        || relative.contains(&b'*')
        || relative.contains(&b'\0')
        || relative.windows(2).any(|window| window == b":/")
    {
        return None;
    }
    let mut components = manifest
        .as_bytes()
        .split(|byte| *byte == b'/')
        .collect::<Vec<_>>();
    components.pop()?;
    for component in relative.split(|byte| *byte == b'/') {
        match component {
            b"" | b"." => {}
            b".." => {
                components.pop()?;
            }
            value => components.push(value),
        }
    }
    let mut bytes = Vec::new();
    for component in components {
        if !bytes.is_empty() {
            bytes.push(b'/');
        }
        bytes.extend_from_slice(component);
    }
    RepositoryPath::try_from_bytes(bytes).ok()
}
