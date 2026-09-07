//! V7 disjoint responses; normalized into the existing strict work boundary (ADR-0075).
use crate::{AskResearchDecisionDecodeError as DecodeError, ResearchOutputPhase};
use serde_json::{Value, json};

pub(crate) fn schema(phase: ResearchOutputPhase, reads: bool) -> Result<Value, DecodeError> {
    let mut schema = crate::research_work_v6_phase_schema(phase, reads)?;
    let branches = match phase {
        ResearchOutputPhase::Initialize => {
            schema["$defs"]["questionsResponse"] = json!({"type":"object","additionalProperties":false,
                "required":["kind","questions"],"properties":{
                    "kind":{"const":"questions"},
                    "questions":schema["$defs"]["work"]["properties"]["questions"]}});
            vec![json!({"$ref":"#/$defs/questionsResponse"})]
        }
        ResearchOutputPhase::Analyze(_) | ResearchOutputPhase::SummarizeOriginals(_) => {
            schema["$defs"]["result"]["properties"]["evidence"]["minItems"] = json!(1);
            let mut branches = vec![
                json!({"$ref":"#/$defs/result"}),
                json!({"$ref":"#/$defs/evidenceNeed"}),
            ];
            if matches!(phase, ResearchOutputPhase::Analyze(_)) {
                branches.push(json!({"$ref":"#/$defs/questionDecision"}));
            }
            branches
        }
        ResearchOutputPhase::Design(_) => vec![
            json!({"$ref":"#/$defs/result"}),
            json!({"$ref":"#/$defs/questionDecision"}),
        ],
        ResearchOutputPhase::DesignTests(_) => vec![json!({"$ref":"#/$defs/result"})],
        ResearchOutputPhase::Finalize => vec![json!({"$ref":"#/$defs/planDecision"})],
    };
    // Every union arm selects its kind before generating variant-specific data.
    // A flat result starts with evidence and biases constrained generation away from it.
    if let Some(mut payload) = schema["$defs"].get("result").cloned() {
        let kind = payload["properties"]["kind"].clone();
        payload["properties"]
            .as_object_mut()
            .ok_or(DecodeError::InvalidSchema)?
            .remove("kind");
        payload["required"]
            .as_array_mut()
            .ok_or(DecodeError::InvalidSchema)?
            .retain(|field| field != "kind");
        schema["$defs"]["resultPayload"] = payload;
        schema["$defs"]["result"] = json!({"type":"object","additionalProperties":false,
            "required":["kind","result"],"properties":{"kind":kind,"result":{"$ref":"#/$defs/resultPayload"}}});
    }
    let response = if branches.len() == 1 {
        branches
            .into_iter()
            .next()
            .ok_or(DecodeError::InvalidSchema)?
    } else {
        json!({"oneOf":branches})
    };
    schema["$id"] = json!("https://a3.local/schemas/ask-research-response-v7.schema.json");
    schema["title"] = json!("A^3 Research Response V7");
    schema["required"] = json!(["schema_version", "response"]);
    schema["properties"] = json!({"schema_version":{"const":7},"response":response});
    crate::schema_projection::prune_definitions(&mut schema).ok_or(DecodeError::InvalidSchema)?;
    Ok(schema)
}

/// Rejects unknown/mixed arms before normalization; no permissive salvage or new authority.
/// The caller still runs the unchanged independent phase/value and original-evidence checks.
pub(crate) fn normalize(value: Value) -> Result<Value, DecodeError> {
    use crate::ask_research_action_codec::{array, exact, object, string};
    if value["schema_version"].as_u64() != Some(7) {
        return Ok(value);
    }
    let root = object(&value)?;
    exact(root, &["schema_version", "response"])?;
    let response = object(&root["response"])?;
    let mut work = json!({"questions":[],"results":[]});
    let decision = match string(response, "kind")? {
        "questions" => {
            exact(response, &["kind", "questions"])?;
            let questions = array(response, "questions")?;
            if questions.is_empty()
                || questions
                    .iter()
                    .any(|q| q.get("request_fragment").is_some())
            {
                return Err(DecodeError::InvalidValue);
            }
            work["questions"] = Value::Array(questions.to_vec());
            json!({"kind":"progress"})
        }
        "interpretation" | "designDecision" => {
            exact(response, &["kind", "result"])?;
            let payload = object(&response["result"])?;
            exact(payload, &["question_id", "text", "evidence"])?;
            let evidence = array(payload, "evidence")?;
            if (response["kind"] == "interpretation"
                && (evidence.is_empty() || evidence.iter().any(|e| e.get("anchor_ref").is_none())))
                || (response["kind"] == "designDecision" && !evidence.is_empty())
            {
                return Err(DecodeError::InvalidValue);
            }
            let mut result = Value::Object(payload.clone());
            result["kind"] = response["kind"].clone();
            work["results"] = json!([result]);
            json!({"kind":"progress"})
        }
        // Exact fields and bounded values of these arms are checked by their existing decoders.
        "evidenceNeed" | "question" | "plan" => Value::Object(response.clone()),
        _ => return Err(DecodeError::InvalidValue),
    };
    Ok(json!({"schema_version":6,"work":work,"decision":decision}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use a3_domain::ResearchQuestionId;
    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn phases() -> [ResearchOutputPhase; 6] {
        let id = ResearchQuestionId::FIRST;
        [
            ResearchOutputPhase::Initialize,
            ResearchOutputPhase::Analyze(id),
            ResearchOutputPhase::SummarizeOriginals(id),
            ResearchOutputPhase::Design(id),
            ResearchOutputPhase::DesignTests(id),
            ResearchOutputPhase::Finalize,
        ]
    }

    #[test]
    fn research_v7_each_response_has_exactly_its_allowed_phases() -> TestResult {
        let responses = [
            (
                json!({"kind":"questions","questions":[{"kind":"repository","outcome":"Explain helper return","priority":"required","dependencies":[]}]}),
                vec![0],
            ),
            (
                json!({"kind":"interpretation","result":{"question_id":1,"text":"helper returns 7","evidence":[{"anchor_ref":"E1"}]}}),
                vec![1, 2],
            ),
            (
                json!({"kind":"evidenceNeed","question_id":1,"targets":["helper"]}),
                vec![1, 2],
            ),
            (
                json!({"kind":"question","message":"Which externally visible compatibility contract is required?"}),
                vec![1, 3],
            ),
            (
                json!({"kind":"designDecision","result":{"question_id":1,"text":"Add a helper return regression and run the existing test profile.","evidence":[]}}),
                vec![3, 4],
            ),
            (
                json!({"kind":"plan","summary":"Document helper behavior","changes":["Document return value"],"interfaces":"No public signature changes","tests":["Verify the returned value"],"assumptions":"Keep existing compatibility"}),
                vec![5],
            ),
        ];
        for (response, allowed) in responses {
            let valid = json!({"schema_version":7,"response":response});
            for (index, phase) in phases().into_iter().enumerate() {
                assert_eq!(
                    crate::DecodeAskResearchDecision
                        .decode_phase(&valid.to_string(), phase)
                        .is_ok(),
                    allowed.contains(&index),
                    "response {} in {phase:?}",
                    valid["response"]["kind"]
                );
                for path in ["root", "response"] {
                    let mut unknown = valid.clone();
                    let target = if path == "root" {
                        &mut unknown
                    } else {
                        &mut unknown["response"]
                    };
                    target["note"] = json!({"finding":"untrusted status"});
                    assert!(
                        crate::DecodeAskResearchDecision
                            .decode_phase(&unknown.to_string(), phase)
                            .is_err()
                    );
                }
            }
        }
        Ok(())
    }

    #[test]
    fn research_v7_schema_has_no_nullable_work_or_progress_and_is_smaller() -> TestResult {
        for phase in phases() {
            let previous = crate::research_work_v6_phase_schema(phase, true)?;
            let current = schema(phase, true)?;
            assert_eq!(current["type"], "object");
            assert_eq!(current["additionalProperties"], false);
            assert_eq!(current["required"], json!(["schema_version", "response"]));
            assert_eq!(current["properties"]["schema_version"]["const"], 7);
            assert_eq!(
                current["properties"].as_object().ok_or("properties")?.len(),
                2
            );
            for absent in ["work", "progress", "v5StatusNote"] {
                assert!(current["$defs"].get(absent).is_none());
            }
            if let Some(arms) = current["properties"]["response"]["oneOf"].as_array() {
                for arm in arms {
                    let reference = arm["$ref"].as_str().ok_or("union reference")?;
                    let definition = current
                        .pointer(reference.strip_prefix('#').ok_or("local reference")?)
                        .ok_or("union definition")?;
                    assert_eq!(definition["required"][0], "kind");
                    assert_eq!(
                        definition["properties"]
                            .as_object()
                            .ok_or("union properties")?
                            .keys()
                            .next()
                            .map(String::as_str),
                        Some("kind")
                    );
                }
            }
            assert_eq!(current, schema(phase, false)?);
            assert!(current.to_string().len() < previous.to_string().len());
            println!(
                "research-schema {phase:?}: V6={} V7={} UTF-8 bytes",
                previous.to_string().len(),
                current.to_string().len()
            );
        }
        Ok(())
    }

    #[test]
    fn research_v7_rejects_wrong_question_evidence_coordinates_and_byte_overflow() {
        let valid = json!({"schema_version":7,"response":{"kind":"interpretation","result":{"question_id":1,"text":"helper returns 7","evidence":[{"anchor_ref":"E1"}]}}});
        let phase = ResearchOutputPhase::Analyze(ResearchQuestionId::FIRST);
        for (field, value) in [
            ("kind", json!("designDecision")),
            ("evidence", json!([])),
            ("evidence", json!(null)),
            ("question_id", json!(2)),
            ("text", json!("ü".repeat(2049))),
            ("evidence", json!([{"anchor_ref":"E0"}])),
            ("evidence", json!([{"anchor_ref":"E1","path":"private.py"}])),
            (
                "evidence",
                json!([{"source_ref":"S1","quote":"helper returns 7"}]),
            ),
            ("evidence", json!(vec![json!({"anchor_ref":"E1"}); 33])),
        ] {
            let mut invalid = valid.clone();
            invalid["response"]["result"][field] = value;
            assert!(
                crate::DecodeAskResearchDecision
                    .decode_phase(&invalid.to_string(), phase)
                    .is_err()
            );
        }
        let empty = json!({"schema_version":7,"response":{"kind":"questions","questions":[]}});
        assert!(
            crate::DecodeAskResearchDecision
                .decode(&empty.to_string())
                .is_err()
        );
        let invented = json!({"schema_version":7,"response":{"kind":"evidenceNeed","question_id":1,"targets":["../escape"]}});
        assert!(
            crate::DecodeAskResearchDecision
                .decode_phase(&invented.to_string(), phase)
                .is_err()
        );
    }
}
