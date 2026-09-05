use crate::source_range_for_node as range;
use a3_application::{LanguageParseControl, LanguageParseFailure};
use a3_domain::{SourceRange, StaticImportBinding, SymbolName, SymbolReference};
use tree_sitter::{Node, Tree};

pub(super) fn collect(
    tree: &Tree,
    source: &[u8],
    control: &dyn LanguageParseControl,
    deadline: std::time::Instant,
) -> Result<Vec<StaticImportBinding>, LanguageParseFailure> {
    let root = tree.root_node();
    let mut stack = vec![(root, range(root)?)];
    let mut result = Vec::new();
    let mut visited = 0usize;
    while let Some((node, scope)) = stack.pop() {
        visited += 1;
        if visited.is_multiple_of(128) && control.is_cancelled() {
            return Err(LanguageParseFailure::Cancelled);
        }
        if visited.is_multiple_of(128) && std::time::Instant::now() >= deadline {
            return Err(LanguageParseFailure::TimedOut);
        }
        if visited > 1_000_000 {
            return Err(LanguageParseFailure::ResourceLimitExceeded);
        }
        let local_scope = if matches!(node.kind(), "statement_block" | "block") {
            range(node)?
        } else {
            scope
        };
        match node.kind() {
            "variable_declarator" => {
                if let (Some(pattern), Some(value)) = (
                    node.child_by_field_name("name"),
                    node.child_by_field_name("value"),
                ) && value.kind() == "call_expression"
                    && value
                        .child_by_field_name("function")
                        .and_then(|n| text(source, n))
                        == Some("require")
                    && let Some(arguments) = value.child_by_field_name("arguments")
                    && arguments.named_child_count() == 1
                    && let Some(module) = arguments.named_child(0).and_then(|n| literal(source, n))
                {
                    if pattern.kind() == "identifier" {
                        push(&mut result, source, pattern, &module, None, scope)?;
                    } else if pattern.kind() == "object_pattern" {
                        for binding in children(pattern) {
                            if binding.kind() == "shorthand_property_identifier_pattern" {
                                push(
                                    &mut result,
                                    source,
                                    binding,
                                    &module,
                                    text(source, binding),
                                    scope,
                                )?;
                            } else if let (Some(key), Some(alias)) = (
                                binding.child_by_field_name("key"),
                                binding.child_by_field_name("value"),
                            ) {
                                push(
                                    &mut result,
                                    source,
                                    alias,
                                    &module,
                                    text(source, key),
                                    scope,
                                )?;
                            }
                        }
                    }
                }
            }
            "import_statement" if node.child_by_field_name("source").is_some() => {
                if let Some(module) = node
                    .child_by_field_name("source")
                    .and_then(|n| literal(source, n))
                    && let Some(clause) = children(node)
                        .into_iter()
                        .find(|n| n.kind() == "import_clause")
                {
                    for binding in children(clause) {
                        match binding.kind() {
                            "identifier" => push(
                                &mut result,
                                source,
                                binding,
                                &module,
                                Some("default"),
                                scope,
                            )?,
                            "namespace_import" => {
                                if let Some(name) = binding.named_child(0) {
                                    push(&mut result, source, name, &module, None, scope)?;
                                }
                            }
                            "named_imports" => {
                                for entry in children(binding) {
                                    if text(source, entry)
                                        .is_some_and(|s| s.trim_start().starts_with("type "))
                                    {
                                        continue;
                                    }
                                    if let Some(export) = entry.child_by_field_name("name") {
                                        let alias =
                                            entry.child_by_field_name("alias").unwrap_or(export);
                                        push(
                                            &mut result,
                                            source,
                                            alias,
                                            &module,
                                            text(source, export),
                                            scope,
                                        )?;
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
                continue;
            }
            "import_from_statement" => {
                if let Some(module) = node
                    .child_by_field_name("module_name")
                    .and_then(|n| text(source, n))
                {
                    for entry in children(node) {
                        if Some(entry) == node.child_by_field_name("module_name") {
                            continue;
                        }
                        if entry.kind() == "aliased_import" {
                            if let (Some(name), Some(alias)) = (
                                entry.child_by_field_name("name"),
                                entry.child_by_field_name("alias"),
                            ) {
                                push(
                                    &mut result,
                                    source,
                                    alias,
                                    module,
                                    text(source, name),
                                    scope,
                                )?;
                            }
                        } else if entry.kind() == "dotted_name" {
                            push(
                                &mut result,
                                source,
                                entry,
                                module,
                                text(source, entry),
                                scope,
                            )?;
                        }
                    }
                }
                continue;
            }
            "import_statement" => {
                for entry in children(node) {
                    let name = entry.child_by_field_name("name").unwrap_or(entry);
                    let alias = entry.child_by_field_name("alias").unwrap_or(name);
                    if let Some(module) = text(source, name) {
                        // Dotted unaliased Python imports bind only the first component.
                        if entry.child_by_field_name("alias").is_some() || !module.contains('.') {
                            push(&mut result, source, alias, module, None, scope)?;
                        }
                    }
                }
                continue;
            }
            "use_declaration" => {
                if let Some(argument) = node.child_by_field_name("argument") {
                    rust_use(argument, source, "", scope, &mut result, 0)?;
                }
                continue;
            }
            _ => {}
        }
        stack.extend(children(node).into_iter().rev().map(|n| (n, local_scope)));
    }
    Ok(result)
}
fn rust_use(
    node: Node<'_>,
    source: &[u8],
    prefix: &str,
    scope: SourceRange,
    result: &mut Vec<StaticImportBinding>,
    depth: usize,
) -> Result<(), LanguageParseFailure> {
    if depth > 32 {
        return Ok(());
    }
    if node.kind() == "scoped_use_list" {
        let path = node
            .child_by_field_name("path")
            .and_then(|n| text(source, n))
            .unwrap_or("");
        let prefix = join(prefix, path);
        if let Some(list) = node.child_by_field_name("list") {
            for child in children(list) {
                rust_use(child, source, &prefix, scope, result, depth + 1)?;
            }
        }
        return Ok(());
    }
    if node.kind() == "use_list" {
        for child in children(node) {
            rust_use(child, source, prefix, scope, result, depth + 1)?;
        }
        return Ok(());
    }
    let path = node.child_by_field_name("path").unwrap_or(node);
    let Some(value) = text(source, path) else {
        return Ok(());
    };
    if value.contains('*') || value.contains('{') {
        return Ok(());
    }
    let full = join(prefix, value);
    let alias = node.child_by_field_name("alias");
    let local = alias
        .or_else(|| path.child_by_field_name("name"))
        .unwrap_or(path);
    // A fully qualified module+name path is resolved by the existing Rust resolver.
    push(result, source, local, &full, Some("self"), scope)
}
fn join(prefix: &str, value: &str) -> String {
    if prefix.is_empty() {
        value.to_owned()
    } else {
        format!("{prefix}::{value}")
    }
}
fn push(
    result: &mut Vec<StaticImportBinding>,
    source: &[u8],
    alias: Node<'_>,
    module: &str,
    export: Option<&str>,
    scope: SourceRange,
) -> Result<(), LanguageParseFailure> {
    if result.len() >= 4096 {
        return Ok(());
    }
    let Some(local) = text(source, alias) else {
        return Ok(());
    };
    if !local
        .chars()
        .all(|c| c.is_alphanumeric() || matches!(c, '_' | '$'))
    {
        return Ok(());
    }
    let (Ok(local), Ok(module)) = (
        SymbolName::try_from_string(local.to_owned()),
        SymbolReference::try_from_string(module.to_owned()),
    ) else {
        return Ok(());
    };
    let export = export.and_then(|s| SymbolName::try_from_string(s.to_owned()).ok());
    result.push(StaticImportBinding {
        local,
        module,
        export,
        range: range(alias)?,
        scope,
    });
    Ok(())
}
fn children(node: Node<'_>) -> Vec<Node<'_>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor).collect()
}
fn text<'a>(source: &'a [u8], node: Node<'_>) -> Option<&'a str> {
    std::str::from_utf8(source.get(node.start_byte()..node.end_byte())?).ok()
}
fn literal(source: &[u8], node: Node<'_>) -> Option<String> {
    let s = text(source, node)?;
    let q = s.as_bytes().first()?;
    if s.len() < 2
        || !matches!(q, b'\'' | b'"')
        || s.as_bytes().last() != Some(q)
        || s.contains('\\')
    {
        return None;
    }
    Some(s[1..s.len() - 1].to_owned())
}
