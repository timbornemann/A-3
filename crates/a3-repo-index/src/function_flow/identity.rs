//! Conservative file-local library identity. Rebinding or mutation denies a built-in claim.
use a3_application::{LanguageParseControl, LanguageParseFailure};
use a3_domain::StaticImportBinding;
use std::{collections::BTreeSet, time::Instant};
use tree_sitter::Tree;

pub(super) fn blocked_roots(
    tree: &Tree,
    source: &[u8],
    imports: &[StaticImportBinding],
    control: &dyn LanguageParseControl,
    deadline: Instant,
) -> Result<BTreeSet<String>, LanguageParseFailure> {
    let mut blocked = BTreeSet::new();
    let mut stack = vec![tree.root_node()];
    let mut visited = 0usize;
    while let Some(node) = stack.pop() {
        visited += 1;
        if visited.is_multiple_of(128) {
            if control.is_cancelled() {
                return Err(LanguageParseFailure::Cancelled);
            }
            if Instant::now() >= deadline {
                return Err(LanguageParseFailure::TimedOut);
            }
        }
        if visited > 1_000_000 {
            return Err(LanguageParseFailure::ResourceLimitExceeded);
        }
        let binding = match node.kind() {
            "variable_declarator"
            | "function_declaration"
            | "function_definition"
            | "class_declaration"
            | "class_definition" => node.child_by_field_name("name"),
            "assignment"
            | "assignment_expression"
            | "augmented_assignment"
            | "augmented_assignment_expression" => node.child_by_field_name("left"),
            "required_parameter" | "optional_parameter" | "typed_parameter"
            | "default_parameter" => node
                .child_by_field_name("pattern")
                .or_else(|| node.child_by_field_name("name")),
            "update_expression" => node.child_by_field_name("argument"),
            "identifier"
                if node
                    .parent()
                    .is_some_and(|p| matches!(p.kind(), "parameters" | "formal_parameters")) =>
            {
                Some(node)
            }
            _ => None,
        };
        if let Some(binding) = binding {
            let mut patterns = vec![binding];
            while let Some(pattern) = patterns.pop() {
                if matches!(
                    pattern.kind(),
                    "identifier" | "shorthand_property_identifier_pattern"
                ) {
                    let name = std::str::from_utf8(&source[pattern.byte_range()]).unwrap_or("");
                    let range = crate::source_range_for_node(pattern)?;
                    if !imports
                        .iter()
                        .any(|i| i.local.as_str() == name && i.range == range)
                    {
                        blocked.insert(name.to_owned());
                    }
                } else if matches!(
                    pattern.kind(),
                    "member_expression" | "attribute" | "subscript" | "subscript_expression"
                ) {
                    if let Some(object) = pattern
                        .child_by_field_name("object")
                        .or_else(|| pattern.child_by_field_name("value"))
                    {
                        patterns.push(object);
                    }
                } else {
                    let mut cursor = pattern.walk();
                    patterns.extend(pattern.named_children(&mut cursor));
                }
            }
        }
        let mut cursor = node.walk();
        stack.extend(node.named_children(&mut cursor));
    }
    Ok(blocked)
}
