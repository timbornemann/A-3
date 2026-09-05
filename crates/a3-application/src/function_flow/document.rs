use super::{
    ExploreFunctionFlows, FlowTraceDirection, FunctionFlowReadFailure, FunctionFlowSelection,
};
use crate::IndexPersistenceControl;
use a3_domain::{
    EvidenceRef, FunctionFlowReadRequest, FunctionFlowReadView, IndexRunId, ProjectIdentity,
};

/// A bounded deterministic tool page with every source dependency retained separately.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionFlowReadDocument {
    /// Untrusted source-derived labels and typed analysis facts, never a runtime trace.
    pub text: String,
    /// Evidence labels in text use their zero-based position in this list.
    pub evidence: Vec<EvidenceRef>,
    /// A page, graph, or byte budget omitted additional information.
    pub truncated: bool,
}
impl ExploreFunctionFlows {
    /// Formats the same targeted reads used by the UI for bounded harness tools.
    pub async fn read_document(
        &self,
        project: &ProjectIdentity,
        run_id: IndexRunId,
        request: &FunctionFlowReadRequest,
        control: &dyn IndexPersistenceControl,
    ) -> Result<Option<FunctionFlowReadDocument>, FunctionFlowReadFailure> {
        let bounded = super::BoundedControl::new(control);
        let control = &bounded as &dyn IndexPersistenceControl;
        let selection = FunctionFlowSelection {
            run_id,
            root: request.root(),
            call_path: request.call_path().to_vec(),
        };
        let Some(inspection) = self.inspect(project, &selection, control).await? else {
            return Ok(None);
        };
        let Some(frame) = inspection.frames.last() else {
            return Ok(None);
        };
        let mut document = FunctionFlowReadDocument {
            text: format!(
                "FUNCTION_FLOW_V1 run={} root={} call_path={:?}\nStatic possibilities, not observed execution; omitted or unknown effects are not absent effects.\n",
                run_id,
                request.root(),
                request
                    .call_path()
                    .iter()
                    .map(|id| id.get())
                    .collect::<Vec<_>>()
            ),
            evidence: inspection.evidence(),
            truncated: false,
        };
        match request.view() {
            FunctionFlowReadView::Steps(offset) => {
                document.truncated = frame.flow.analysis().steps().len() > usize::from(offset) + 50;
                for step in frame
                    .flow
                    .analysis()
                    .steps()
                    .iter()
                    .skip(usize::from(offset))
                    .take(50)
                {
                    let target = frame
                        .flow
                        .calls()
                        .iter()
                        .find(|c| c.step == step.id)
                        .and_then(|c| c.target);
                    let partial = step.inputs.len() > 50 || step.outputs.len() > 50;
                    document.truncated |= partial;
                    if !document.line(format!("STEP id={} parent={:?} kind={:?} name={:?} target={:?} process_mode={:?} inputs={:?} outputs={:?} partial_values={}\n",step.id.get(),step.parent.map(|p|p.get()),step.kind,step.name.as_ref().map(|n|n.as_str()),target,step.process.as_ref().map(|p|p.mode),step.inputs.iter().take(50).map(|v|v.get()).collect::<Vec<_>>(),step.outputs.iter().take(50).map(|v|v.get()).collect::<Vec<_>>(),partial),EvidenceRef::new(frame.owner.revision().clone(),step.range)) {break;}
                }
            }
            FunctionFlowReadView::Values(offset) => {
                document.truncated =
                    frame.flow.analysis().values().len() > usize::from(offset) + 50;
                for value in frame
                    .flow
                    .analysis()
                    .values()
                    .iter()
                    .skip(usize::from(offset))
                    .take(50)
                {
                    if !document.line(
                        format!(
                            "VALUE id={} name={:?} kind={:?} producer={:?} script_argument={:?}\n",
                            value.id.get(),
                            value.name.as_str(),
                            value.kind,
                            value.producer.map(|p| p.get()),
                            value.script_argument
                        ),
                        EvidenceRef::new(frame.owner.revision().clone(), value.range),
                    ) {
                        break;
                    }
                }
            }
            FunctionFlowReadView::Origins(value) | FunctionFlowReadView::Uses(value) => {
                let direction = if matches!(request.view(), FunctionFlowReadView::Origins(_)) {
                    FlowTraceDirection::Origins
                } else {
                    FlowTraceDirection::Uses
                };
                let Some(trace) = self
                    .trace_value(project, &selection, value, direction, control)
                    .await?
                else {
                    return Ok(None);
                };
                document.evidence = trace.evidence;
                document.truncated = trace.truncated;
                for node in trace.nodes {
                    if !document.line(format!("TRACE direction={direction:?} context={:?} value={} function={:?} name={:?} kind={:?} unknown={} next={:?}\n",node.address.call_path.iter().map(|s|s.get()).collect::<Vec<_>>(),node.address.value.get(),node.function_name,node.name,node.kind,node.unknown,node.next.iter().map(|a|(a.call_path.iter().map(|s|s.get()).collect::<Vec<_>>(),a.value.get())).collect::<Vec<_>>()),node.evidence) {break;}
                }
            }
        }
        // Every gap is explicit even when its full detail exceeds this page.
        let gaps = frame.flow.analysis().gaps();
        document.text.push_str(&format!(
            "COVERAGE known_gaps={} kinds={:?} page_truncated={}\n",
            gaps.len(),
            gaps.iter()
                .map(|g| g.kind)
                .collect::<std::collections::BTreeSet<_>>(),
            document.truncated
        ));
        bounded.check()?;
        Ok(Some(document))
    }
}
impl FunctionFlowReadDocument {
    fn line(&mut self, line: String, evidence: EvidenceRef) -> bool {
        if self.text.len() + line.len() + 128 > 12 * 1024 {
            self.truncated = true;
            return false;
        }
        let ordinal = self
            .evidence
            .iter()
            .position(|e| e == &evidence)
            .unwrap_or(self.evidence.len());
        if ordinal == self.evidence.len() {
            self.evidence.push(evidence);
        }
        self.text.push_str(&format!("flow_source={ordinal} "));
        self.text.push_str(&line);
        true
    }
}
