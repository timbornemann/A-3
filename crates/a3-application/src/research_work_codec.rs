//! V5 research updates: bounded proposed answers, never authoritative completion flags.
use crate::AskResearchDecisionDecodeError as DecodeError;
use a3_domain::{
    ResearchQuestionDraft, ResearchQuestionId, ResearchQuestionKind, ResearchQuestionPriority,
    ResearchResultKind,
};
use serde_json::{Value, json};

/// Initial decomposition and results proposed by one valid V5 document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResearchWorkUpdate {
    /// Nonempty only before the Core freezes the contract.
    pub questions: Vec<ResearchQuestionDraft>,
    /// Independently attributed answers; no implicit closure of omitted questions.
    pub results: Vec<ResearchResultProposal>,
}

/// Core-selected document phase, never inferred from text supplied by the model or repository.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResearchOutputPhase {
    /// Define the immutable question contract once.
    Initialize,
    /// Answer existing questions without redefining them.
    Analyze(ResearchQuestionId),
    /// Describe fully delivered originals in the fixed Core plan inventory, not open research.
    SummarizeOriginals(ResearchQuestionId),
    /// Propose future behavior using admitted prerequisites, without claiming new originals.
    Design(ResearchQuestionId),
    /// Format already admitted results without reopening research.
    Finalize,
}

/// Narrows V5 to the current trusted phase and drops unreachable schema definitions.
pub fn research_work_phase_schema(
    phase: ResearchOutputPhase,
    _reads: bool,
) -> Result<Value, DecodeError> {
    let mut schema = research_work_decision_schema()?;
    match phase {
        ResearchOutputPhase::Initialize => {
            schema["$defs"]["work"]["properties"]["questions"]["minItems"] = json!(1);
            schema["$defs"]["question"]["required"]
                .as_array_mut()
                .ok_or(DecodeError::InvalidSchema)?
                .retain(|field| field != "request_fragment");
            schema["$defs"]["question"]["properties"]
                .as_object_mut()
                .ok_or(DecodeError::InvalidSchema)?
                .remove("request_fragment");
        }
        ResearchOutputPhase::Analyze(_)
        | ResearchOutputPhase::SummarizeOriginals(_)
        | ResearchOutputPhase::Design(_)
        | ResearchOutputPhase::Finalize => {
            schema["$defs"]["work"]["properties"]["questions"] =
                json!({"type":"array","maxItems":0,"items":{"type":"null"}});
        }
    }
    if let ResearchOutputPhase::Analyze(question)
    | ResearchOutputPhase::SummarizeOriginals(question)
    | ResearchOutputPhase::Design(question) = phase
    {
        schema["$defs"]["work"]["properties"]["results"]["maxItems"] = json!(1);
        if matches!(phase, ResearchOutputPhase::SummarizeOriginals(_)) {
            schema["$defs"]["work"]["properties"]["results"]["minItems"] = json!(1);
            schema["$defs"]["result"]["properties"]["evidence"]["minItems"] = json!(1);
        }
        schema["$defs"]["result"]["properties"]["question_id"] = json!({"const":question.get()});
        schema["$defs"]["result"]["properties"]["evidence"]["items"] =
            json!({"$ref":"#/$defs/anchor"});
        if matches!(phase, ResearchOutputPhase::Design(_)) {
            schema["$defs"]["work"]["properties"]["results"]["description"] = json!(
                "For decision.kind=progress return exactly one concrete designDecision. Empty results are allowed only with decision.kind=question for a consequential missing user choice; never request repository reads for future design."
            );
            schema["$defs"]["result"]["properties"]["kind"] = json!({"const":"designDecision"});
            schema["$defs"]["result"]["properties"]["evidence"] =
                json!({"type":"array","maxItems":0,"items":{"type":"null"}});
        } else {
            // Repository analysis cannot emit a proposed design or self-authorize a
            // bounded unknown. The Core derives negative boundaries from actual receipts.
            schema["$defs"]["result"]["properties"]["kind"] = json!({"const":"interpretation"});
        }
    } else {
        schema["$defs"]["work"]["properties"]["results"] =
            json!({"type":"array","maxItems":0,"items":{"type":"null"}});
    }
    schema["properties"]["decision"] = json!({"$ref":"#/$defs/planDecision"});
    if phase != ResearchOutputPhase::Finalize {
        schema["properties"]["decision"] = if matches!(
            phase,
            ResearchOutputPhase::Analyze(_) | ResearchOutputPhase::Design(_)
        ) {
            json!({"oneOf":[{"$ref":"#/$defs/progress"},{"$ref":"#/$defs/questionDecision"}]})
        } else {
            json!({"$ref":"#/$defs/progress"})
        };
    }
    crate::schema_projection::prune_definitions(&mut schema).ok_or(DecodeError::InvalidSchema)?;
    Ok(schema)
}

/// Public interpretation/design proposed for one issued question.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResearchResultProposal {
    /// One-based Core-assigned question position.
    pub question_id: ResearchQuestionId,
    /// Explicit epistemic kind (never Fact).
    pub kind: ResearchResultKind,
    /// Bounded user-facing result.
    pub text: String,
    /// Exact original snippets used to admit evidence; snippets are not persisted.
    pub evidence: Vec<ResearchQuoteProposal>,
    /// References to actual delivered windows; the Core supplies all source coordinates.
    pub anchors: Vec<ResearchEvidenceAnchorId>,
}

/// Packet-local position of one of the at most eight emitted original source windows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResearchEvidenceAnchorId(u16);
impl ResearchEvidenceAnchorId {
    /// Validates a position; existence and freshness are checked against actual delivery.
    pub fn new(value: u16) -> Result<Self, DecodeError> {
        if !(1..=8).contains(&value) {
            return Err(DecodeError::InvalidValue);
        }
        Ok(Self(value))
    }
    /// Returns the window position, never a durable source identity.
    #[must_use]
    pub const fn get(self) -> u16 {
        self.0
    }
}

/// Model-selected original quote checked against actual delivery at the source boundary.
#[derive(Clone, PartialEq, Eq)]
pub struct ResearchQuoteProposal {
    /// Previously issued source ordinal.
    pub source_ordinal: u16,
    /// Exact original UTF-8 text, never treated as instructions.
    pub quote: String,
}
impl std::fmt::Debug for ResearchQuoteProposal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ResearchQuoteProposal")
            .field("source_ordinal", &self.source_ordinal)
            .finish_non_exhaustive()
    }
}

/// Builds the V5 schema from the unchanged historical action grammar.
pub fn research_work_decision_schema() -> Result<Value, DecodeError> {
    let mut schema = crate::DecodeAskResearchDecision.json_schema().as_json()?;
    schema["$id"] = json!("https://a3.local/schemas/ask-research-decision-v5.schema.json");
    schema["title"] = json!("A^3 Research Work Decision V5");
    schema["required"] = json!(["schema_version", "decision", "work"]);
    schema["properties"]["schema_version"] = json!({"const":5});
    schema["properties"]["work"] = json!({"$ref":"#/$defs/work"});
    let definitions = schema["$defs"]
        .as_object_mut()
        .ok_or(DecodeError::InvalidSchema)?;
    // V5 status is not a completion or evidence decision. Keep the historical note
    // grammar separate so legacy answer/research payloads retain their strict bounds.
    let mut status_note = definitions
        .get("note")
        .cloned()
        .ok_or(DecodeError::InvalidSchema)?;
    for field in ["gap", "next_step"] {
        status_note["properties"][field]["minLength"] = json!(0);
    }
    status_note["properties"]["finding_source_refs"]["uniqueItems"] = json!(false);
    definitions.insert("v5StatusNote".to_owned(), status_note);
    definitions.insert(
        "progress".to_owned(), json!({"type":"object","additionalProperties":false,
            "required":["kind","note"], "properties":{"kind":{"const":"progress"},"note":{"$ref":"#/$defs/v5StatusNote"}}})
    );
    definitions.insert(
        "questionDecision".to_owned(), json!({"type":"object","additionalProperties":false,
            "required":["kind","note","message"], "properties":{"kind":{"const":"question"},"note":{"$ref":"#/$defs/v5StatusNote"},
            "message":{"type":"string","minLength":1,"maxLength":1024}}})
    );
    definitions.insert(
        "work".to_owned(),
        json!({
            "type":"object", "additionalProperties":false, "required":["questions","results"],
            "properties":{
                "questions":{"type":"array","maxItems":32,"items":{"$ref":"#/$defs/question"}},
                "results":{"type":"array","maxItems":32,"items":{"$ref":"#/$defs/result"}}
            }
        }),
    );
    definitions.insert("question".to_owned(), json!({
        "type":"object","additionalProperties":false,
        "required":["request_fragment","outcome","priority","kind","dependencies"],
        "properties":{
            "request_fragment":{"type":"string","minLength":1,"maxLength":2048},
            "outcome":{"type":"string","minLength":1,"maxLength":512},
            "priority":{"enum":["required","supporting","optional"]},
            "kind":{"enum":["repository","design"]},
            "dependencies":{"type":"array","maxItems":31,"items":{"type":"integer","minimum":1,"maximum":32}}
        }
    }));
    definitions.insert("result".to_owned(), json!({
        "type":"object","additionalProperties":false,"required":["question_id","kind","text","evidence"],
        "properties":{
            "question_id":{"type":"integer","minimum":1,"maximum":32},
            "kind":{"enum":["interpretation","designDecision","boundedUnknown"]},
            "text":{"type":"string","minLength":1,"maxLength":4096},
            "evidence":{"type":"array","maxItems":32,"items":{"oneOf":[{"$ref":"#/$defs/quote"},{"$ref":"#/$defs/anchor"}]}}
        }
    }));
    definitions.insert(
        "anchor".to_owned(),
        json!({"type":"object","additionalProperties":false,"required":["anchor_ref"],
        "properties":{"anchor_ref":{"enum":["E1","E2","E3","E4","E5","E6","E7","E8"]}}}),
    );
    definitions.insert(
        "planDecision".to_owned(),
        json!({"type":"object","additionalProperties":false,
        "required":["kind","note","summary","changes","interfaces","tests","assumptions"],
        "properties":{"kind":{"const":"plan"},"note":{"$ref":"#/$defs/v5StatusNote"},
        "summary":{"type":"string","minLength":1,"maxLength":4096},
        "changes":{"type":"array","minItems":1,"maxItems":32,"items":{"$ref":"#/$defs/planStep"}},
        "interfaces":{"type":"string","minLength":1,"maxLength":4096},
        "tests":{"type":"array","minItems":1,"maxItems":32,"items":{"$ref":"#/$defs/planStep"}},
        "assumptions":{"type":"string","minLength":1,"maxLength":4096}}}),
    );
    definitions.insert(
        "planStep".to_owned(),
        json!({"type":"string","minLength":1,"maxLength":2048,"pattern":"^[^\\r\\n]+$"}),
    );
    definitions.insert(
        "quote".to_owned(),
        json!({
            "type":"object","additionalProperties":false,"required":["source_ref","quote"],
            "properties":{
                "source_ref":{"$ref":"#/$defs/sourceRef"},
                "quote":{"type":"string","minLength":1,"maxLength":512}
            }
        }),
    );
    schema["properties"]["decision"]["oneOf"]
        .as_array_mut()
        .ok_or(DecodeError::InvalidSchema)?
        .extend([
            json!({"$ref":"#/$defs/progress"}),
            json!({"$ref":"#/$defs/questionDecision"}),
            json!({"$ref":"#/$defs/planDecision"}),
        ]);
    Ok(schema)
}

pub(crate) fn validate_phase(value: &Value, phase: ResearchOutputPhase) -> Result<(), DecodeError> {
    use crate::ask_research_action_codec::{array, object, string};
    if value["schema_version"] != 5 {
        return Err(DecodeError::UnsupportedVersion);
    }
    let decision = object(&value["decision"])?;
    let work = object(&value["work"])?;
    let questions = array(work, "questions")?;
    let results = array(work, "results")?;
    let kind = string(decision, "kind")?;
    let valid = match phase {
        ResearchOutputPhase::Initialize => {
            kind == "progress"
                && !questions.is_empty()
                && results.is_empty()
                && questions
                    .iter()
                    .all(|q| q.get("request_fragment").is_none())
        }
        ResearchOutputPhase::Analyze(id)
        | ResearchOutputPhase::SummarizeOriginals(id)
        | ResearchOutputPhase::Design(id) => {
            matches!(kind, "progress" | "question")
                && questions.is_empty()
                && results.len() <= 1
                && (!matches!(phase, ResearchOutputPhase::Design(_))
                    || kind == "question"
                    || results.len() == 1)
                && (!matches!(phase, ResearchOutputPhase::SummarizeOriginals(_))
                    || (kind == "progress" && results.len() == 1))
                && results.iter().all(|r| {
                    r["question_id"] == id.get()
                        && (!matches!(
                            phase,
                            ResearchOutputPhase::Analyze(_)
                                | ResearchOutputPhase::SummarizeOriginals(_)
                        ) || r["kind"] == "interpretation")
                        && (!matches!(phase, ResearchOutputPhase::SummarizeOriginals(_))
                            || r["evidence"]
                                .as_array()
                                .is_some_and(|items| !items.is_empty()))
                        && (!matches!(phase, ResearchOutputPhase::Design(_))
                            || (r["kind"] == "designDecision"
                                && r["evidence"].as_array().is_some_and(Vec::is_empty)))
                        && r["evidence"].as_array().is_some_and(|items| {
                            items.iter().all(|item| item.get("anchor_ref").is_some())
                        })
                })
        }
        ResearchOutputPhase::Finalize => {
            kind == "plan" && questions.is_empty() && results.is_empty()
        }
    };
    if valid {
        Ok(())
    } else {
        Err(DecodeError::InvalidValue)
    }
}

pub(crate) fn decode_work(value: &Value) -> Result<ResearchWorkUpdate, DecodeError> {
    use crate::ask_research_action_codec::{array, bounded, exact, object, source_ordinal, string};
    let work = object(value)?;
    exact(work, &["questions", "results"])?;
    let definitions = array(work, "questions")?;
    let results = array(work, "results")?;
    if definitions.len() > 32 || results.len() > 32 {
        return Err(DecodeError::InvalidValue);
    }
    let mut questions = Vec::with_capacity(definitions.len());
    for value in definitions {
        let item = object(value)?;
        let fields = if item.contains_key("request_fragment") {
            &[
                "request_fragment",
                "outcome",
                "priority",
                "kind",
                "dependencies",
            ][..]
        } else {
            &["outcome", "priority", "kind", "dependencies"][..]
        };
        exact(item, fields)?;
        let deps = array(item, "dependencies")?;
        if deps.len() > 31 {
            return Err(DecodeError::InvalidValue);
        }
        questions.push(ResearchQuestionDraft {
            request_fragment: if item.contains_key("request_fragment") {
                bounded(string(item, "request_fragment")?, 2048)?
            } else {
                String::new()
            },
            outcome: bounded(string(item, "outcome")?, 512)?,
            priority: match string(item, "priority")? {
                "required" => ResearchQuestionPriority::Required,
                "supporting" => ResearchQuestionPriority::Supporting,
                "optional" => ResearchQuestionPriority::Optional,
                _ => return Err(DecodeError::InvalidValue),
            },
            kind: match string(item, "kind")? {
                "repository" => ResearchQuestionKind::Repository,
                "design" => ResearchQuestionKind::Design,
                _ => return Err(DecodeError::InvalidValue),
            },
            dependencies: deps.iter().map(question_id).collect::<Result<_, _>>()?,
        });
    }
    let mut proposals = Vec::with_capacity(results.len());
    for value in results {
        let item = object(value)?;
        exact(item, &["question_id", "kind", "text", "evidence"])?;
        let id = question_id(item.get("question_id").ok_or(DecodeError::InvalidShape)?)?;
        if proposals
            .iter()
            .any(|p: &ResearchResultProposal| p.question_id == id)
        {
            return Err(DecodeError::InvalidValue);
        }
        let values = array(item, "evidence")?;
        if values.len() > 32 {
            return Err(DecodeError::InvalidValue);
        }
        let mut evidence = Vec::with_capacity(values.len());
        let mut anchors = Vec::new();
        for value in values {
            let quote = object(value)?;
            if quote.contains_key("anchor_ref") {
                exact(quote, &["anchor_ref"])?;
                let number = string(quote, "anchor_ref")?
                    .strip_prefix('E')
                    .ok_or(DecodeError::InvalidValue)?;
                let anchor = ResearchEvidenceAnchorId::new(
                    number.parse().map_err(|_| DecodeError::InvalidValue)?,
                )?;
                if number != anchor.get().to_string() {
                    return Err(DecodeError::InvalidValue);
                }
                // References are a set. Validate every encoded element and enforce the
                // input bound above, but repeating the same original adds no new claim.
                if !anchors.contains(&anchor) {
                    anchors.push(anchor);
                }
                continue;
            }
            exact(quote, &["source_ref", "quote"])?;
            evidence.push(ResearchQuoteProposal {
                source_ordinal: source_ordinal(string(quote, "source_ref")?)?,
                quote: bounded(string(quote, "quote")?, 512)?,
            });
        }
        proposals.push(ResearchResultProposal {
            question_id: id,
            kind: match string(item, "kind")? {
                "interpretation" => ResearchResultKind::Interpretation,
                "designDecision" => ResearchResultKind::DesignDecision,
                "boundedUnknown" => ResearchResultKind::BoundedUnknown,
                _ => return Err(DecodeError::InvalidValue),
            },
            text: bounded(string(item, "text")?, 4096)?,
            evidence,
            anchors,
        });
    }
    Ok(ResearchWorkUpdate {
        questions,
        results: proposals,
    })
}

fn question_id(value: &Value) -> Result<ResearchQuestionId, DecodeError> {
    value
        .as_u64()
        .and_then(|n| u16::try_from(n).ok())
        .and_then(|n| ResearchQuestionId::new(n).ok())
        .ok_or(DecodeError::InvalidValue)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn repeated_canonical_original_anchors_form_one_bounded_source_set() -> Result<(), DecodeError>
    {
        let mut work = json!({"questions":[],"results":[{"question_id":1,"kind":"interpretation","text":"Two methods use the same original window.","evidence":[{"anchor_ref":"E2"},{"anchor_ref":"E1"},{"anchor_ref":"E2"}]}]});
        let decoded = decode_work(&work)?;
        assert_eq!(
            decoded.results[0]
                .anchors
                .iter()
                .map(|a| a.get())
                .collect::<Vec<_>>(),
            vec![2, 1]
        );
        for invalid in [
            json!({"anchor_ref":"E0"}),
            json!({"anchor_ref":"E9"}),
            json!({"anchor_ref":"E01"}),
            json!({"anchor_ref":"E1","quote":"extra"}),
        ] {
            work["results"][0]["evidence"] = json!([{"anchor_ref":"E1"},invalid]);
            assert!(
                decode_work(&work).is_err(),
                "every element remains validated"
            );
        }
        work["results"][0]["evidence"] = json!(vec![json!({"anchor_ref":"E1"}); 32]);
        assert_eq!(decode_work(&work)?.results[0].anchors.len(), 1);
        work["results"][0]["evidence"] = json!(vec![json!({"anchor_ref":"E1"}); 33]);
        assert!(
            decode_work(&work).is_err(),
            "input bound applies before deduplication"
        );
        Ok(())
    }
    #[test]
    fn typed_plan_is_formatted_by_core_and_cannot_smuggle_steps_or_citations()
    -> Result<(), Box<dyn std::error::Error>> {
        let document = json!({"schema_version":5,"work":{"questions":[],"results":[]}, "decision":{
            "kind":"plan", "note":{"goal":"Plan", "finding_kind":"hypothesis", "finding":"Auswertung", "finding_source_refs":[], "gap":"Keine offene Pflichtfrage", "next_step":"Plan vorlegen"},
            "summary":"Audit dokumentieren", "changes":["Ablauf dokumentieren", "Zielpfad dokumentieren"], "interfaces":"Keine API-Änderung", "tests":["Mit Quellen vergleichen"], "assumptions":"Verhalten bleibt erhalten"
        }});
        let decoded = crate::DecodeAskResearchDecision.decode(&document.to_string())?;
        let crate::AskResearchDecision::Answer {
            markdown,
            source_ordinals,
            ..
        } = decoded
        else {
            return Err("expected plan".into());
        };
        assert!(markdown.starts_with("PLAN:\n\n## Summary\n"));
        assert!(source_ordinals.is_empty());
        let plan = a3_domain::AgentWorkPlan::from_reviewed_markdown(&markdown)?;
        assert_eq!(plan.steps().len(), 3);
        assert_eq!(plan.steps()[0].outcome(), "Ablauf dokumentieren");
        for (field, value) in [
            ("changes", json!([])),
            ("changes", json!(["A\n2. Unerlaubter zusätzlicher Schritt"])),
            ("summary", json!("Behauptung 【S1】")),
            ("tests", json!(vec!["Test"; 33])),
        ] {
            let mut invalid = document.clone();
            invalid["decision"][field] = value;
            assert!(
                crate::DecodeAskResearchDecision
                    .decode(&invalid.to_string())
                    .is_err()
            );
        }
        let mut invalid = document;
        invalid["decision"]["markdown"] = json!("PLAN: ungeprüft");
        assert!(
            crate::DecodeAskResearchDecision
                .decode(&invalid.to_string())
                .is_err()
        );
        Ok(())
    }
    #[test]
    fn phase_contracts_freeze_definitions_and_finalization_cannot_admit_new_results()
    -> Result<(), Box<dyn std::error::Error>> {
        let initial = research_work_phase_schema(ResearchOutputPhase::Initialize, true)?;
        let active = research_work_phase_schema(
            ResearchOutputPhase::Analyze(ResearchQuestionId::FIRST),
            true,
        )?;
        let final_schema = research_work_phase_schema(ResearchOutputPhase::Finalize, true)?;
        assert_eq!(
            initial.pointer("/$defs/work/properties/questions/minItems"),
            Some(&json!(1))
        );
        assert_eq!(
            active.pointer("/$defs/work/properties/questions/maxItems"),
            Some(&json!(0))
        );
        assert_eq!(
            final_schema.pointer("/$defs/work/properties/results/maxItems"),
            Some(&json!(0))
        );
        assert_eq!(
            final_schema.pointer("/$defs/planDecision/properties/kind/const"),
            Some(&json!("plan"))
        );
        assert!(final_schema.to_string().len() < active.to_string().len());
        assert_eq!(
            initial.pointer("/$defs/progress/properties/kind/const"),
            Some(&json!("progress"))
        );
        assert!(
            initial
                .pointer("/$defs/progress/properties/markdown")
                .is_none()
        );
        Ok(())
    }

    #[test]
    fn complete_original_inventory_is_narrower_than_open_analysis() -> Result<(), DecodeError> {
        let phase = ResearchOutputPhase::SummarizeOriginals(ResearchQuestionId::FIRST);
        let schema = research_work_phase_schema(phase, true)?;
        assert_eq!(
            schema["$defs"]["work"]["properties"]["results"]["minItems"],
            1
        );
        assert_eq!(
            schema["$defs"]["work"]["properties"]["results"]["maxItems"],
            1
        );
        assert_eq!(
            schema["$defs"]["result"]["properties"]["evidence"]["minItems"],
            1
        );
        assert!(schema["$defs"].get("questionDecision").is_none());
        let mut document = json!({"schema_version":5,"decision":{"kind":"progress","note":{
            "goal":"Inventory", "finding_kind":"hypothesis", "finding":"Originals", "finding_source_refs":[], "gap":"None", "next_step":"Design"
        }},"work":{"questions":[],"results":[]}});
        let decode = |value: &Value| {
            crate::DecodeAskResearchDecision.decode_phase(&value.to_string(), phase)
        };
        assert!(decode(&document).is_err());
        assert!(
            crate::DecodeAskResearchDecision
                .decode_phase(
                    &document.to_string(),
                    ResearchOutputPhase::Analyze(ResearchQuestionId::FIRST)
                )
                .is_ok()
        );
        document["work"]["results"] = json!([{"question_id":1,"kind":"interpretation","text":"Visible entry point calls the external API; its implementation is outside these originals.","evidence":[{"anchor_ref":"E1"}]}]);
        assert!(decode(&document).is_ok());
        document["decision"]["kind"] = json!("question");
        document["decision"]["message"] = json!("Read more?");
        assert!(decode(&document).is_err());
        document["decision"]["kind"] = json!("progress");
        document["decision"]
            .as_object_mut()
            .ok_or(DecodeError::InvalidShape)?
            .remove("message");
        for (field, invalid) in [
            ("kind", json!("designDecision")),
            ("question_id", json!(2)),
            ("evidence", json!([])),
            ("evidence", json!([{"source_ref":"S1","quote":"entry"}])),
        ] {
            let mut variant = document.clone();
            variant["work"]["results"][0][field] = invalid;
            assert!(decode(&variant).is_err());
        }
        Ok(())
    }

    #[test]
    fn research_v5_empty_navigation_status_is_neutral_not_a_completion_or_repair()
    -> Result<(), Box<dyn std::error::Error>> {
        let note = json!({"goal":"Inspect originals","finding_kind":"hypothesis","finding":"Contract prepared","finding_source_refs":[],"gap":"","next_step":" \t"});
        let raw = json!({"schema_version":5,"decision":{"kind":"progress","note":note},"work":{"questions":[],"results":[]}});
        let crate::AskResearchDecision::Answer {
            note: admitted,
            evidence_status,
            ..
        } = crate::DecodeAskResearchDecision.decode_phase(
            &raw.to_string(),
            ResearchOutputPhase::Analyze(ResearchQuestionId::FIRST),
        )?
        else {
            return Err("progress".into());
        };
        assert_eq!(
            admitted.gap,
            "Keine zusätzliche Beleglücke gemeldet; der Prüfstand bleibt maßgeblich."
        );
        assert_eq!(
            admitted.next_step,
            "Nächsten Schritt aus dem Prüfstand bestimmen."
        );
        assert_eq!(
            evidence_status,
            crate::AskResearchEvidenceStatus::Incomplete
        );
        assert!(admitted.work.as_ref().ok_or("work")?.results.is_empty());
        assert!(admitted.source_ordinals.is_empty());
        let schema = research_work_phase_schema(ResearchOutputPhase::Initialize, true)?;
        assert_eq!(
            schema["$defs"]["v5StatusNote"]["properties"]["gap"]["minLength"],
            0
        );
        assert_eq!(
            schema["$defs"]["v5StatusNote"]["properties"]["next_step"]["minLength"],
            0
        );
        for field in ["gap", "next_step"] {
            for invalid in [
                json!(null),
                json!(false),
                json!("x".repeat(1025)),
                json!("\u{0000}"),
                json!("\u{0007}"),
            ] {
                let mut bad = raw.clone();
                bad["decision"]["note"][field] = invalid;
                assert!(
                    crate::DecodeAskResearchDecision
                        .decode(&bad.to_string())
                        .is_err()
                );
            }
        }
        for field in ["goal", "finding"] {
            let mut bad = raw.clone();
            bad["decision"]["note"][field] = json!("");
            assert!(
                crate::DecodeAskResearchDecision
                    .decode(&bad.to_string())
                    .is_err()
            );
        }
        let legacy = json!({"schema_version":4,"decision":{"kind":"answer","note":note,"evidence_status":"incomplete","markdown":"Pending","source_refs":[]}});
        assert!(
            crate::DecodeAskResearchDecision
                .decode(&legacy.to_string())
                .is_err()
        );
        let legacy_schema = crate::DecodeAskResearchDecision.json_schema().as_json()?;
        assert_eq!(
            legacy_schema["$defs"]["note"]["properties"]["gap"]["minLength"],
            1
        );
        Ok(())
    }

    #[test]
    fn research_v5_status_sources_are_a_validated_bounded_set_not_a_repair_reason()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut document = json!({"schema_version":5,"decision":{"kind":"progress","note":{"goal":"Inspect","finding_kind":"hypothesis","finding":"Working","finding_source_refs":["S3","S1","S3","S1"],"gap":"","next_step":""}},"work":{"questions":[],"results":[]}});
        let phase = ResearchOutputPhase::Analyze(ResearchQuestionId::FIRST);
        let crate::AskResearchDecision::Answer {
            note,
            evidence_status,
            ..
        } = crate::DecodeAskResearchDecision.decode_phase(&document.to_string(), phase)?
        else {
            return Err("progress".into());
        };
        assert_eq!(note.source_ordinals, vec![3, 1]);
        assert_eq!(
            evidence_status,
            crate::AskResearchEvidenceStatus::Incomplete
        );
        assert!(note.work.ok_or("work")?.results.is_empty());
        document["decision"]["note"]["finding_source_refs"] = json!(vec!["S1"; 32]);
        assert!(
            crate::DecodeAskResearchDecision
                .decode_phase(&document.to_string(), phase)
                .is_ok()
        );
        for refs in [
            json!(vec!["S1"; 33]),
            json!(["S1", "S1", "S0"]),
            json!(["S1", "S1", "S201"]),
            json!(["S1", "S1", "S01"]),
            json!(["S1", "S1", "S+1"]),
            json!(["S1", "S1", null]),
            json!(["S1", "S1", 3]),
            json!(["S1", "S1", "E1"]),
        ] {
            document["decision"]["note"]["finding_source_refs"] = refs;
            assert!(
                crate::DecodeAskResearchDecision
                    .decode_phase(&document.to_string(), phase)
                    .is_err()
            );
        }
        let schema = research_work_phase_schema(phase, true)?;
        assert_ne!(
            schema["$defs"]["v5StatusNote"]["properties"]["finding_source_refs"]["uniqueItems"],
            true
        );
        assert_eq!(
            schema["$defs"]["v5StatusNote"]["properties"]["finding_source_refs"]["maxItems"],
            32
        );
        let legacy = crate::DecodeAskResearchDecision.json_schema().as_json()?;
        assert_eq!(
            legacy["$defs"]["note"]["properties"]["finding_source_refs"]["uniqueItems"],
            true
        );
        let legacy_note = json!({"goal":"Inspect","finding_kind":"hypothesis","finding":"Working","finding_source_refs":["S1","S1"],"gap":"Open","next_step":"Check"});
        for version in [4, 5] {
            let historical = json!({"schema_version":version,"decision":{"kind":"answer","note":legacy_note,"evidence_status":"incomplete","markdown":"Pending","source_refs":[]},"work":{"questions":[],"results":[]}});
            let mut historical = historical;
            if version == 4 {
                historical.as_object_mut().ok_or("document")?.remove("work");
            }
            assert!(
                crate::DecodeAskResearchDecision
                    .decode(&historical.to_string())
                    .is_err()
            );
        }
        Ok(())
    }

    #[test]
    fn v5_unreferenced_status_is_a_hypothesis_not_a_repair_or_source_claim()
    -> Result<(), Box<dyn std::error::Error>> {
        let note = json!({"goal":"Plan new tests","finding_kind":"conclusion","finding":"Proposed new test cases","finding_source_refs":[],"gap":"No further design gap","next_step":"Format plan"});
        let raw = json!({"schema_version":5,"decision":{"kind":"progress","note":note},"work":{"questions":[],"results":[]}});
        let crate::AskResearchDecision::Answer { note: admitted, .. } =
            crate::DecodeAskResearchDecision.decode_phase(
                &raw.to_string(),
                ResearchOutputPhase::Analyze(ResearchQuestionId::FIRST),
            )?
        else {
            return Err("progress".into());
        };
        assert_eq!(
            admitted.finding_kind,
            crate::AskResearchFindingKind::Hypothesis
        );
        assert!(admitted.work.as_ref().ok_or("work")?.results.is_empty());
        let legacy = json!({"schema_version":4,"decision":{"kind":"answer","note":note,"evidence_status":"incomplete","markdown":"Pending","source_refs":[]}});
        assert_eq!(
            crate::DecodeAskResearchDecision.decode(&legacy.to_string()),
            Err(DecodeError::MissingSources)
        );
        Ok(())
    }
    #[test]
    fn design_progress_requires_an_answer_but_preserves_consequential_questions() {
        let note = json!({"goal":"Entwurf", "finding_kind":"hypothesis", "finding":"Auswertung", "finding_source_refs":[], "gap":"Entscheidung", "next_step":"Entwurf festlegen"});
        let mut document = json!({"schema_version":5,"work":{"questions":[],"results":[]},"decision":{"kind":"progress","note":note}});
        let design = ResearchOutputPhase::Design(ResearchQuestionId::FIRST);
        assert!(
            crate::DecodeAskResearchDecision
                .decode_phase(&document.to_string(), design)
                .is_err(),
            "an empty design is invalid output, not a request for repository reads"
        );
        assert!(
            crate::DecodeAskResearchDecision
                .decode_phase(
                    &document.to_string(),
                    ResearchOutputPhase::Analyze(ResearchQuestionId::FIRST)
                )
                .is_ok()
        );
        document["decision"] = json!({"kind":"question","note":note,"message":"Sollen vorhandene Nutzerdaten ersetzt oder erhalten werden?"});
        assert!(
            crate::DecodeAskResearchDecision
                .decode_phase(&document.to_string(), design)
                .is_ok()
        );
        document["decision"] = json!({"kind":"progress","note":note});
        document["work"]["results"] = json!([{"question_id":1,"kind":"designDecision","text":"Vorhandene Daten erhalten und neue Einträge über die bestehende API ergänzen.","evidence":[]}]);
        assert!(
            crate::DecodeAskResearchDecision
                .decode_phase(&document.to_string(), design)
                .is_ok()
        );
    }

    #[test]
    fn runtime_rejects_documents_from_another_phase_without_relying_on_provider_schema() {
        let note = json!({"goal":"Logziel", "finding_kind":"hypothesis", "finding":"Auswertung", "finding_source_refs":[], "gap":"Konstruktor", "next_step":"Beleg auswerten"});
        let mut initial = json!({"schema_version":5,"work":{"questions":[{"outcome":"Logziel erklären", "kind":"repository", "priority":"required", "dependencies":[]}],"results":[]},"decision":{"kind":"progress", "note":note}});
        assert!(
            crate::DecodeAskResearchDecision
                .decode_phase(&initial.to_string(), ResearchOutputPhase::Initialize)
                .is_ok()
        );
        assert!(
            crate::DecodeAskResearchDecision
                .decode_phase(&initial.to_string(), ResearchOutputPhase::Finalize)
                .is_err()
        );
        initial["work"]["questions"] = json!([]);
        let active = ResearchOutputPhase::Analyze(ResearchQuestionId::FIRST);
        assert!(
            crate::DecodeAskResearchDecision
                .decode_phase(&initial.to_string(), active)
                .is_ok()
        );
        initial["decision"]["kind"] = json!("answer");
        initial["decision"]["markdown"] = json!("Recherchezwischenstand.");
        initial["decision"]["source_refs"] = json!([]);
        initial["decision"]["evidence_status"] = json!("incomplete");
        assert!(
            crate::DecodeAskResearchDecision
                .decode(&initial.to_string())
                .is_ok()
        );
        assert!(
            crate::DecodeAskResearchDecision
                .decode_phase(&initial.to_string(), active)
                .is_err()
        );
        initial["decision"] = json!({"kind":"progress", "note":note});
        initial["work"]["results"] = json!([{"question_id":2,"kind":"interpretation","text":"Logziel", "evidence":[{"anchor_ref":"E1"}]}]);
        assert!(
            crate::DecodeAskResearchDecision
                .decode_phase(&initial.to_string(), active)
                .is_err()
        );
        initial["work"]["results"][0]["question_id"] = json!(1);
        assert!(
            crate::DecodeAskResearchDecision
                .decode_phase(&initial.to_string(), active)
                .is_ok()
        );
        let design = ResearchOutputPhase::Design(ResearchQuestionId::FIRST);
        assert!(
            crate::DecodeAskResearchDecision
                .decode_phase(&initial.to_string(), design)
                .is_err()
        );
        initial["work"]["results"][0]["kind"] = json!("designDecision");
        assert!(
            crate::DecodeAskResearchDecision
                .decode_phase(&initial.to_string(), design)
                .is_err()
        );
        initial["work"]["results"][0]["evidence"] = json!([]);
        assert!(
            crate::DecodeAskResearchDecision
                .decode_phase(&initial.to_string(), design)
                .is_ok()
        );
        assert_eq!(
            research_work_phase_schema(design, false).map(|schema| {
                schema["$defs"]["result"]["properties"]["evidence"]["maxItems"].clone()
            }),
            Ok(json!(0))
        );
        initial["work"]["results"][0]["kind"] = json!("interpretation");
        initial["work"]["results"][0]["evidence"] = json!([{"source_ref":"S1","quote":"log.txt"}]);
        assert!(
            crate::DecodeAskResearchDecision
                .decode(&initial.to_string())
                .is_ok()
        );
        assert!(
            crate::DecodeAskResearchDecision
                .decode_phase(&initial.to_string(), active)
                .is_err()
        );
    }
    #[test]
    fn v5_schema_is_closed_and_has_no_model_fact_or_completion_status() -> Result<(), DecodeError> {
        let schema = research_work_decision_schema()?;
        assert_eq!(schema["properties"]["schema_version"]["const"], 5);
        assert_eq!(schema["$defs"]["result"]["additionalProperties"], false);
        assert_eq!(
            schema["$defs"]["work"]["required"],
            json!(["questions", "results"])
        );
        assert!(
            schema["$defs"]["result"]["properties"]
                .get("status")
                .is_none()
        );
        assert!(decode_work(&json!({"questions":[],"results":[],"completed":true})).is_err());
        Ok(())
    }
}
