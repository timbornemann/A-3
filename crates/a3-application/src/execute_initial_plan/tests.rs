use super::*;
use crate::{ModelWireThinking, StructuredOutputSchema};
use a3_domain::{
    ModelCapabilities, ModelContextLimit, ModelId, ModelOutputLimit, ModelParallelismLimit,
    ModelProfileSettings, ModelPromptSchemaGrounding, ModelProviderId, ModelSamplingProfile,
    ModelStopSequences, ModelStructuredOutputCapability, ModelTemperature,
    ModelTokenCountingStrategy, ModelToolCallMode, ModelTopP,
};
use serde_json::json;

type TestResult<T = ()> = Result<T, Box<dyn Error>>;

#[test]
fn explicit_plan_halts_settle_positive_usage_without_materialization_or_structure_failure() {
    for reason in [PlanPhaseHalt::Refuse, PlanPhaseHalt::Clarify] {
        let observed = PlanObservation {
            outcome: Ok(PlanPhaseOutcome::Halted(reason)),
            rejection: InitialPlanRejection::RejectedReply,
            usage: ModelProviderUsage::new(Some(20), Some(7)),
        };
        let (settled, result) = observed.into_settlement();
        assert!(
            matches!(settled, InitialPlanFinish::Halted {reason: actual, prompt_tokens:20,output_tokens:7} if actual == reason)
        );
        assert!(matches!(result, Ok(PlanDisposition::Halted(actual)) if actual == reason));
        for usage in [
            ModelProviderUsage::new(Some(20), None),
            ModelProviderUsage::new(Some(0), Some(7)),
            ModelProviderUsage::new(Some(20), Some(0)),
        ] {
            let observed = PlanObservation {
                outcome: Ok(PlanPhaseOutcome::Halted(reason)),
                rejection: InitialPlanRejection::RejectedReply,
                usage,
            };
            assert!(matches!(
                observed.into_settlement().1,
                Err(InitialPlanExecutionFailure::Usage)
            ));
        }
    }
}

#[test]
fn repair_only_changes_system_instruction_and_keeps_repeated_grounding() -> TestResult {
    let profile = repair_profile()?;
    let schema =
        StructuredOutputSchema::new(json!({"type":"object","additionalProperties":false}))?;
    let system = format!(
        "Plan v1 JSON only. Echo job P0123456789abcdef. Schema: {}",
        schema.value()
    );
    let data = ModelMessage::try_from_string(
        ModelMessageRole::User,
        r#"{"source":"def plan():\r\n    return 1\r\n"}"#.to_owned(),
    )?;
    let initial = ModelProviderRequest::new(
        profile,
        vec![
            ModelMessage::try_from_string(ModelMessageRole::System, system.clone())?,
            data,
        ],
        Some(schema),
    )?;
    let repaired = repair_request(&initial, false)?;
    assert_eq!(repaired.profile(), initial.profile());
    assert_eq!(repaired.structured_output(), initial.structured_output());
    assert_eq!(&repaired.messages()[1..], &initial.messages()[1..]);
    assert_eq!(repaired.messages()[0].role(), ModelMessageRole::System);
    assert_eq!(
        repaired.messages()[0].content(),
        format!("{system}{REPAIR_INSTRUCTION}")
    );
    assert_ne!(repaired, initial);

    // Frozen A04 Qwen run 005 measured the complete initial request at 2658 bytes.
    // Only this system string changes, so JSON's encoded delta is the actual wire growth.
    const A04_INITIAL_REQUEST_BYTES: usize = 2_658;
    const FOUR_K_REQUEST_CAP: usize = 2_688;
    let initial_system = serde_json::to_vec(initial.messages()[0].content())?;
    let repaired_system = serde_json::to_vec(repaired.messages()[0].content())?;
    let overhead = repaired_system
        .len()
        .checked_sub(initial_system.len())
        .ok_or("repair unexpectedly shortened its original system instruction")?;
    assert!(overhead <= FOUR_K_REQUEST_CAP - A04_INITIAL_REQUEST_BYTES);

    // Reuse the measured content-free body length with the real phase budget checker;
    // no provider call, request truncation or larger context is involved.
    let format_bytes = serde_json::to_vec(
        repaired
            .structured_output()
            .ok_or("repair lost its output schema")?
            .value(),
    )?
    .len();
    let projection = |bytes| {
        ModelWireProjection::from_serialized(
            &vec![b'x'; bytes],
            repaired
                .messages()
                .iter()
                .map(|message| message.content().len())
                .sum(),
            format_bytes,
            4_096,
            1_024,
            ModelWireThinking::Disabled,
        )
    };
    PhaseWireBudget::fit(
        projection(A04_INITIAL_REQUEST_BYTES + overhead)?,
        repaired.profile(),
    )?;
    // The previous suffix added 31 encoded bytes, exceeding the actual 4k budget by one.
    assert_eq!(
        PhaseWireBudget::fit(
            projection(A04_INITIAL_REQUEST_BYTES + 31)?,
            initial.profile()
        ),
        Err(PhaseWireBudgetError::Capacity)
    );
    Ok(())
}

#[test]
fn add_selection_repair_preserves_the_request_and_uses_a_bounded_control_hint() -> TestResult {
    let schema = StructuredOutputSchema::new(json!({
        "type":"object","required":["decision","job","version"],"additionalProperties":false,
        "properties":{"version":{"const":3},"job":{"const":"P0123456789abcdef"},
            "decision":{"enum":["add","refuse","clarify","tooLarge"]}}
    }))?;
    let system = format!(
        "Plan v3 JSON only. Choose a decision. Echo job P0123456789abcdef. Schema: {}",
        schema.value()
    );
    let initial = ModelProviderRequest::new(
        repair_profile()?,
        vec![
            ModelMessage::try_from_string(ModelMessageRole::System, system.clone())?,
            ModelMessage::try_from_string(ModelMessageRole::User,
                r#"{"inventory":"emptyPublishedIndex","requirements":[{"text":"Implement the service","must":true}],"constraints":["No test files."],"offers":[{"offer":"P1","operation":"add","target":"server.py"}]}"#.to_owned())?,
        ],
        Some(schema),
    )?;
    let repaired = repair_request(&initial, true)?;
    assert_eq!(repaired.profile(), initial.profile());
    assert_eq!(repaired.structured_output(), initial.structured_output());
    assert_eq!(&repaired.messages()[1..], &initial.messages()[1..]);
    let instruction = repaired.messages()[0].content();
    assert_eq!(
        instruction,
        format!("{system}{ADD_SELECTION_REPAIR_INSTRUCTION}")
    );
    assert!(!instruction.contains("name a change"));
    assert!(!instruction.contains("outcome"));
    let encoded_overhead = serde_json::to_vec(instruction)?.len()
        - serde_json::to_vec(initial.messages()[0].content())?.len();
    assert_eq!(encoded_overhead, 28);
    // This only bounds the new hint against the existing measured 30-byte repair allowance.
    // The concrete provider still projects and fits every complete V3 request before dispatch.
    let projection = ModelWireProjection::from_serialized(
        &vec![b'x'; 2_658 + encoded_overhead],
        repaired
            .messages()
            .iter()
            .map(|message| message.content().len())
            .sum(),
        serde_json::to_vec(
            repaired
                .structured_output()
                .ok_or("schema missing")?
                .value(),
        )?
        .len(),
        4_096,
        1_024,
        ModelWireThinking::Disabled,
    )?;
    PhaseWireBudget::fit(projection, repaired.profile())?;
    Ok(())
}

fn repair_profile() -> TestResult<ModelProfile> {
    Ok(ModelProfile::from_probe(
        ModelProviderId::try_from_string("plan-test-provider".to_owned())?,
        ModelId::try_from_string("not-dispatched".to_owned())?,
        ModelProfileSettings::new(
            ModelContextLimit::new(4_096)?,
            ModelOutputLimit::new(1_024)?,
            ModelTokenCountingStrategy::ConservativeUtf8BytesV1,
            ModelParallelismLimit::new(1)?,
            ModelSamplingProfile::new(
                ModelTemperature::from_milli(0)?,
                ModelTopP::from_milli(1_000)?,
            ),
            ModelStopSequences::new(Vec::new())?,
            ModelPromptSchemaGrounding::RepeatSchemaInPrompt,
        )?,
        ModelCapabilities::new(
            ModelStructuredOutputCapability::Verified,
            ModelToolCallMode::Disabled,
        ),
    ))
}

#[test]
fn regular_missing_evidence_and_size_are_settled_without_a_plan() {
    let offer = TaskStepId::from_bytes([6; 32]);
    let usage = ModelProviderUsage::new(Some(20), Some(7));
    let need = PlanObservation {
        outcome: Ok(PlanPhaseOutcome::NeedEvidence {
            offer,
            kind: PlanEvidenceKind::Callers,
        }),
        rejection: InitialPlanRejection::RejectedReply,
        usage,
    };
    let (settlement, disposition) = need.into_settlement();
    assert!(matches!(
        settlement,
        InitialPlanFinish::NeedEvidence {
            prompt_tokens: 20,
            output_tokens: 7
        }
    ));
    assert!(matches!(
        disposition,
        Ok(PlanDisposition::NeedEvidence { offer: id, kind: PlanEvidenceKind::Callers }) if id == offer
    ));
    let too_large = PlanObservation {
        outcome: Ok(PlanPhaseOutcome::TooLarge),
        rejection: InitialPlanRejection::RejectedReply,
        usage,
    };
    let (settlement, disposition) = too_large.into_settlement();
    assert!(matches!(
        settlement,
        InitialPlanFinish::TooLarge {
            prompt_tokens: 20,
            output_tokens: 7
        }
    ));
    assert!(matches!(disposition, Ok(PlanDisposition::TooLarge)));
}

#[test]
fn partial_usage_and_transport_failure_never_become_materializable() {
    let incomplete = PlanObservation {
        outcome: Ok(PlanPhaseOutcome::TooLarge),
        rejection: InitialPlanRejection::RejectedReply,
        usage: ModelProviderUsage::new(Some(20), None),
    };
    let (settlement, disposition) = incomplete.into_settlement();
    assert!(matches!(
        settlement,
        InitialPlanFinish::ObservedFailure {
            kind: InitialPlanRejection::Unknown,
            reported_prompt_tokens: Some(20),
            reported_output_tokens: None,
        }
    ));
    assert!(matches!(
        disposition,
        Err(InitialPlanExecutionFailure::Usage)
    ));

    let unavailable = PlanObservation::from_collection_failure(
        ModelStreamCollectionFailure::Provider(ModelProviderFailure::Unavailable),
    );
    let (settlement, disposition) = unavailable.into_settlement();
    assert!(matches!(
        settlement,
        InitialPlanFinish::ObservedFailure {
            kind: InitialPlanRejection::ProviderFailure,
            reported_prompt_tokens: None,
            reported_output_tokens: None,
        }
    ));
    assert!(matches!(
        disposition,
        Err(InitialPlanExecutionFailure::Provider(
            ModelProviderFailure::Unavailable
        ))
    ));
}
