use crate::index_publication::IndexPublicationRepositoryError as Error;
use a3_domain::{
    FlowArgument, FlowCallLink, FlowGap, FlowGapKind, FlowProcess, FlowProcessMode,
    FlowProcessTarget, FlowStep, FlowStepId, FlowStepKind, FlowValue, FlowValueId, FlowValueKind,
    FunctionFlow, FunctionFlowError, GraphSymbol, IndexedFunctionFlow, RepositoryPath,
    SourcePosition, SourceRange, SymbolId, SymbolName, SymbolReference,
};
use serde::{Deserialize, Serialize};

pub(crate) const MAX_BODY_BYTES: usize = 8 * 1024 * 1024;
const STEPS: &[FlowStepKind] = &[
    FlowStepKind::Call,
    FlowStepKind::Assign,
    FlowStepKind::Condition,
    FlowStepKind::Branch,
    FlowStepKind::Loop,
    FlowStepKind::Return,
    FlowStepKind::Throw,
    FlowStepKind::Break,
    FlowStepKind::Continue,
    FlowStepKind::Await,
    FlowStepKind::Handler,
    FlowStepKind::Deferred,
    FlowStepKind::Unknown,
    FlowStepKind::Process,
];
const VALUES: &[FlowValueKind] = &[
    FlowValueKind::Parameter,
    FlowValueKind::Local,
    FlowValueKind::External,
    FlowValueKind::CallResult,
    FlowValueKind::Merge,
    FlowValueKind::ScriptArgument,
];
const GAPS: &[FlowGapKind] = &[
    FlowGapKind::Unsupported,
    FlowGapKind::Dynamic,
    FlowGapKind::Limit,
    FlowGapKind::ParseError,
];
type Range = [u32; 6];
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Body {
    range: Range,
    lexical_scope: Range,
    steps: Vec<Step>,
    values: Vec<Value>,
    gaps: Vec<(u8, Range)>,
    calls: Vec<(u32, Option<[u8; 32]>)>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Step {
    id: u32,
    kind: u8,
    parent: Option<u32>,
    range: Range,
    name: Option<String>,
    callee_range: Option<Range>,
    process: Option<Process>,
    inputs: Vec<u32>,
    outputs: Vec<u32>,
    arguments: Vec<Argument>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Argument {
    keyword: Option<String>,
    values: Vec<u32>,
    range: Range,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Process {
    mode: u8,
    target: Option<(u8, Vec<u8>)>,
    arguments: Vec<Argument>,
}
fn encode_argument(a: &FlowArgument) -> Argument {
    Argument {
        keyword: a.keyword.as_ref().map(|n| n.as_str().to_owned()),
        values: a.values.iter().map(|v| v.get()).collect(),
        range: encoded_range(a.range),
    }
}
fn decode_argument(a: Argument) -> Result<FlowArgument, FunctionFlowError> {
    Ok(FlowArgument {
        keyword: a
            .keyword
            .map(SymbolName::try_from_string)
            .transpose()
            .map_err(|_| FunctionFlowError::InvalidIdentity)?,
        values: ids(a.values)?,
        range: decoded_range(a.range)?,
    })
}
const MODES: &[FlowProcessMode] = &[
    FlowProcessMode::Wait,
    FlowProcessMode::Spawn,
    FlowProcessMode::CompileOnly,
];
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Value {
    id: u32,
    name: String,
    kind: u8,
    range: Range,
    scope: Range,
    dependencies: Vec<u32>,
    producer: Option<u32>,
    script_argument: Option<u16>,
}
fn encoded_range(r: SourceRange) -> Range {
    [
        r.start_byte(),
        r.end_byte(),
        r.start_position().row(),
        r.start_position().column(),
        r.end_position().row(),
        r.end_position().column(),
    ]
}
fn decoded_range(r: Range) -> Result<SourceRange, FunctionFlowError> {
    SourceRange::new(
        r[0] as usize,
        r[1] as usize,
        SourcePosition::new(r[2], r[3]),
        SourcePosition::new(r[4], r[5]),
    )
    .map_err(|_| FunctionFlowError::OutsideOwner)
}
fn code<T: PartialEq>(k: &T, kinds: &[T]) -> Result<u8, Error> {
    kinds
        .iter()
        .position(|x| x == k)
        .and_then(|i| u8::try_from(i).ok())
        .ok_or(Error::InvalidStoredData)
}
fn kind<T: Copy>(code: u8, kinds: &[T]) -> Result<T, FunctionFlowError> {
    kinds
        .get(usize::from(code))
        .copied()
        .ok_or(FunctionFlowError::InvalidIdentity)
}
fn ids(values: Vec<u32>) -> Result<Vec<FlowValueId>, FunctionFlowError> {
    values.into_iter().map(FlowValueId::new).collect()
}

pub(crate) fn encode(flow: &IndexedFunctionFlow) -> Result<String, Error> {
    let analysis = flow.analysis();
    let body = Body {
        range: encoded_range(analysis.range()),
        lexical_scope: encoded_range(analysis.lexical_scope()),
        steps: analysis
            .steps()
            .iter()
            .map(|s| {
                Ok(Step {
                    id: s.id.get(),
                    kind: code(&s.kind, STEPS)?,
                    parent: s.parent.map(FlowStepId::get),
                    range: encoded_range(s.range),
                    name: s.name.as_ref().map(|n| n.as_str().to_owned()),
                    callee_range: s.callee_range.map(encoded_range),
                    process: s
                        .process
                        .as_ref()
                        .map(|p| {
                            Ok(Process {
                                mode: code(&p.mode, MODES)?,
                                target: p.target.as_ref().map(|t| match t {
                                    FlowProcessTarget::File(path) => (0, path.as_bytes().to_vec()),
                                    FlowProcessTarget::PackageScript(name) => {
                                        (1, name.as_str().as_bytes().to_vec())
                                    }
                                }),
                                arguments: p.arguments.iter().map(encode_argument).collect(),
                            })
                        })
                        .transpose()?,
                    inputs: s.inputs.iter().map(|i| i.get()).collect(),
                    outputs: s.outputs.iter().map(|i| i.get()).collect(),
                    arguments: s
                        .arguments
                        .iter()
                        .map(|a| Argument {
                            keyword: a.keyword.as_ref().map(|n| n.as_str().to_owned()),
                            values: a.values.iter().map(|i| i.get()).collect(),
                            range: encoded_range(a.range),
                        })
                        .collect(),
                })
            })
            .collect::<Result<_, Error>>()?,
        values: analysis
            .values()
            .iter()
            .map(|v| {
                Ok(Value {
                    id: v.id.get(),
                    name: v.name.as_str().to_owned(),
                    kind: code(&v.kind, VALUES)?,
                    range: encoded_range(v.range),
                    scope: encoded_range(v.scope),
                    dependencies: v.dependencies.iter().map(|i| i.get()).collect(),
                    producer: v.producer.map(FlowStepId::get),
                    script_argument: v.script_argument,
                })
            })
            .collect::<Result<_, Error>>()?,
        gaps: analysis
            .gaps()
            .iter()
            .map(|g| Ok((code(&g.kind, GAPS)?, encoded_range(g.range))))
            .collect::<Result<_, Error>>()?,
        calls: flow
            .calls()
            .iter()
            .map(|c| (c.step.get(), c.target.map(|s| *s.as_bytes())))
            .collect(),
    };
    let encoded = serde_json::to_string(&body).map_err(|_| Error::InvalidStoredData)?;
    if encoded.len() > MAX_BODY_BYTES {
        return Err(Error::ResourceLimit);
    }
    Ok(encoded)
}
pub(crate) fn decode(owner: &GraphSymbol, body: &str) -> Result<IndexedFunctionFlow, Error> {
    if body.len() > MAX_BODY_BYTES {
        return Err(Error::ResourceLimit);
    }
    let body: Body = serde_json::from_str(body).map_err(|_| Error::InvalidStoredData)?;
    decode_body(owner, body).map_err(|_| Error::InvalidStoredData)
}
fn decode_body(owner: &GraphSymbol, b: Body) -> Result<IndexedFunctionFlow, FunctionFlowError> {
    let count = b.steps.len() + b.values.len() + b.gaps.len() + b.calls.len();
    if count > 2 * a3_domain::MAX_FUNCTION_FLOW_ELEMENTS {
        return Err(FunctionFlowError::Limit);
    }
    let steps = b
        .steps
        .into_iter()
        .map(|s| {
            Ok(FlowStep {
                id: FlowStepId::new(s.id)?,
                kind: kind(s.kind, STEPS)?,
                parent: s.parent.map(FlowStepId::new).transpose()?,
                range: decoded_range(s.range)?,
                name: s
                    .name
                    .map(SymbolReference::try_from_string)
                    .transpose()
                    .map_err(|_| FunctionFlowError::InvalidIdentity)?,
                callee_range: s.callee_range.map(decoded_range).transpose()?,
                process: s
                    .process
                    .map(|p| {
                        Ok(FlowProcess {
                            mode: kind(p.mode, MODES)?,
                            target: p
                                .target
                                .map(|(k, v)| match k {
                                    0 => RepositoryPath::try_from_bytes(v)
                                        .map(FlowProcessTarget::File)
                                        .map_err(|_| FunctionFlowError::InvalidIdentity),
                                    1 => String::from_utf8(v)
                                        .map_err(|_| FunctionFlowError::InvalidIdentity)
                                        .and_then(|name| {
                                            SymbolName::try_from_string(name)
                                                .map(FlowProcessTarget::PackageScript)
                                                .map_err(|_| FunctionFlowError::InvalidIdentity)
                                        }),
                                    _ => Err(FunctionFlowError::InvalidIdentity),
                                })
                                .transpose()?,
                            arguments: p
                                .arguments
                                .into_iter()
                                .map(decode_argument)
                                .collect::<Result<_, FunctionFlowError>>()?,
                        })
                    })
                    .transpose()?,
                inputs: ids(s.inputs)?,
                outputs: ids(s.outputs)?,
                arguments: s
                    .arguments
                    .into_iter()
                    .map(|a| {
                        Ok(FlowArgument {
                            keyword: a
                                .keyword
                                .map(SymbolName::try_from_string)
                                .transpose()
                                .map_err(|_| FunctionFlowError::InvalidIdentity)?,
                            values: ids(a.values)?,
                            range: decoded_range(a.range)?,
                        })
                    })
                    .collect::<Result<_, FunctionFlowError>>()?,
            })
        })
        .collect::<Result<_, FunctionFlowError>>()?;
    let values = b
        .values
        .into_iter()
        .map(|v| {
            Ok(FlowValue {
                id: FlowValueId::new(v.id)?,
                name: SymbolName::try_from_string(v.name)
                    .map_err(|_| FunctionFlowError::InvalidIdentity)?,
                kind: kind(v.kind, VALUES)?,
                range: decoded_range(v.range)?,
                scope: decoded_range(v.scope)?,
                dependencies: ids(v.dependencies)?,
                producer: v.producer.map(FlowStepId::new).transpose()?,
                script_argument: v.script_argument,
            })
        })
        .collect::<Result<_, FunctionFlowError>>()?;
    let gaps = b
        .gaps
        .into_iter()
        .map(|(k, r)| {
            Ok(FlowGap {
                kind: kind(k, GAPS)?,
                range: decoded_range(r)?,
            })
        })
        .collect::<Result<_, FunctionFlowError>>()?;
    let calls = b
        .calls
        .into_iter()
        .map(|(s, t)| {
            Ok(FlowCallLink {
                step: FlowStepId::new(s)?,
                target: t.map(SymbolId::from_bytes),
            })
        })
        .collect::<Result<_, FunctionFlowError>>()?;
    IndexedFunctionFlow::new(
        owner,
        FunctionFlow::new(
            owner.parsed().id(),
            decoded_range(b.range)?,
            steps,
            values,
            gaps,
        )?
        .with_lexical_scope(decoded_range(b.lexical_scope)?)?,
        calls,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn private_codec_round_trips_and_rejects_corrupt_references()
    -> Result<(), Box<dyn std::error::Error>> {
        let range = SourceRange::new(0, 10, SourcePosition::new(0, 0), SourcePosition::new(0, 10))?;
        let owner = GraphSymbol::new(
            SymbolId::from_bytes([1; 32]),
            a3_domain::FileRevision::new(
                RepositoryPath::try_from_bytes(b"a.ts".to_vec())?,
                a3_domain::ContentHash::from_bytes([2; 32]),
            ),
            a3_domain::ParsedSymbol::new(
                a3_domain::LocalSymbolId::new(1)?,
                a3_domain::SymbolKind::Function,
                SymbolName::try_from_string("A".to_owned())?,
                range,
                range,
            )?,
        );
        let id = FlowStepId::new(1)?;
        let flow = IndexedFunctionFlow::new(
            &owner,
            FunctionFlow::new(
                owner.parsed().id(),
                range,
                vec![FlowStep {
                    id,
                    kind: FlowStepKind::Call,
                    parent: None,
                    range,
                    name: None,
                    callee_range: Some(range),
                    process: None,
                    inputs: Vec::new(),
                    outputs: Vec::new(),
                    arguments: Vec::new(),
                }],
                Vec::new(),
                Vec::new(),
            )?,
            vec![FlowCallLink {
                step: id,
                target: None,
            }],
        )?;
        let encoded = encode(&flow)?;
        assert_eq!(decode(&owner, &encoded)?, flow);
        for (pointer, replacement) in [
            ("/steps/0/id", serde_json::json!(0)),
            ("/steps/0/kind", serde_json::json!(255)),
            ("/steps/0/parent", serde_json::json!(1)),
            ("/steps/0/inputs", serde_json::json!([4])),
            ("/steps/0/range", serde_json::json!([0, 11, 0, 0, 0, 11])),
            ("/calls", serde_json::json!([])),
        ] {
            let mut body: serde_json::Value = serde_json::from_str(&encoded)?;
            *body.pointer_mut(pointer).ok_or("fixture pointer")? = replacement;
            assert!(
                decode(&owner, &serde_json::to_string(&body)?).is_err(),
                "{pointer}"
            );
        }
        let mut body: serde_json::Value = serde_json::from_str(&encoded)?;
        body["untrusted_extra"] = serde_json::json!(true);
        assert!(decode(&owner, &serde_json::to_string(&body)?).is_err());
        assert!(decode(&owner, &" ".repeat(MAX_BODY_BYTES + 1)).is_err());
        Ok(())
    }
}
