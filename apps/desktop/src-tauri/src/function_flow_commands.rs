use crate::{CompositionRoot, DesktopBoundedReadControl, decode_stable_id, lock_recovering_poison};
use a3_application::{ExploreFunctionFlows, FunctionFlowReadFailure, FunctionFlowSelection};
use a3_domain::{
    FlowStepId, FlowValueId, GraphSymbol, IndexRunId, ModuleCardEvidenceId, ModuleId, SymbolId,
    SymbolKind, SymbolRole,
};
use a3_protocol::*;
use std::collections::BTreeMap;

#[tauri::command]
pub(crate) async fn query_function_flows(
    request: QueryFunctionFlowsRequestV1,
    root: tauri::State<'_, CompositionRoot>,
) -> Result<FunctionFlowsResponseV1, CommandErrorV1> {
    execute(request, root.inner()).await
}
pub(crate) async fn execute(
    request: QueryFunctionFlowsRequestV1,
    root: &CompositionRoot,
) -> Result<FunctionFlowsResponseV1, CommandErrorV1> {
    if request.protocol_version != ProtocolVersion::CURRENT {
        return Err(CommandErrorV1::unsupported_protocol_version());
    }
    validate(&request.query)?;
    let Some(active) = lock_recovering_poison(&root.active_project).clone() else {
        return Ok(response(FunctionFlowsResultV1::NoProject));
    };
    let Some(store) = root.function_flow_index.as_ref() else {
        return Ok(response(FunctionFlowsResultV1::NoPublishedIndex));
    };
    let explorer = ExploreFunctionFlows::new(store.clone());
    let Some(index) = store
        .latest_published_index(&active.project, &DesktopBoundedReadControl::new())
        .await
        .map_err(|_| unavailable())?
    else {
        return Ok(response(FunctionFlowsResultV1::NoPublishedIndex));
    };
    let modules = index
        .publication()
        .modules()
        .memberships()
        .iter()
        .filter(|m| m.evidence().kind().is_primary())
        .map(|m| (m.symbol_id(), m.module_id()))
        .collect::<BTreeMap<_, _>>();
    let result = match request.query {
        FunctionFlowQueryV1::Source { selection, step } => {
            let selection = decode_selection(&selection)?;
            let control = DesktopBoundedReadControl::new();
            let Some(inspection) = explorer
                .inspect(&active.project, &selection, &control)
                .await
                .map_err(map_error)?
            else {
                return Ok(response(FunctionFlowsResultV1::SelectionChanged));
            };
            let frame = inspection.frames.last().ok_or_else(invalid)?;
            let range = frame
                .flow
                .analysis()
                .steps()
                .iter()
                .find(|s| s.id.get() == step)
                .map(|s| s.range)
                .ok_or_else(invalid)?;
            let preview = a3_application::read_current_source_preview(
                &a3_workspace::WorkspaceAgentSourceReader,
                &active.project,
                frame.owner.revision(),
                Some(range),
                &control,
            )
            .await
            .map_err(crate::map_project_map_source_preview_error_to_v1)?;
            if explorer
                .inspect(&active.project, &selection, &control)
                .await
                .map_err(map_error)?
                .is_none()
            {
                return Ok(response(FunctionFlowsResultV1::SelectionChanged));
            }
            FunctionFlowsResultV1::Source {
                preview: crate::map_project_map_source_preview_to_v1(&preview),
            }
        }
        FunctionFlowQueryV1::Catalog { term, offset } => {
            let Some(page) = explorer
                .catalog(
                    &active.project,
                    &term,
                    offset as usize,
                    &DesktopBoundedReadControl::new(),
                )
                .await
                .map_err(map_error)?
            else {
                return Ok(response(FunctionFlowsResultV1::NoPublishedIndex));
            };
            if page.run_id != index.run().id() {
                return Ok(response(FunctionFlowsResultV1::SelectionChanged));
            }
            FunctionFlowsResultV1::Catalog {
                page: FunctionFlowPageV1 {
                    entries: page
                        .symbols
                        .iter()
                        .map(|s| {
                            entry(
                                s,
                                FunctionFlowSelection {
                                    run_id: page.run_id,
                                    root: s.id(),
                                    call_path: Vec::new(),
                                },
                                &modules,
                            )
                        })
                        .collect(),
                    has_more: page.has_more,
                },
            }
        }
        FunctionFlowQueryV1::Inspect {
            selection,
            step_offset,
            value_offset,
        } => {
            let selection = decode_selection(&selection)?;
            let Some(inspected) = explorer
                .inspect(
                    &active.project,
                    &selection,
                    &DesktopBoundedReadControl::new(),
                )
                .await
                .map_err(map_error)?
            else {
                return Ok(response(FunctionFlowsResultV1::SelectionChanged));
            };
            if selection.run_id != index.run().id() {
                return Ok(response(FunctionFlowsResultV1::SelectionChanged));
            }
            let frame = inspected.frames.last().ok_or_else(unavailable)?;
            let flow = frame.flow.analysis();
            if step_offset as usize > flow.steps().len().div_ceil(50) * 50
                || value_offset as usize > flow.values().len().div_ceil(50) * 50
            {
                return Err(invalid());
            }
            let targets = frame
                .flow
                .calls()
                .iter()
                .map(|c| (c.step, c.target))
                .collect::<BTreeMap<_, _>>();
            let steps = flow
                .steps()
                .iter()
                .skip(step_offset as usize)
                .take(50)
                .map(|s| {
                    let target = targets
                        .get(&s.id)
                        .copied()
                        .flatten()
                        .filter(|_| selection.call_path.len() < 7)
                        .map(|_| {
                            let mut next = selection.clone();
                            next.call_path.push(s.id);
                            encode_selection(&next)
                        });
                    FunctionFlowStepV1 {
                        process_mode: s.process.as_ref().map(|p| match p.mode {
                            a3_domain::FlowProcessMode::Wait => FunctionFlowProcessModeV1::Wait,
                            a3_domain::FlowProcessMode::Spawn => FunctionFlowProcessModeV1::Spawn,
                            a3_domain::FlowProcessMode::CompileOnly => {
                                FunctionFlowProcessModeV1::CompileOnly
                            }
                        }),
                        values_truncated: s.inputs.len() > 50 || s.outputs.len() > 50,
                        id: s.id.get(),
                        parent: s.parent.map(FlowStepId::get),
                        kind: step_kind(s.kind),
                        name: s.name.as_ref().map(|n| n.as_str().to_owned()),
                        line: s.range.start_position().row() + 1,
                        target,
                        inputs: s.inputs.iter().take(50).map(|i| i.get()).collect(),
                        outputs: s.outputs.iter().take(50).map(|i| i.get()).collect(),
                    }
                })
                .collect();
            FunctionFlowsResultV1::Flow {
                flow: Box::new(FunctionFlowViewV1 {
                    selection: encode_selection(&selection),
                    name: frame.owner.parsed().name().as_str().to_owned(),
                    source: source(&frame.owner, &modules),
                    breadcrumbs: inspected
                        .frames
                        .iter()
                        .enumerate()
                        .map(|(i, f)| {
                            entry(
                                &f.owner,
                                FunctionFlowSelection {
                                    run_id: selection.run_id,
                                    root: selection.root,
                                    call_path: selection.call_path[..i].to_vec(),
                                },
                                &modules,
                            )
                        })
                        .collect(),
                    steps,
                    values: flow
                        .values()
                        .iter()
                        .skip(value_offset as usize)
                        .take(50)
                        .map(|v| FunctionFlowValueV1 {
                            id: v.id.get(),
                            name: v.name.as_str().to_owned(),
                            kind: value_kind(v.kind),
                            line: v.range.start_position().row() + 1,
                        })
                        .collect(),
                    step_total: flow.steps().len() as u32,
                    value_total: flow.values().len() as u32,
                    gaps: flow
                        .gaps()
                        .iter()
                        .take(50)
                        .map(|g| FunctionFlowGapV1 {
                            kind: gap_kind(g.kind),
                            line: g.range.start_position().row() + 1,
                        })
                        .collect(),
                    gaps_truncated: flow.gaps().len() > 50,
                }),
            }
        }
        FunctionFlowQueryV1::Trace {
            selection,
            value,
            direction,
        } => {
            let selection = decode_selection(&selection)?;
            let dir = match direction {
                FunctionFlowTraceDirectionV1::Origins => {
                    a3_application::FlowTraceDirection::Origins
                }
                FunctionFlowTraceDirectionV1::Uses => a3_application::FlowTraceDirection::Uses,
            };
            let Some(trace) = explorer
                .trace_value(
                    &active.project,
                    &selection,
                    FlowValueId::new(value).map_err(|_| invalid())?,
                    dir,
                    &DesktopBoundedReadControl::new(),
                )
                .await
                .map_err(map_error)?
            else {
                return Ok(response(FunctionFlowsResultV1::SelectionChanged));
            };
            FunctionFlowsResultV1::Trace {
                trace: FunctionFlowTraceV1 {
                    direction,
                    truncated: trace.truncated,
                    nodes: trace
                        .nodes
                        .into_iter()
                        .map(|n| FunctionFlowTraceNodeV1 {
                            selection: encode_selection(&FunctionFlowSelection {
                                run_id: selection.run_id,
                                root: selection.root,
                                call_path: n.address.call_path,
                            }),
                            value: FunctionFlowValueV1 {
                                id: n.address.value.get(),
                                name: n.name,
                                kind: value_kind(n.kind),
                                line: n.evidence.range().start_position().row() + 1,
                            },
                            function_name: n.function_name,
                            path: String::from_utf8_lossy(n.evidence.revision().path().as_bytes())
                                .into_owned(),
                            unknown: n.unknown,
                        })
                        .collect(),
                },
            }
        }
    };
    // Do not deliver results for a project switched while the async read was pending.
    if lock_recovering_poison(&root.active_project)
        .as_ref()
        .is_none_or(|p| p.project.worktree().id() != active.project.worktree().id())
    {
        return Ok(response(FunctionFlowsResultV1::SelectionChanged));
    }
    Ok(response(result))
}
fn validate(query: &FunctionFlowQueryV1) -> Result<(), CommandErrorV1> {
    match query {
        FunctionFlowQueryV1::Source { selection, step } if *step > 0 && *step <= 4096 => {
            decode_selection(selection).map(|_| ())
        }
        FunctionFlowQueryV1::Catalog { term, offset }
            if term.len() <= 512 && *offset <= 1_000_000 && offset % 50 == 0 =>
        {
            Ok(())
        }
        FunctionFlowQueryV1::Inspect {
            selection,
            step_offset,
            value_offset,
        } if *step_offset <= 4050
            && *value_offset <= 4050
            && step_offset % 50 == 0
            && value_offset % 50 == 0 =>
        {
            decode_selection(selection).map(|_| ())
        }
        FunctionFlowQueryV1::Trace {
            selection, value, ..
        } if *value > 0 && *value <= 4096 => decode_selection(selection).map(|_| ()),
        _ => Err(invalid()),
    }
}

fn decode_selection(s: &FunctionFlowSelectionV1) -> Result<FunctionFlowSelection, CommandErrorV1> {
    if s.call_path.len() >= 8 {
        return Err(invalid());
    }
    Ok(FunctionFlowSelection {
        run_id: IndexRunId::from_bytes(decode_stable_id(&s.run_id).map_err(|_| invalid())?),
        root: SymbolId::from_bytes(decode_stable_id(&s.root).map_err(|_| invalid())?),
        call_path: s
            .call_path
            .iter()
            .copied()
            .map(FlowStepId::new)
            .collect::<Result<_, _>>()
            .map_err(|_| invalid())?,
    })
}
fn encode_selection(s: &FunctionFlowSelection) -> FunctionFlowSelectionV1 {
    FunctionFlowSelectionV1 {
        run_id: s.run_id.to_string(),
        root: s.root.to_string(),
        call_path: s.call_path.iter().map(|i| i.get()).collect(),
    }
}
fn entry(
    owner: &GraphSymbol,
    selection: FunctionFlowSelection,
    modules: &BTreeMap<SymbolId, ModuleId>,
) -> FunctionFlowEntryV1 {
    let category = if owner.parsed().roles().contains(SymbolRole::Test) {
        FunctionFlowCategoryV1::Test
    } else if owner.parsed().kind() == SymbolKind::Module
        || owner.parsed().name().as_str().starts_with("scripts:")
    {
        FunctionFlowCategoryV1::Script
    } else if owner.parsed().roles().contains(SymbolRole::Entrypoint) {
        FunctionFlowCategoryV1::Entrypoint
    } else {
        FunctionFlowCategoryV1::Function
    };
    FunctionFlowEntryV1 {
        selection: encode_selection(&selection),
        name: owner.parsed().name().as_str().to_owned(),
        category,
        source: source(owner, modules),
    }
}
fn source(owner: &GraphSymbol, modules: &BTreeMap<SymbolId, ModuleId>) -> FunctionFlowSourceV1 {
    let ids = modules.get(&owner.id()).map(|module| {
        (
            module.to_string(),
            owner.id().to_string(),
            crate::encode_hex(ModuleCardEvidenceId::for_symbol_v1(owner).as_bytes()),
        )
    });
    FunctionFlowSourceV1 {
        path: String::from_utf8_lossy(owner.revision().path().as_bytes()).into_owned(),
        line: owner.parsed().declaration_range().start_position().row() + 1,
        preview: ids
            .as_ref()
            .map(|(m, s, e)| ProjectMapIndexEvidenceSelectionV1::Symbol {
                module_id: m.clone(),
                symbol_id: s.clone(),
                evidence_id: e.clone(),
            }),
        map_selection: ids.map(|(m, s, e)| ProjectMapEntitySelectionV1::Symbol {
            module_id: m,
            symbol_id: s,
            evidence_id: e,
        }),
    }
}
fn response(result: FunctionFlowsResultV1) -> FunctionFlowsResponseV1 {
    FunctionFlowsResponseV1 {
        protocol_version: ProtocolVersion::CURRENT,
        result,
    }
}
fn invalid() -> CommandErrorV1 {
    CommandErrorV1::project_open(ErrorCodeV1::InvalidFunctionFlowQuery)
}
fn unavailable() -> CommandErrorV1 {
    CommandErrorV1::project_open(ErrorCodeV1::FunctionFlowUnavailable)
}
fn map_error(e: FunctionFlowReadFailure) -> CommandErrorV1 {
    match e {
        FunctionFlowReadFailure::InvalidQuery => invalid(),
        FunctionFlowReadFailure::Storage(_) => unavailable(),
    }
}

fn step_kind(kind: a3_domain::FlowStepKind) -> FunctionFlowStepKindV1 {
    match kind {
        a3_domain::FlowStepKind::Process => FunctionFlowStepKindV1::Process,
        a3_domain::FlowStepKind::Call => FunctionFlowStepKindV1::Call,
        a3_domain::FlowStepKind::Assign => FunctionFlowStepKindV1::Assign,
        a3_domain::FlowStepKind::Condition => FunctionFlowStepKindV1::Condition,
        a3_domain::FlowStepKind::Branch => FunctionFlowStepKindV1::Branch,
        a3_domain::FlowStepKind::Loop => FunctionFlowStepKindV1::Loop,
        a3_domain::FlowStepKind::Return => FunctionFlowStepKindV1::Return,
        a3_domain::FlowStepKind::Throw => FunctionFlowStepKindV1::Throw,
        a3_domain::FlowStepKind::Break => FunctionFlowStepKindV1::Break,
        a3_domain::FlowStepKind::Continue => FunctionFlowStepKindV1::Continue,
        a3_domain::FlowStepKind::Await => FunctionFlowStepKindV1::Await,
        a3_domain::FlowStepKind::Handler => FunctionFlowStepKindV1::Handler,
        a3_domain::FlowStepKind::Deferred => FunctionFlowStepKindV1::Deferred,
        a3_domain::FlowStepKind::Unknown => FunctionFlowStepKindV1::Unknown,
    }
}

fn value_kind(kind: a3_domain::FlowValueKind) -> FunctionFlowValueKindV1 {
    match kind {
        a3_domain::FlowValueKind::ScriptArgument => FunctionFlowValueKindV1::ScriptArgument,
        a3_domain::FlowValueKind::Parameter => FunctionFlowValueKindV1::Parameter,
        a3_domain::FlowValueKind::Local => FunctionFlowValueKindV1::Local,
        a3_domain::FlowValueKind::External => FunctionFlowValueKindV1::External,
        a3_domain::FlowValueKind::CallResult => FunctionFlowValueKindV1::CallResult,
        a3_domain::FlowValueKind::Merge => FunctionFlowValueKindV1::Merge,
    }
}

fn gap_kind(kind: a3_domain::FlowGapKind) -> FunctionFlowGapKindV1 {
    match kind {
        a3_domain::FlowGapKind::Unsupported => FunctionFlowGapKindV1::Unsupported,
        a3_domain::FlowGapKind::Dynamic => FunctionFlowGapKindV1::Dynamic,
        a3_domain::FlowGapKind::Limit => FunctionFlowGapKindV1::Limit,
        a3_domain::FlowGapKind::ParseError => FunctionFlowGapKindV1::ParseError,
    }
}

#[cfg(test)]
mod boundary_tests {
    use super::*;
    #[test]
    fn source_and_trace_accept_only_bounded_opaque_occurrences() {
        let selection = FunctionFlowSelectionV1 {
            run_id: "a".repeat(64),
            root: "b".repeat(64),
            call_path: vec![1; 7],
        };
        assert!(
            validate(&FunctionFlowQueryV1::Source {
                selection: selection.clone(),
                step: 4096
            })
            .is_ok()
        );
        for step in [0, 4097, u32::MAX] {
            assert!(
                validate(&FunctionFlowQueryV1::Source {
                    selection: selection.clone(),
                    step
                })
                .is_err()
            );
        }
        let mut deeper = selection.clone();
        deeper.call_path.push(1);
        assert!(
            validate(&FunctionFlowQueryV1::Source {
                selection: deeper,
                step: 1
            })
            .is_err()
        );
        let mut forged = selection;
        forged.root = "../../private".to_owned();
        assert!(
            validate(&FunctionFlowQueryV1::Source {
                selection: forged,
                step: 1
            })
            .is_err()
        );
        let raw = serde_json::json!({"protocolVersion":1,"query":{"kind":"source","selection":{"runId":"a".repeat(64),"root":"b".repeat(64),"callPath":[]},"step":1,"path":"secret.txt"}});
        assert!(serde_json::from_value::<QueryFunctionFlowsRequestV1>(raw).is_err());
    }
}
