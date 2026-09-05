//! Deliberately small process model. This module never opens or executes a target.
use a3_application::LanguageParseFailure;
use a3_domain::{
    FlowArgument, FlowGap, FlowGapKind, FlowProcess, FlowProcessMode, FlowProcessTarget,
    FlowStepKind, FunctionFlow, RepositoryPath, StaticImportBinding,
};
use tree_sitter::{Node, Tree};

pub(super) fn annotate(
    tree: &Tree,
    source: &[u8],
    path: &RepositoryPath,
    imports: &[StaticImportBinding],
    blocked: &std::collections::BTreeSet<String>,
    flow: FunctionFlow,
) -> Result<FunctionFlow, LanguageParseFailure> {
    let mut steps = flow.steps().to_vec();
    let mut gaps = flow.gaps().to_vec();
    for step in &mut steps {
        if step.kind != FlowStepKind::Call {
            continue;
        }
        let Some(name) = step.name.as_ref().map(|n| n.as_str()) else {
            continue;
        };
        let Some((module, method)) = imported(name, step.range, imports) else {
            continue;
        };
        let mode = match (module, method) {
            ("node:child_process" | "child_process", "execFileSync" | "spawnSync")
            | ("subprocess", "run" | "call" | "check_call" | "check_output") => {
                FlowProcessMode::Wait
            }
            ("node:child_process" | "child_process", "execFile" | "spawn" | "fork")
            | ("subprocess", "Popen") => FlowProcessMode::Spawn,
            _ => continue,
        };
        let Some(node) = tree.root_node().descendant_for_byte_range(
            step.range.start_byte() as usize,
            step.range.end_byte() as usize,
        ) else {
            continue;
        };
        let args = node
            .child_by_field_name("arguments")
            .map(children)
            .unwrap_or_default();
        // Any local binding of the imported root prevents a library identity claim.
        let root = name.split('.').next().unwrap_or(name);
        let shadowed = blocked.contains(root)
            || flow.values().iter().any(|v| {
                v.name.as_str() == root
                    && !imports
                        .iter()
                        .any(|i| i.local.as_str() == root && i.range == v.range)
                    && matches!(
                        v.kind,
                        a3_domain::FlowValueKind::Local
                            | a3_domain::FlowValueKind::Parameter
                            | a3_domain::FlowValueKind::Merge
                    )
            });
        let python = module == "subprocess";
        let options = if python { None } else { args.get(2).copied() };
        let cwd = if python {
            args.iter()
                .find(|a| {
                    a.kind() == "keyword_argument"
                        && a.child_by_field_name("name").and_then(|n| text(source, n))
                            == Some("cwd")
                })
                .and_then(|a| a.child_by_field_name("value"))
        } else {
            options
                .and_then(|o| {
                    children(o).into_iter().find(|p| {
                        p.child_by_field_name("key").and_then(|n| text(source, n)) == Some("cwd")
                    })
                })
                .and_then(|p| p.child_by_field_name("value"))
        };
        let known_cwd = cwd.and_then(|n| text(source, n)).is_some_and(|s| {
            if python {
                imports.iter().any(|i| {
                    i.module.as_str() == "os"
                        && !blocked.contains(i.local.as_str())
                        && !blocked.contains("__file__")
                        && i.export.is_none()
                        && s == format!("{}.path.dirname(__file__)", i.local.as_str())
                })
            } else {
                s == "import.meta.dirname"
                    || (s == "__dirname"
                        && !blocked.contains("__dirname")
                        && path.as_bytes().ends_with(b".cjs"))
            }
        });
        let safe_options = if python {
            args.iter().skip(1).all(|a| a.kind() == "keyword_argument")
                && args
                    .iter()
                    .filter(|a| a.kind() == "keyword_argument")
                    .all(|a| {
                        let key = a.child_by_field_name("name").and_then(|n| text(source, n));
                        matches!(
                            key,
                            Some("cwd" | "check" | "capture_output" | "text" | "timeout")
                        ) || key == Some("shell")
                            && a.child_by_field_name("value").and_then(|n| text(source, n))
                                == Some("False")
                    })
        } else {
            options.is_some_and(|o| {
                o.kind() == "object"
                    && children(o)
                        .iter()
                        .filter(|p| {
                            p.child_by_field_name("key").and_then(|n| text(source, n))
                                == Some("cwd")
                        })
                        .count()
                        == 1
                    && children(o).iter().all(|p| {
                        matches!(
                            p.child_by_field_name("key").and_then(|n| text(source, n)),
                            Some("cwd" | "encoding" | "stdio" | "timeout")
                        )
                    })
            })
        };
        let argv = if python {
            args.first()
                .filter(|n| n.kind() == "list")
                .map(|n| children(*n))
                .unwrap_or_default()
        } else {
            let mut argv = args.first().copied().into_iter().collect::<Vec<_>>();
            argv.extend(
                args.get(1)
                    .filter(|n| n.kind() == "array")
                    .map(|n| children(*n))
                    .unwrap_or_default(),
            );
            argv
        };
        let executable = argv.first().and_then(|n| literal(source, *n));
        let supported = matches!(
            executable.as_deref(),
            Some("node" | "nodejs" | "python" | "python3")
        );
        let spread = argv.iter().any(|a| {
            matches!(
                a.kind(),
                "spread_element" | "list_splat" | "dictionary_splat"
            )
        });
        let target = if !shadowed && known_cwd && safe_options && supported && !spread {
            argv.get(1)
                .and_then(|n| literal(source, *n))
                .and_then(|s| relative_target(path, &s))
                .map(FlowProcessTarget::File)
        } else {
            None
        };
        let arguments = argv
            .iter()
            .skip(2)
            .take(64)
            .filter_map(|n| {
                let range = crate::source_range_for_node(*n).ok()?;
                Some(FlowArgument {
                    keyword: None,
                    range,
                    values: flow
                        .values()
                        .iter()
                        .filter(|v| {
                            range.contains(v.range)
                                || step.inputs.contains(&v.id)
                                    && text(source, *n) == Some(v.name.as_str())
                        })
                        .map(|v| v.id)
                        .collect(),
                })
            })
            .collect();
        if target.is_none() || argv.len() > 66 {
            gaps.push(FlowGap {
                kind: FlowGapKind::Dynamic,
                range: step.range,
            });
        }
        step.kind = FlowStepKind::Process;
        step.process = Some(FlowProcess {
            mode,
            target,
            arguments,
        });
    }
    let result = FunctionFlow::new(
        flow.owner(),
        flow.range(),
        steps,
        flow.values().to_vec(),
        gaps,
    );
    let result = match result {
        Err(a3_domain::FunctionFlowError::Limit) => FunctionFlow::new(
            flow.owner(),
            flow.range(),
            Vec::new(),
            Vec::new(),
            vec![FlowGap {
                kind: FlowGapKind::Limit,
                range: flow.range(),
            }],
        ),
        other => other,
    };
    result
        .and_then(|f| f.with_lexical_scope(flow.lexical_scope()))
        .map_err(|_| LanguageParseFailure::InvalidResult)
}

fn imported<'a>(
    name: &str,
    range: a3_domain::SourceRange,
    imports: &'a [StaticImportBinding],
) -> Option<(&'a str, &'a str)> {
    let mut candidates = imports.iter().filter_map(|i| {
        if !i.scope.contains(range) {
            return None;
        }
        if let Some(export) = &i.export {
            (i.local.as_str() == name).then_some((i.module.as_str(), export.as_str()))
        } else {
            name.strip_prefix(i.local.as_str())
                .and_then(|tail| tail.strip_prefix('.'))
                .and_then(|tail| {
                    // Method names are chosen from fixed library names, not allocated source text.
                    [
                        "execFileSync",
                        "spawnSync",
                        "execFile",
                        "spawn",
                        "fork",
                        "run",
                        "call",
                        "check_call",
                        "check_output",
                        "Popen",
                    ]
                    .into_iter()
                    .find(|m| *m == tail)
                    .map(|m| (i.module.as_str(), m))
                })
        }
    });
    let first = candidates.next()?;
    candidates.next().is_none().then_some(first)
}

pub(super) fn script_argument(
    node: Node<'_>,
    source: &[u8],
    imports: &[StaticImportBinding],
) -> Option<(String, u16)> {
    let object = node
        .child_by_field_name("object")
        .or_else(|| node.child_by_field_name("value"))?;
    let index = node
        .child_by_field_name("index")
        .or_else(|| node.child_by_field_name("subscript"))?;
    let name = text(source, object)?;
    let offset = if name == "process.argv" {
        2
    } else if imports.iter().any(|i| {
        i.module.as_str() == "sys"
            && i.export.is_none()
            && name == format!("{}.argv", i.local.as_str())
    }) {
        1
    } else {
        return None;
    };
    let slot = text(source, index)?
        .parse::<u16>()
        .ok()?
        .checked_sub(offset)?;
    (slot < 64).then(|| (format!("argument{}", slot + 1), slot))
}

pub(crate) fn relative_target(owner: &RepositoryPath, target: &str) -> Option<RepositoryPath> {
    if target.is_empty()
        || target.starts_with(['/', '\\', '-'])
        || target.contains([':', '\\', '\0'])
    {
        return None;
    }
    let mut parts = std::str::from_utf8(owner.as_bytes())
        .ok()?
        .split('/')
        .collect::<Vec<_>>();
    parts.pop();
    for part in target.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            part => parts.push(part),
        }
    }
    RepositoryPath::try_from_bytes(parts.join("/").into_bytes()).ok()
}
fn children(node: Node<'_>) -> Vec<Node<'_>> {
    let mut c = node.walk();
    node.named_children(&mut c).collect()
}
fn text<'a>(source: &'a [u8], node: Node<'_>) -> Option<&'a str> {
    std::str::from_utf8(source.get(node.byte_range())?).ok()
}
fn literal(source: &[u8], node: Node<'_>) -> Option<String> {
    let s = text(source, node)?;
    let quote = *s.as_bytes().first()?;
    if s.len() < 2
        || !matches!(quote, b'\'' | b'"')
        || s.as_bytes().last() != Some(&quote)
        || s.contains(['\\', '\n', '\r'])
    {
        return None;
    }
    Some(s[1..s.len() - 1].to_owned())
}
