use super::{
    BoundedControl, ExploreFunctionFlows, FUNCTION_FLOW_MAX_CONTEXTS, FUNCTION_FLOW_PAGE_SIZE,
    FunctionFlowFrame, FunctionFlowReadFailure, FunctionFlowSelection,
};
use crate::IndexPersistenceControl;
use a3_domain::{
    EvidenceRef, FlowStepId, FlowStepKind, FlowValueId, FlowValueKind, GraphSymbol, IndexRunId,
    ProjectIdentity, SymbolId,
};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

#[derive(Default)]
struct ReadBudget {
    scanned: AtomicUsize,
    exhausted: AtomicBool,
}
impl ReadBudget {
    fn tick(&self) -> bool {
        let scanned = self
            .scanned
            .fetch_add(1, Ordering::Relaxed)
            .saturating_add(1);
        if scanned > 4096 {
            self.exhausted.store(true, Ordering::Relaxed);
            false
        } else {
            true
        }
    }
}

/// Direction relative to a selected value occurrence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlowTraceDirection {
    /// Follow definitions and actual arguments.
    Origins,
    /// Follow local uses, formal arguments, and returns.
    Uses,
}
/// One value in one exact call context; repeated callees never share an address.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct FlowValueAddress {
    /// Occurrence path from the selected root.
    pub call_path: Vec<FlowStepId>,
    /// Function-local value version.
    pub value: FlowValueId,
}
/// One evidence-grounded value in the bounded trace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlowValueTraceNode {
    /// Exact context and definition.
    pub address: FlowValueAddress,
    /// Current source-derived callable name.
    pub function_name: String,
    /// Source binding or synthetic result label.
    pub name: String,
    /// How this value entered the function.
    pub kind: FlowValueKind,
    /// Definition source, never a claimed runtime value.
    pub evidence: EvidenceRef,
    /// Adjacent definitions in the requested direction.
    pub next: Vec<FlowValueAddress>,
    /// External, dynamic or incompletely modeled effects.
    pub unknown: bool,
}
/// A finite context-sensitive trace, never a claimed runtime execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlowValueTrace {
    /// Direction selected by the caller.
    pub direction: FlowTraceDirection,
    /// At most fifty value occurrences.
    pub nodes: Vec<FlowValueTraceNode>,
    /// Every participating callable revision, including argument-mapping context.
    pub evidence: Vec<EvidenceRef>,
    /// Fixed node/context/edge budgets prevented exhaustive exploration.
    pub truncated: bool,
}
struct Session<'a> {
    explorer: &'a ExploreFunctionFlows,
    project: &'a ProjectIdentity,
    run: IndexRunId,
    owners: BTreeMap<SymbolId, &'a GraphSymbol>,
    frames: BTreeMap<Vec<FlowStepId>, FunctionFlowFrame>,
    control: &'a BoundedControl<'a>,
    budget: &'a ReadBudget,
    truncated: bool,
}
impl Session<'_> {
    fn budget(&mut self) -> bool {
        if !self.budget.tick() {
            self.truncated = true;
            false
        } else {
            true
        }
    }
    async fn child(
        &mut self,
        path: &[FlowStepId],
        call: FlowStepId,
    ) -> Result<Option<Vec<FlowStepId>>, FunctionFlowReadFailure> {
        let mut next = path.to_vec();
        next.push(call);
        if self.frames.contains_key(&next) {
            return Ok(Some(next));
        }
        if next.len() >= FUNCTION_FLOW_MAX_CONTEXTS
            || self.frames.len() >= FUNCTION_FLOW_MAX_CONTEXTS
        {
            self.truncated = true;
            return Ok(None);
        }
        let target = self
            .frames
            .get(path)
            .and_then(|f| f.flow.calls().iter().find(|c| c.step == call))
            .and_then(|c| c.target);
        let Some(owner) = target.and_then(|t| self.owners.get(&t)).copied() else {
            return Ok(None);
        };
        self.control.check()?;
        let Some(flow) = self
            .explorer
            .store
            .read_function_flow(self.project, self.run, owner, self.control)
            .await?
        else {
            return Ok(None);
        };
        self.frames.insert(
            next.clone(),
            FunctionFlowFrame {
                owner: owner.clone(),
                flow,
            },
        );
        Ok(Some(next))
    }
}
impl ExploreFunctionFlows {
    /// Traces arguments and returns by call occurrence, with explicit unknown boundaries.
    pub async fn trace_value(
        &self,
        project: &ProjectIdentity,
        selection: &FunctionFlowSelection,
        value: FlowValueId,
        direction: FlowTraceDirection,
        control: &dyn IndexPersistenceControl,
    ) -> Result<Option<FlowValueTrace>, FunctionFlowReadFailure> {
        let bounded = BoundedControl::new(control);
        let Some(index) = self.store.latest_published_index(project, &bounded).await? else {
            return Ok(None);
        };
        if index.run().id() != selection.run_id {
            return Ok(None);
        }
        let Some(inspection) = self.inspect(project, selection, &bounded).await? else {
            return Ok(None);
        };
        let frames = inspection
            .frames
            .into_iter()
            .enumerate()
            .map(|(i, f)| (selection.call_path[..i].to_vec(), f))
            .collect();
        let budget = ReadBudget::default();
        let mut session = Session {
            explorer: self,
            project,
            run: selection.run_id,
            owners: index
                .publication()
                .graph()
                .symbols()
                .iter()
                .map(|s| (s.id(), s))
                .collect(),
            frames,
            control: &bounded,
            budget: &budget,
            truncated: false,
        };
        let start = FlowValueAddress {
            call_path: selection.call_path.clone(),
            value,
        };
        let mut queue = VecDeque::from([start]);
        let mut seen = BTreeSet::new();
        let mut nodes = Vec::new();
        while let Some(address) = queue.pop_front() {
            bounded.check()?;
            if !seen.insert(address.clone()) {
                continue;
            }
            if nodes.len() >= FUNCTION_FLOW_PAGE_SIZE || !session.budget() {
                session.truncated = true;
                break;
            }
            let Some(frame) = session.frames.get(&address.call_path).cloned() else {
                session.truncated = true;
                continue;
            };
            let flow = frame.flow.analysis();
            let Some(value) = flow
                .values()
                .iter()
                .take_while(|_| budget.tick())
                .find(|v| v.id == address.value)
            else {
                if budget.exhausted.load(Ordering::Relaxed) {
                    break;
                }
                return Err(FunctionFlowReadFailure::InvalidQuery);
            };
            let mut neighbors = BTreeSet::new();
            let mut unknown = value.kind == FlowValueKind::External || !flow.gaps().is_empty();
            match direction {
                FlowTraceDirection::Origins => {
                    neighbors.extend(
                        value
                            .dependencies
                            .iter()
                            .take_while(|_| budget.tick())
                            .copied()
                            .map(|value| FlowValueAddress {
                                call_path: address.call_path.clone(),
                                value,
                            }),
                    );
                    if value.kind == FlowValueKind::CallResult
                        && let Some(step) = value.producer
                    {
                        let process = flow
                            .steps()
                            .iter()
                            .take_while(|_| budget.tick())
                            .any(|s| s.id == step && s.kind == FlowStepKind::Process);
                        if process {
                            unknown = true;
                        } else if let Some(child) = session.child(&address.call_path, step).await? {
                            if let Some(target) = session.frames.get(&child) {
                                let deferred = target
                                    .flow
                                    .analysis()
                                    .steps()
                                    .first()
                                    .is_some_and(|s| s.kind == FlowStepKind::Deferred);
                                let awaited =
                                    flow.steps().iter().take_while(|_| budget.tick()).any(|s| {
                                        s.kind == FlowStepKind::Await
                                            && s.inputs
                                                .iter()
                                                .take_while(|_| budget.tick())
                                                .any(|id| *id == value.id)
                                    });
                                unknown |= deferred && !awaited;
                                for returned in target
                                    .flow
                                    .analysis()
                                    .steps()
                                    .iter()
                                    .take_while(|_| budget.tick())
                                    .filter(|s| {
                                        s.kind == FlowStepKind::Return && (!deferred || awaited)
                                    })
                                {
                                    neighbors.extend(
                                        returned
                                            .inputs
                                            .iter()
                                            .take_while(|_| budget.tick())
                                            .copied()
                                            .map(|value| FlowValueAddress {
                                                call_path: child.clone(),
                                                value,
                                            }),
                                    );
                                }
                            }
                        } else {
                            unknown = true;
                        }
                    }
                    if let Some(slot) = value.script_argument {
                        unknown = true;
                        for depth in (0..address.call_path.len()).rev() {
                            let parent_path = &address.call_path[..depth];
                            let call = address.call_path[depth];
                            if let Some(process) = session
                                .frames
                                .get(parent_path)
                                .and_then(|p| {
                                    p.flow
                                        .analysis()
                                        .steps()
                                        .iter()
                                        .take_while(|_| budget.tick())
                                        .find(|s| s.id == call)
                                })
                                .and_then(|s| s.process.as_ref())
                            {
                                if let Some(argument) = process.arguments.get(usize::from(slot)) {
                                    neighbors.extend(
                                        argument.values.iter().take_while(|_| budget.tick()).map(
                                            |id| FlowValueAddress {
                                                call_path: parent_path.to_vec(),
                                                value: *id,
                                            },
                                        ),
                                    );
                                    unknown = !flow.gaps().is_empty() || process.target.is_none();
                                }
                                break;
                            }
                        }
                    }
                    if value.kind == FlowValueKind::Parameter {
                        if let Some((call, parent_path)) = address.call_path.split_last() {
                            if let Some(parent) = session.frames.get(parent_path) {
                                let params = flow
                                    .values()
                                    .iter()
                                    .take_while(|_| budget.tick())
                                    .filter(|v| v.kind == FlowValueKind::Parameter)
                                    .collect::<Vec<_>>();
                                let ordinal = params
                                    .iter()
                                    .take_while(|_| budget.tick())
                                    .position(|v| v.id == value.id);
                                let argument = parent
                                    .flow
                                    .analysis()
                                    .steps()
                                    .iter()
                                    .take_while(|_| budget.tick())
                                    .find(|s| s.id == *call)
                                    .filter(|s| {
                                        s.kind == FlowStepKind::Call
                                            && bindings_known(
                                                flow,
                                                parent.flow.analysis(),
                                                s,
                                                &budget,
                                            )
                                    })
                                    .and_then(|s| {
                                        s.arguments
                                            .iter()
                                            .take_while(|_| budget.tick())
                                            .find(|a| {
                                                a.keyword.as_ref().is_some_and(|k| {
                                                    k.as_str() == value.name.as_str()
                                                })
                                            })
                                            .or_else(|| {
                                                ordinal.and_then(|i| {
                                                    s.arguments
                                                        .iter()
                                                        .take_while(|_| budget.tick())
                                                        .filter(|a| a.keyword.is_none())
                                                        .nth(i)
                                                })
                                            })
                                    });
                                if let Some(argument) = argument {
                                    neighbors.extend(
                                        argument
                                            .values
                                            .iter()
                                            .take_while(|_| budget.tick())
                                            .copied()
                                            .map(|value| FlowValueAddress {
                                                call_path: parent_path.to_vec(),
                                                value,
                                            }),
                                    );
                                } else {
                                    unknown = true;
                                }
                            }
                        } else {
                            unknown = true;
                        }
                    }
                }
                FlowTraceDirection::Uses => {
                    for dependent in flow.values() {
                        if !session.budget() {
                            break;
                        }
                        if dependent
                            .dependencies
                            .iter()
                            .take_while(|_| budget.tick())
                            .any(|id| *id == value.id)
                        {
                            neighbors.insert(FlowValueAddress {
                                call_path: address.call_path.clone(),
                                value: dependent.id,
                            });
                        }
                    }
                    for step in flow.steps() {
                        if !session.budget() {
                            break;
                        }
                        if step.kind == FlowStepKind::Return
                            && step
                                .inputs
                                .iter()
                                .take_while(|_| budget.tick())
                                .any(|id| *id == value.id)
                            && let Some((call, parent_path)) = address.call_path.split_last()
                            && let Some(parent) = session.frames.get(parent_path)
                            && parent
                                .flow
                                .analysis()
                                .steps()
                                .iter()
                                .take_while(|_| budget.tick())
                                .any(|s| s.id == *call && s.kind == FlowStepKind::Call)
                            && (!flow
                                .steps()
                                .first()
                                .is_some_and(|s| s.kind == FlowStepKind::Deferred)
                                || parent
                                    .flow
                                    .analysis()
                                    .steps()
                                    .iter()
                                    .take_while(|_| budget.tick())
                                    .any(|s| {
                                        s.kind == FlowStepKind::Await
                                            && s.inputs.iter().take_while(|_| budget.tick()).any(
                                                |id| {
                                                    parent
                                                        .flow
                                                        .analysis()
                                                        .values()
                                                        .iter()
                                                        .take_while(|_| budget.tick())
                                                        .any(|v| {
                                                            v.id == *id && v.producer == Some(*call)
                                                        })
                                                },
                                            )
                                    }))
                        {
                            neighbors.extend(
                                parent
                                    .flow
                                    .analysis()
                                    .values()
                                    .iter()
                                    .take_while(|_| budget.tick())
                                    .filter(|v| {
                                        v.kind == FlowValueKind::CallResult
                                            && v.producer == Some(*call)
                                    })
                                    .map(|v| FlowValueAddress {
                                        call_path: parent_path.to_vec(),
                                        value: v.id,
                                    }),
                            );
                        }
                        if let Some(process) = &step.process {
                            if process
                                .arguments
                                .iter()
                                .take_while(|_| budget.tick())
                                .any(|a| {
                                    a.values
                                        .iter()
                                        .take_while(|_| budget.tick())
                                        .any(|id| *id == value.id)
                                })
                            {
                                if let Some(child) =
                                    session.child(&address.call_path, step.id).await?
                                {
                                    if let Some(target) = session.frames.get(&child) {
                                        for parameter in target.flow.analysis().values() {
                                            if parameter.script_argument.is_some_and(|slot| {
                                                process
                                                    .arguments
                                                    .get(usize::from(slot))
                                                    .is_some_and(|a| {
                                                        a.values
                                                            .iter()
                                                            .take_while(|_| budget.tick())
                                                            .any(|id| *id == value.id)
                                                    })
                                            }) {
                                                neighbors.insert(FlowValueAddress {
                                                    call_path: child.clone(),
                                                    value: parameter.id,
                                                });
                                            }
                                        }
                                    }
                                } else {
                                    unknown = true;
                                }
                            }
                            continue;
                        }
                        if step.kind != FlowStepKind::Call
                            || !step
                                .arguments
                                .iter()
                                .take_while(|_| budget.tick())
                                .any(|a| {
                                    a.values
                                        .iter()
                                        .take_while(|_| budget.tick())
                                        .any(|id| *id == value.id)
                                })
                        {
                            continue;
                        }
                        let Some(child) = session.child(&address.call_path, step.id).await? else {
                            unknown = true;
                            continue;
                        };
                        if let Some(target) = session.frames.get(&child) {
                            if !bindings_known(target.flow.analysis(), flow, step, &budget) {
                                unknown = true;
                                continue;
                            }
                            let params = target
                                .flow
                                .analysis()
                                .values()
                                .iter()
                                .take_while(|_| budget.tick())
                                .filter(|v| v.kind == FlowValueKind::Parameter)
                                .collect::<Vec<_>>();
                            for (ordinal, arg) in step
                                .arguments
                                .iter()
                                .take_while(|_| budget.tick())
                                .enumerate()
                                .filter(|(_, a)| {
                                    a.values
                                        .iter()
                                        .take_while(|_| budget.tick())
                                        .any(|id| *id == value.id)
                                })
                            {
                                let param = arg.keyword.as_ref().map_or_else(
                                    || params.get(ordinal).copied(),
                                    |key| {
                                        params
                                            .iter()
                                            .take_while(|_| budget.tick())
                                            .copied()
                                            .find(|v| v.name.as_str() == key.as_str())
                                    },
                                );
                                if let Some(param) = param {
                                    neighbors.insert(FlowValueAddress {
                                        call_path: child.clone(),
                                        value: param.id,
                                    });
                                } else {
                                    unknown = true;
                                }
                            }
                        }
                    }
                }
            }
            if neighbors.len() > FUNCTION_FLOW_PAGE_SIZE {
                session.truncated = true;
            }
            let neighbors = neighbors
                .into_iter()
                .take(FUNCTION_FLOW_PAGE_SIZE)
                .collect::<Vec<_>>();
            for neighbor in &neighbors {
                if !seen.contains(neighbor) && !queue.contains(neighbor) {
                    if queue.len() + nodes.len() < FUNCTION_FLOW_PAGE_SIZE {
                        queue.push_back(neighbor.clone());
                    } else {
                        session.truncated = true;
                    }
                }
            }
            nodes.push(FlowValueTraceNode {
                address,
                function_name: frame.owner.parsed().name().as_str().to_owned(),
                name: value.name.as_str().to_owned(),
                kind: value.kind,
                evidence: EvidenceRef::new(frame.owner.revision().clone(), value.range),
                next: neighbors,
                unknown: unknown || budget.exhausted.load(Ordering::Relaxed),
            });
        }
        a3_domain::FunctionFlowBatch::new(
            index.publication(),
            session
                .frames
                .values()
                .map(|f| (f.flow.symbol(), f.flow.clone()))
                .collect::<BTreeMap<_, _>>()
                .into_values()
                .collect(),
        )
        .map_err(|_| {
            FunctionFlowReadFailure::Storage(crate::KnowledgeIndexFailure::IndexPublicationMismatch)
        })?;
        if !self.is_current(project, &index, &bounded).await? {
            return Ok(None);
        }
        let evidence = session
            .frames
            .values()
            .map(|f| EvidenceRef::new(f.owner.revision().clone(), f.flow.analysis().range()))
            .collect();
        Ok(Some(FlowValueTrace {
            direction,
            nodes,
            evidence,
            truncated: session.truncated || budget.exhausted.load(Ordering::Relaxed),
        }))
    }
}

fn bindings_known(
    target: &a3_domain::FunctionFlow,
    caller: &a3_domain::FunctionFlow,
    step: &a3_domain::FlowStep,
    budget: &ReadBudget,
) -> bool {
    let known = !target.gaps().iter().take_while(|_| budget.tick()).any(|g| {
        g.kind == a3_domain::FlowGapKind::Unsupported
            && target
                .values()
                .iter()
                .take_while(|_| budget.tick())
                .any(|v| v.kind == FlowValueKind::Parameter && g.range.contains(v.range))
    }) && !caller.gaps().iter().take_while(|_| budget.tick()).any(|g| {
        g.kind == a3_domain::FlowGapKind::Dynamic
            && step
                .arguments
                .iter()
                .take_while(|_| budget.tick())
                .any(|a| a.range.contains(g.range))
    });
    known && !budget.exhausted.load(Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn edge_budget_never_reopens_after_exhaustion() {
        let budget = ReadBudget::default();
        for _ in 0..4096 {
            assert!(budget.tick());
        }
        for _ in 0..100 {
            assert!(!budget.tick());
        }
        assert!(budget.exhausted.load(Ordering::Relaxed));
    }
}
