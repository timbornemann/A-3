mod addition_selection;
mod executor_freshness;

use super::*;
use crate::{
    ModelOperationControl, ModelProviderFailure, ModelProviderFuture, ModelRequestTimeout,
    ModelWireThinking,
};
use a3_domain::{
    AcceptanceCriterion, AcceptanceCriterionStatement, AgentFileStartLine, CanonicalDirectory,
    CommandDiscoveryEvidence, CommandDiscoverySchemaVersion, CriterionEvidenceRequirement,
    CriterionOrigin, DiffInvariantVerification, DiscoveredCommand, GitHead, GitReferenceName,
    GoalConstraint, GoalContractDraft, GoalContractTimestamp, GoalObjective, GoalOriginalRequest,
    IndexPublication, IndexRunRecord, IndexRunSequence, IndexRunStatus, LinkedGraph,
    MinimumTestCaseCount, ModelCapabilities, ModelContextLimit, ModelId, ModelOutputLimit,
    ModelParallelismLimit, ModelProfileSettings, ModelProviderId, ModelSamplingProfile,
    ModelStopSequences, ModelStructuredOutputCapability, ModelTemperature,
    ModelTokenCountingStrategy, ModelToolCallMode, ModelTopP, ModulePolicyVersion,
    ModuleProjection, ModuleSymbolSet, RankProjection, RankingPolicyVersion, RepositoryCard,
    RepositoryId, RepositoryIdentity, RepositoryPath, SourcePosition, SourceRange,
    SuccessVerification, TaskId, VerificationRequirement, VerificationSpecId, WorkspaceDirectory,
    WorktreeAnchorId, WorktreeIdentity,
};

type TestResult<T = ()> = Result<T, Box<dyn Error>>;
const CONSTRAINT: &str = "Preserve exact fractions and the current public function.";
const SOURCE: &str = "def mean(values):\n    return sum(values) // len(values)\n";

#[test]
fn loopback_offer_can_only_prove_the_exact_original_recipe() -> TestResult {
    use a3_domain::{
        LoopbackExpectedBody, LoopbackHeaderChecks, LoopbackHttpCheck, LoopbackHttpRecipe,
        LoopbackMethod, LoopbackProbe, LoopbackStart, TaskStepId,
    };
    let fixture = Fixture::new(SOURCE)?;
    let port = std::num::NonZeroU16::try_from(8765)?;
    let recipe = |status| {
        LoopbackHttpRecipe::new(
            fixture.path.clone(),
            LoopbackStart::Default,
            port,
            vec![LoopbackHttpCheck::new(
                LoopbackProbe::new(LoopbackMethod::Get, "/health".to_owned())?,
                status,
                LoopbackExpectedBody::AnyBounded,
                LoopbackHeaderChecks::default(),
            )?],
        )
    };
    let goal = fixture.goal(
        1,
        CriterionEvidenceRequirement::LoopbackHttp(recipe(200)?),
        CONSTRAINT,
    )?;
    let criterion = goal.draft().acceptance_criteria()[0].id();
    let offer = |status| -> TestResult<PlanStepOffer> {
        Ok(PlanStepOffer::new(
            TaskStepId::from_bytes([68; 32]),
            Vec::new(),
            vec![criterion],
            VerificationSpec::loopback_http(
                VerificationSpecId::from_bytes([69; 32]),
                VerificationRequirement::try_from_string(
                    "Verify the exact HTTP endpoint".to_owned(),
                )?,
                recipe(status)?,
            ),
        ))
    };
    let state = fixture.state(&goal, None)?;
    assert!(matches!(
        PreparedPlanPhase::new(state, vec![offer(404)?], None),
        Err(PlanPhaseFailure::InvalidOffer)
    ));
    let exact = offer(200)?;
    let expected = exact.verification.clone();
    let step = exact.id;
    let phase = PreparedPlanPhase::new(state, vec![exact], None)?;
    let view: Value = serde_json::from_str(phase.request().messages()[1].content())?;
    assert_eq!(view["offers"][0]["check"], "http");
    assert_eq!(view["offers"][0]["proves"], json!(["R1"]));
    let system = phase.request().messages()[0].content();
    assert!(system.contains("Summarize each outcome in one line"));
    assert!(system.contains("max 64 UTF-8 bytes"));
    assert!(system.contains("No newlines, control characters or copied response bodies"));
    let schema = phase
        .request()
        .structured_output()
        .ok_or("missing Plan schema")?
        .value();
    assert_eq!(
        schema["oneOf"][0]["properties"]["steps"]["items"]["properties"]["after"]["maxItems"],
        0
    );
    let self_dependency = json!({
        "version": 1, "job": phase.alias,
        "steps": [{"offer": "P1", "outcome": "Implement the local service", "after": ["P1"]}],
    });
    assert!(matches!(
        phase.complete(state, &serde_json::to_vec(&self_dependency)?),
        Err(PlanPhaseFailure::InvalidDependency)
    ));
    let long = json!({
        "version": 1, "job": phase.alias,
        "steps": [{"offer": "P1", "outcome": "ä".repeat(33)}],
    });
    assert!(matches!(
        phase.complete(state, &serde_json::to_vec(&long)?),
        Err(PlanPhaseFailure::InvalidReply)
    ));
    let copied_body = json!({
        "version": 1, "job": phase.alias,
        "steps": [{"offer": "P1", "outcome": "Implement the response body ready\n"}],
    });
    assert!(matches!(
        phase.complete(state, &serde_json::to_vec(&copied_body)?),
        Err(PlanPhaseFailure::InvalidReply)
    ));
    let reply = json!({
        "version": 1, "job": phase.alias,
        "steps": [{"offer": "P1", "outcome": "Implement the requested HTTP endpoint"}],
    });
    let PlanPhaseOutcome::Steps(plan) = phase.complete(state, &serde_json::to_vec(&reply)?)? else {
        return Err("expected the exact HTTP verification step".into());
    };
    let definition = plan
        .ledger()
        .step(step)
        .ok_or("missing HTTP step")?
        .definition();
    assert_eq!(definition.verification_spec(), &expected);
    assert_eq!(definition.acceptance_criteria(), &[criterion]);
    Ok(())
}

#[test]
fn loopback_offer_cannot_downgrade_the_original_startup_coverage() -> TestResult {
    use a3_domain::{
        ContentHash, LoopbackExpectedBody, LoopbackHeaderChecks, LoopbackHttpCheck,
        LoopbackHttpRecipe, LoopbackMethod, LoopbackProbe, LoopbackStart, TaskStepId,
    };
    let fixture = Fixture::new(SOURCE)?;
    let explicit = LoopbackHttpRecipe::new(
        fixture.path.clone(),
        LoopbackStart::Explicit,
        std::num::NonZeroU16::try_from(18_765)?,
        vec![LoopbackHttpCheck::new(
            LoopbackProbe::new(LoopbackMethod::Get, "/health".to_owned())?,
            200,
            LoopbackExpectedBody::Exact {
                hash: ContentHash::from_bytes([9; 32]),
                bytes: 5,
            },
            LoopbackHeaderChecks::default(),
        )?],
    )?;
    let aggregate = explicit.clone().with_startup_coverage(0)?;
    let goal = fixture.goal(
        1,
        CriterionEvidenceRequirement::LoopbackHttp(aggregate.clone()),
        CONSTRAINT,
    )?;
    let criterion = goal.draft().acceptance_criteria()[0].id();
    let offer = |recipe| -> TestResult<PlanStepOffer> {
        Ok(PlanStepOffer::new(
            TaskStepId::from_bytes([68; 32]),
            Vec::new(),
            vec![criterion],
            VerificationSpec::loopback_http(
                VerificationSpecId::from_bytes([69; 32]),
                VerificationRequirement::try_from_string(
                    "Verify the complete service startup behavior".to_owned(),
                )?,
                recipe,
            ),
        ))
    };
    let state = fixture.state(&goal, None)?;
    assert!(matches!(
        PreparedPlanPhase::new(state, vec![offer(explicit)?], None),
        Err(PlanPhaseFailure::InvalidOffer)
    ));
    let exact = offer(aggregate)?;
    let expected = exact.verification.clone();
    let step = exact.id;
    let phase = PreparedPlanPhase::new(state, vec![exact], None)?;
    let view: Value = serde_json::from_str(phase.request().messages()[1].content())?;
    assert_eq!(view["offers"][0]["check"], "http");
    let reply = json!({
        "version": 1, "job": phase.alias,
        "steps": [{"offer": "P1", "outcome": "Implement the requested server startup and responses"}],
    });
    let PlanPhaseOutcome::Steps(plan) = phase.complete(state, &serde_json::to_vec(&reply)?)? else {
        return Err("expected the complete HTTP verification step".into());
    };
    let definition = plan
        .ledger()
        .step(step)
        .ok_or("missing HTTP step")?
        .definition();
    assert_eq!(definition.verification_spec(), &expected);
    assert_eq!(definition.acceptance_criteria(), &[criterion]);
    Ok(())
}

#[test]
fn bounded_active_view_is_deterministic_and_keeps_the_late_constraint() -> TestResult {
    let fixture = Fixture::new(SOURCE)?;
    let goal = fixture.goal(31, fixture.diff_requirement()?, CONSTRAINT)?;
    let state = fixture.state(&goal, None)?;
    let first = PreparedPlanPhase::new(state, fixture.offers(2)?, Some(&fixture.page))?;
    let repeated = PreparedPlanPhase::new(state, fixture.offers(2)?, Some(&fixture.page))?;
    assert_eq!(first.anchor(), repeated.anchor());
    assert_eq!(first.job_digest(), repeated.job_digest());
    assert_eq!(first.request(), repeated.request());
    let messages = first.request().messages();
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].role(), ModelMessageRole::System);
    assert!(!messages[0].content().contains("def mean"));
    assert_eq!(messages[1].role(), ModelMessageRole::User);
    let view: Value = serde_json::from_str(messages[1].content())?;
    assert_eq!(view["outsideView"], 29);
    assert_eq!(view["constraints"], json!([CONSTRAINT]));
    assert_eq!(view["source"], json!({"file":"average.py","text":SOURCE}));
    assert!(!messages[1].content().contains("R31"));
    assert!(!messages[1].content().contains("omitted original detail"));
    assert!(goal.original().ok_or("missing original")?.as_bytes().len() > 16 * 1024);
    assert_eq!(view["offers"][0]["addresses"], json!(["R1"]));
    assert_eq!(view["offers"][1]["proves"], json!(["R2"]));

    let projection = Projector(fixture.profile.provider_id().clone());
    let (actual, budget) = first.project_wire(&projection)?;
    assert_eq!(actual.context_tokens(), 4096);
    assert_eq!(actual.thinking(), ModelWireThinking::Disabled);
    assert!(actual.format_bytes() > 0);
    assert!(budget.prompt_tokens() > actual.request_bytes());
    assert_eq!(budget.output_tokens(), 1024);
    Ok(())
}

#[test]
fn empty_inventory_must_match_the_current_publication_even_for_the_same_run() -> TestResult {
    let fixture = Fixture::new(SOURCE)?;
    let goal = fixture.goal(1, fixture.diff_requirement()?, CONSTRAINT)?;
    let empty = index(fixture.published.run().snapshot_id(), Vec::new())?;
    assert_eq!(empty.run(), fixture.published.run());
    let empty_state = PlanPhaseState::new(&fixture.project, &goal, &empty, &fixture.profile, None)?;
    let phase = PreparedPlanPhase::new(empty_state, fixture.offers(1)?, None)?;
    let schema = phase
        .request()
        .structured_output()
        .ok_or("missing Plan schema")?
        .value();
    let alternatives = schema["oneOf"].as_array().ok_or("missing reply variants")?;
    assert_eq!(alternatives.len(), 2);
    let properties = &schema["oneOf"][0]["properties"]["steps"]["items"]["properties"];
    assert_eq!(properties["after"]["maxItems"], 0);
    let view: Value = serde_json::from_str(phase.request().messages()[1].content())?;
    assert_eq!(view["constraints"], json!([CONSTRAINT]));
    assert!(phase.request().messages()[0].content().starts_with(SYSTEM));
    assert!(
        alternatives
            .iter()
            .all(|variant| variant["properties"].get("need").is_none())
    );
    for kind in ["source", "callers", "verification"] {
        let missing = json!({"version":1,"job":phase.alias,"need":{"offer":"P1","kind":kind}});
        assert!(matches!(
            phase.complete(empty_state, &serde_json::to_vec(&missing)?),
            Err(PlanPhaseFailure::InvalidReply)
        ));
    }
    for outcome in ["tooLarge", "R1", "P1", phase.alias.as_str()] {
        let placeholder = json!({
            "version":1,"job":phase.alias,"steps":[{"offer":"P1","outcome":outcome}],
        });
        assert!(matches!(
            phase.complete(empty_state, &serde_json::to_vec(&placeholder)?),
            Err(PlanPhaseFailure::InvalidReply)
        ));
    }
    let reply = json!({"version":1,"job":phase.alias,"tooLarge":true});
    assert!(matches!(
        phase.complete(empty_state, &serde_json::to_vec(&reply)?),
        Ok(PlanPhaseOutcome::TooLarge)
    ));
    assert!(matches!(
        phase.complete(fixture.state(&goal, None)?, &serde_json::to_vec(&reply)?),
        Err(PlanPhaseFailure::Stale)
    ));
    Ok(())
}

#[test]
fn large_empty_single_offer_shortens_only_the_system_guidance() -> TestResult {
    let fixture = Fixture::new(SOURCE)?;
    let constraint = "Preserve every current input and output. ".repeat(30);
    let goal = fixture.goal(1, fixture.diff_requirement()?, constraint.trim_end())?;
    let empty = index(fixture.published.run().snapshot_id(), Vec::new())?;
    let current = PlanPhaseState::new(&fixture.project, &goal, &empty, &fixture.profile, None)?;
    let phase = PreparedPlanPhase::new(current, fixture.offers(1)?, None)?;
    let messages = phase.request().messages();
    assert!(messages[0].content().starts_with(COMPACT_EMPTY_OFFER));
    assert!(messages[0].content().contains("max 64 UTF-8 bytes"));
    let view: Value = serde_json::from_str(messages[1].content())?;
    assert_eq!(view["constraints"], json!([constraint.trim_end()]));
    assert_eq!(view["offers"][0]["addresses"], json!(["R1"]));
    let (_, budget) = phase.project_wire(&Projector(fixture.profile.provider_id().clone()))?;
    assert_eq!(budget.output_tokens(), 1024);
    Ok(())
}

#[test]
fn plan_caption_grammar_rejects_escaped_controls_and_preserves_unicode() -> TestResult {
    let fixture = Fixture::new(SOURCE)?;
    let goal = fixture.goal(1, fixture.diff_requirement()?, CONSTRAINT)?;
    let state = fixture.state(&goal, None)?;
    let phase = PreparedPlanPhase::new(state, fixture.offers(1)?, None)?;
    let schema = phase
        .request()
        .structured_output()
        .ok_or("missing Plan schema")?
        .value();
    assert_eq!(
        schema["oneOf"][0]["properties"]["steps"]["items"]["properties"]["outcome"]["pattern"],
        r#"^[^"\\\x00-\x1F\x7F-\x9F]{1,64}$"#,
    );
    let reply = |outcome: &str| {
        serde_json::to_vec(&json!({
            "version":1,"job":phase.alias,
            "steps":[{"offer":"P1","outcome":outcome}],
        }))
    };
    for code in (0..=0x1f).chain(0x7f..=0x9f) {
        let character = char::from_u32(code).ok_or("invalid control fixture")?;
        let caption = format!("Implement{character}the server");
        assert!(matches!(
            phase.complete(state, &reply(&caption)?),
            Err(PlanPhaseFailure::InvalidReply)
        ));
    }
    for caption in [r#"Preserve the "name" field"#, r"Edit src\main.rs"] {
        assert!(matches!(
            phase.complete(state, &reply(caption)?),
            Err(PlanPhaseFailure::InvalidReply)
        ));
    }
    let caption = "Implement Grüße und 東京 🚀 in server.py";
    let PlanPhaseOutcome::Steps(plan) = phase.complete(state, &reply(caption)?)? else {
        return Err("expected a Unicode Plan caption".into());
    };
    assert_eq!(
        plan.ledger()
            .step(step_id(1))
            .ok_or("missing Plan caption step")?
            .definition()
            .intended_outcome()
            .as_str(),
        caption,
    );
    Ok(())
}

#[test]
fn model_plan_captions_use_sixty_four_utf8_bytes_without_changing_domain_text() -> TestResult {
    let fixture = Fixture::new(SOURCE)?;
    let goal = fixture.goal(1, fixture.diff_requirement()?, CONSTRAINT)?;
    let state = fixture.state(&goal, None)?;
    let phase = PreparedPlanPhase::new(state, fixture.offers(1)?, None)?;
    let schema = phase
        .request()
        .structured_output()
        .ok_or("missing Plan schema")?
        .value();
    let outcome_schema =
        &schema["oneOf"][0]["properties"]["steps"]["items"]["properties"]["outcome"];
    assert_eq!(outcome_schema["maxLength"], 64);
    let reply = |outcome: &str| {
        serde_json::to_vec(&json!({
            "version":1,"job":phase.alias,
            "steps":[{"offer":"P1","outcome":outcome}],
        }))
    };
    for caption in [
        "Implement argument parsing and preserve required server defaults".to_owned(),
        "ä".repeat(32),
        "🚀".repeat(16),
    ] {
        assert_eq!(caption.len(), 64);
        let PlanPhaseOutcome::Steps(plan) = phase.complete(state, &reply(&caption)?)? else {
            return Err("expected a 64-byte Plan caption".into());
        };
        assert_eq!(
            plan.ledger()
                .step(step_id(1))
                .ok_or("missing bounded Plan step")?
                .definition()
                .intended_outcome()
                .as_str(),
            caption,
        );
        let overlong = format!("{caption}x");
        assert_eq!(overlong.len(), 65);
        assert!(TaskStepOutcome::try_from_string(overlong.clone()).is_ok());
        assert!(matches!(
            phase.complete(state, &reply(&overlong)?),
            Err(PlanPhaseFailure::InvalidReply)
        ));
    }
    Ok(())
}

#[test]
fn resolved_diff_and_http_offers_reject_a_target_path_as_the_entire_outcome() -> TestResult {
    use a3_domain::{
        LoopbackExpectedBody, LoopbackHeaderChecks, LoopbackHttpCheck, LoopbackHttpRecipe,
        LoopbackMethod, LoopbackProbe, LoopbackStart,
    };

    let fixture = Fixture::new(SOURCE)?;
    let path = RepositoryPath::try_from_bytes(b"server.py".to_vec())?;
    let recipe = LoopbackHttpRecipe::new(
        path.clone(),
        LoopbackStart::Default,
        std::num::NonZeroU16::try_from(8765)?,
        vec![LoopbackHttpCheck::new(
            LoopbackProbe::new(LoopbackMethod::Get, "/".to_owned())?,
            200,
            LoopbackExpectedBody::AnyBounded,
            LoopbackHeaderChecks::default(),
        )?],
    )?;
    let goal = fixture.goal(
        1,
        CriterionEvidenceRequirement::LoopbackHttp(recipe.clone()),
        CONSTRAINT,
    )?;
    let criterion = goal.draft().acceptance_criteria()[0].id();
    let diff = VerificationSpec::diff_invariant(
        VerificationSpecId::from_bytes([70; 32]),
        VerificationRequirement::try_from_string("Only server.py changes".to_owned())?,
        DiffInvariantVerification::new(DiffInvariantMode::ExactPaths, vec![path])?,
    );
    let http = VerificationSpec::loopback_http(
        VerificationSpecId::from_bytes([71; 32]),
        VerificationRequirement::try_from_string("Verify the original HTTP response".to_owned())?,
        recipe,
    );
    let state = fixture.state(&goal, None)?;
    for (addresses, proves, verification) in [
        (vec![criterion], Vec::new(), diff),
        (Vec::new(), vec![criterion], http),
    ] {
        let phase = PreparedPlanPhase::new(
            state,
            vec![PlanStepOffer::new(
                step_id(1),
                addresses,
                proves,
                verification,
            )],
            None,
        )?;
        let reply = |outcome: &str| {
            serde_json::to_vec(&json!({
                "version":1,"job":phase.alias,
                "steps":[{"offer":"P1","outcome":outcome}],
            }))
        };
        assert!(matches!(
            phase.complete(state, &reply("server.py")?),
            Err(PlanPhaseFailure::InvalidReply)
        ));
        for caption in ["Create server.py", "サーバーを実装する"] {
            let PlanPhaseOutcome::Steps(plan) = phase.complete(state, &reply(caption)?)? else {
                return Err("expected an implementation outcome for the bound target".into());
            };
            assert_eq!(
                plan.ledger()
                    .step(step_id(1))
                    .ok_or("missing implementation step")?
                    .definition()
                    .intended_outcome()
                    .as_str(),
                caption,
            );
        }
    }
    Ok(())
}

#[test]
fn selected_outcomes_keep_core_proofs_and_only_explicit_dependencies() -> TestResult {
    let fixture = Fixture::new(SOURCE)?;
    let goal = fixture.goal(3, fixture.diff_requirement()?, CONSTRAINT)?;
    let state = fixture.state(&goal, None)?;
    let phase = PreparedPlanPhase::new(state, fixture.offers(3)?, None)?;
    let reply = json!({
        "version":1,"job":phase.alias,
        "steps":[
            {"offer":"P2","outcome":"Change the result calculation"},
            {"offer":"P1","outcome":"Preserve the public call shape"},
            {"offer":"P3","outcome":"Update the dependent use","after":["P2"]},
        ],
    });
    let PlanPhaseOutcome::Steps(plan) = phase.complete(state, &serde_json::to_vec(&reply)?)? else {
        return Err("expected Plan steps".into());
    };
    let ledger = plan.ledger();
    let edited = ledger
        .step(step_id(2))
        .ok_or("missing edit step")?
        .definition();
    let preserved = ledger
        .step(step_id(1))
        .ok_or("missing preservation step")?
        .definition();
    let dependent = ledger
        .step(step_id(3))
        .ok_or("missing dependent step")?
        .definition();
    assert_eq!(
        edited.intended_outcome().as_str(),
        "Change the result calculation"
    );
    assert_eq!(edited.acceptance_criteria(), &[criterion_id(2)]);
    assert_eq!(edited.verification_spec(), &fixture.diff_spec(2)?);
    assert_eq!(preserved.addressed_criteria(), &[criterion_id(1)]);
    assert!(preserved.acceptance_criteria().is_empty());
    assert!(preserved.dependencies().is_empty());
    assert_eq!(dependent.dependencies(), &[StepDependency::new(step_id(2))]);
    AdmitAgentGoalExecution.execute_task(&goal, ledger)?;
    Ok(())
}

#[test]
fn known_core_prerequisite_applies_even_when_the_model_omits_it() -> TestResult {
    let fixture = Fixture::new(SOURCE)?;
    let goal = fixture.goal(2, fixture.diff_requirement()?, CONSTRAINT)?;
    let state = fixture.state(&goal, None)?;
    let independent = PreparedPlanPhase::new(state, fixture.offers(2)?, None)?;
    let mut offers = fixture.offers(1)?;
    let check = fixture.offers(2)?.pop().ok_or("check offer missing")?;
    offers.push(check.with_required_offers(vec![step_id(1)]));
    let phase = PreparedPlanPhase::new(state, offers, None)?;
    assert_ne!(phase.job_digest(), independent.job_digest());
    let view: Value = serde_json::from_str(phase.request().messages()[1].content())?;
    assert_eq!(view["offers"][1]["requires"], json!(["P1"]));
    let missing =
        json!({"version":1,"job":phase.alias,"steps":[{"offer":"P2","outcome":"Verify"}]});
    assert!(matches!(
        phase.complete(state, &serde_json::to_vec(&missing)?),
        Err(PlanPhaseFailure::InvalidDependency)
    ));
    let reply = json!({"version":1,"job":phase.alias,"steps":[
        {"offer":"P2","outcome":"Verify the change"},
        {"offer":"P1","outcome":"Implement the change"},
    ]});
    let PlanPhaseOutcome::Steps(plan) = phase.complete(state, &serde_json::to_vec(&reply)?)? else {
        return Err("expected the two plan steps".into());
    };
    let check = plan.ledger().step(step_id(2)).ok_or("check step missing")?;
    assert_eq!(
        check.definition().dependencies(),
        &[StepDependency::new(step_id(1))]
    );
    assert_eq!(check.status(), a3_domain::TaskStepStatus::Pending);
    Ok(())
}

#[test]
fn strict_reply_rejects_duplicate_or_foreign_keys_aliases_and_forward_dependencies() -> TestResult {
    let fixture = Fixture::new(SOURCE)?;
    let goal = fixture.goal(3, fixture.diff_requirement()?, CONSTRAINT)?;
    let state = fixture.state(&goal, None)?;
    for (body, expected) in [
        (
            r#""steps":[{"offer":"P1","outcome":"x","outcome":"y"}]"#,
            PlanPhaseFailure::InvalidReply,
        ),
        (
            r#""steps":[{"offer":"P1","outcome":"x","proof":"fake"}]"#,
            PlanPhaseFailure::InvalidReply,
        ),
        (
            r#""steps":[{"offer":"P1","outcome":"x"}],"tooLarge":null"#,
            PlanPhaseFailure::InvalidReply,
        ),
        (r#""steps":[["P1","x"]]"#, PlanPhaseFailure::InvalidReply),
        (
            r#""steps":[],"tooLarge":true"#,
            PlanPhaseFailure::InvalidReply,
        ),
        (
            r#""steps":[{"offer":"P3","outcome":"x"}]"#,
            PlanPhaseFailure::UnknownOffer,
        ),
        (
            r#""steps":[{"offer":"P1","outcome":"x"},{"offer":"P1","outcome":"y"}]"#,
            PlanPhaseFailure::DuplicateOffer,
        ),
        (
            r#""steps":[{"offer":"P1","outcome":"x","after":["P2"]}]"#,
            PlanPhaseFailure::InvalidDependency,
        ),
        (
            r#""steps":[{"offer":"P1","outcome":"x"},{"offer":"P2","outcome":"y","after":["P1","P1"]}]"#,
            PlanPhaseFailure::InvalidDependency,
        ),
    ] {
        let phase = PreparedPlanPhase::new(state, fixture.offers(2)?, None)?;
        let reply = format!(r#"{{"version":1,"job":"{}",{body}}}"#, phase.alias);
        assert_eq!(
            phase.complete(state, reply.as_bytes()).err(),
            Some(expected)
        );
    }
    let phase = PreparedPlanPhase::new(state, fixture.offers(2)?, None)?;
    assert_eq!(
        phase
            .complete(state, br#"{"version":1,"job":"older","tooLarge":true}"#)
            .err(),
        Some(PlanPhaseFailure::InvalidReply)
    );
    Ok(())
}

#[test]
fn regular_limits_need_no_executable_plan() -> TestResult {
    let fixture = Fixture::new(SOURCE)?;
    let goal = fixture.goal(3, fixture.diff_requirement()?, CONSTRAINT)?;
    let state = fixture.state(&goal, None)?;
    let phase = PreparedPlanPhase::new(state, fixture.offers(2)?, None)?;
    let reply = json!({"version":1,"job":phase.alias,"need":{"offer":"P2","kind":"callers"}});
    assert!(matches!(
        phase.complete(state, &serde_json::to_vec(&reply)?)?,
        PlanPhaseOutcome::NeedEvidence { offer, kind: PlanEvidenceKind::Callers } if offer == step_id(2)
    ));
    let phase = PreparedPlanPhase::new(state, fixture.offers(2)?, None)?;
    let reply = json!({"version":1,"job":phase.alias,"tooLarge":true});
    assert!(matches!(
        phase.complete(state, &serde_json::to_vec(&reply)?)?,
        PlanPhaseOutcome::TooLarge
    ));
    Ok(())
}

#[test]
fn proof_offer_requires_the_original_obligation_and_fresh_core_catalog() -> TestResult {
    let fixture = Fixture::new(SOURCE)?;
    let command = DiscoveredCommand::try_new(
        DiscoveredCommandKind::Test,
        WorkspaceDirectory::Root,
        "python".to_owned(),
        vec!["-B".to_owned(), "-m".to_owned(), "unittest".to_owned()],
        vec![CommandDiscoveryEvidence::File(
            fixture.page.revision().clone(),
        )],
    )?;
    let minimum = MinimumTestCaseCount::new(2)?;
    let goal = fixture.goal(
        1,
        CriterionEvidenceRequirement::ExistingTest {
            command: command.id(),
            selector: TestCaseSelector::All,
            minimum_cases: minimum,
            scope: VerificationScope::Workspace,
            python_integer_translation: None,
        },
        CONSTRAINT,
    )?;
    let make_offer = |minimum| -> TestResult<Vec<PlanStepOffer>> {
        Ok(vec![PlanStepOffer::new(
            step_id(1),
            vec![],
            vec![criterion_id(1)],
            VerificationSpec::test(
                VerificationSpecId::from_bytes([1; 32]),
                VerificationRequirement::try_from_string(
                    "Run the matching existing tests".to_owned(),
                )?,
                command.id(),
                TestCaseSelector::All,
                minimum,
                VerificationScope::Workspace,
            ),
        )])
    };
    assert!(matches!(
        PreparedPlanPhase::new(fixture.state(&goal, None)?, make_offer(minimum)?, None),
        Err(PlanPhaseFailure::InvalidOffer)
    ));
    let catalog = ProjectCommandCatalog::new(
        CommandDiscoverySchemaVersion::V1,
        fixture.project.worktree().id(),
        vec![command.clone()],
    )?;
    let state = fixture.state(&goal, Some(&catalog))?;
    assert!(matches!(
        PreparedPlanPhase::new(state, make_offer(MinimumTestCaseCount::new(1)?)?, None),
        Err(PlanPhaseFailure::InvalidOffer)
    ));
    let phase = PreparedPlanPhase::new(state, make_offer(minimum)?, None)?;
    let newer = index(
        SnapshotId::from_bytes([90; 32]),
        vec![fixture.page.revision().clone()],
    )?;
    let current = PlanPhaseState::new(
        &fixture.project,
        &goal,
        &newer,
        &fixture.profile,
        Some(&catalog),
    )?;
    let reply = json!({"version":1,"job":phase.alias,"tooLarge":true});
    assert!(matches!(
        phase.complete(current, &serde_json::to_vec(&reply)?),
        Err(PlanPhaseFailure::Stale)
    ));

    let changed = FileRevision::new(fixture.path.clone(), ContentHash::from_bytes([91; 32]));
    let newer = index(SnapshotId::from_bytes([92; 32]), vec![changed])?;
    let current = PlanPhaseState::new(
        &fixture.project,
        &goal,
        &newer,
        &fixture.profile,
        Some(&catalog),
    )?;
    assert!(matches!(
        PreparedPlanPhase::new(current, make_offer(minimum)?, None),
        Err(PlanPhaseFailure::InvalidOffer)
    ));
    Ok(())
}

#[test]
fn duplicate_core_identity_and_secrets_never_reach_wire() -> TestResult {
    let fixture = Fixture::new(SOURCE)?;
    let goal = fixture.goal(3, fixture.diff_requirement()?, CONSTRAINT)?;
    let state = fixture.state(&goal, None)?;
    let mut offers = fixture.offers(2)?;
    offers[1].id = offers[0].id;
    assert!(matches!(
        PreparedPlanPhase::new(state, offers, None),
        Err(PlanPhaseFailure::DuplicateIdentity)
    ));
    let secret = fixture.goal(
        3,
        fixture.diff_requirement()?,
        "AWS key AKIAIOSFODNN7EXAMPLE",
    )?;
    assert!(matches!(
        PreparedPlanPhase::new(fixture.state(&secret, None)?, fixture.offers(1)?, None),
        Err(PlanPhaseFailure::UnsafeContent)
    ));
    let stale = FileRevision::new(fixture.path.clone(), ContentHash::from_bytes([98; 32]));
    let wrong = page(stale, SOURCE)?;
    assert!(matches!(
        PreparedPlanPhase::new(state, fixture.offers(1)?, Some(&wrong)),
        Err(PlanPhaseFailure::Stale)
    ));
    Ok(())
}

#[test]
fn accounted_schema_repetition_and_large_exact_source_can_hit_wire_capacity() -> TestResult {
    let source = "x".repeat(2048);
    let mut fixture = Fixture::new(&source)?;
    fixture.profile = profile(ModelPromptSchemaGrounding::RepeatSchemaInPrompt)?;
    let goal = fixture.goal(3, fixture.diff_requirement()?, CONSTRAINT)?;
    let state = fixture.state(&goal, None)?;
    let phase = PreparedPlanPhase::new(state, fixture.offers(3)?, Some(&fixture.page))?;
    let schema = phase
        .request()
        .structured_output()
        .ok_or("schema missing")?;
    assert!(
        phase.request().messages()[0]
            .content()
            .contains(&schema.value().to_string())
    );
    let provider = Projector(fixture.profile.provider_id().clone());
    assert_eq!(
        phase.project_wire(&provider).err(),
        Some(PlanPhaseFailure::WireBudget(PhaseWireBudgetError::Capacity))
    );
    Ok(())
}

struct Fixture {
    project: ProjectIdentity,
    profile: ModelProfile,
    published: PublishedIndex,
    page: AgentSourcePage,
    path: RepositoryPath,
}

impl Fixture {
    fn new(source: &str) -> TestResult<Self> {
        let root =
            CanonicalDirectory::from_canonicalized(std::env::current_dir()?.canonicalize()?)?;
        let repository = RepositoryId::from_bytes([61; 32]);
        let project = ProjectIdentity::new(
            RepositoryIdentity::new(repository, root.clone(), None),
            WorktreeIdentity::new(
                WorktreeId::from_bytes([62; 32]),
                WorktreeAnchorId::from_bytes([63; 32]),
                repository,
                root,
            ),
            GitHead::Unborn {
                reference: GitReferenceName::try_from_full_name("refs/heads/main")?,
            },
        )?;
        let path = RepositoryPath::try_from_bytes(b"average.py".to_vec())?;
        let revision = FileRevision::new(
            path.clone(),
            ContentHash::from_bytes(*blake3::hash(source.as_bytes()).as_bytes()),
        );
        Ok(Self {
            project,
            profile: profile(ModelPromptSchemaGrounding::FormatFieldOnly)?,
            published: index(SnapshotId::from_bytes([64; 32]), vec![revision.clone()])?,
            page: page(revision, source)?,
            path,
        })
    }

    fn state<'a>(
        &'a self,
        goal: &'a GoalContract,
        commands: Option<&'a ProjectCommandCatalog>,
    ) -> TestResult<PlanPhaseState<'a>> {
        Ok(PlanPhaseState::new(
            &self.project,
            goal,
            &self.published,
            &self.profile,
            commands,
        )?)
    }

    fn goal(
        &self,
        count: u8,
        evidence: CriterionEvidenceRequirement,
        constraint: &str,
    ) -> TestResult<GoalContract> {
        let task = TaskId::from_bytes([65; 32]);
        let mut raw = String::new();
        let mut entries = Vec::new();
        for id in 1..=count {
            let start = raw.len();
            let title = format!("Preserve result R{id:02}");
            raw.push_str(&title);
            raw.push('\n');
            raw.push_str(&"omitted original detail ".repeat(24));
            raw.push('\n');
            entries.push((id, title, start..raw.len()));
        }
        let tail = raw.len();
        raw.push_str(constraint);
        let original = GoalOriginalRequest::try_from_bytes(
            GoalContractReference::initial(task),
            raw.into_bytes(),
        )?;
        let criteria = entries
            .into_iter()
            .map(|(id, title, range)| {
                Ok(AcceptanceCriterion::new(
                    criterion_id(id),
                    AcceptanceCriterionStatement::try_from_string(title)?,
                )
                .with_origin(CriterionOrigin::new(
                    original.range(range.start, range.end)?,
                    evidence.clone(),
                )))
            })
            .collect::<TestResult<Vec<_>>>()?;
        let constraint = GoalConstraint::try_from_string(constraint.to_owned())?
            .with_origin(original.range(tail, original.as_bytes().len())?);
        let draft = GoalContractDraft::new(
            GoalObjective::try_from_string("Keep all requested results".to_owned())?,
            criteria,
            vec![constraint],
            vec![],
            vec![],
            SuccessVerification::try_from_string("Check the requested results".to_owned())?,
        )?;
        Ok(
            GoalContract::initial(task, draft, GoalContractTimestamp::from_unix_millis(1)?)
                .with_original(original)?,
        )
    }

    fn diff_requirement(&self) -> TestResult<CriterionEvidenceRequirement> {
        Ok(CriterionEvidenceRequirement::ChangeOnly(self.invariant()?))
    }

    fn invariant(&self) -> TestResult<DiffInvariantVerification> {
        Ok(DiffInvariantVerification::new(
            DiffInvariantMode::ExactPaths,
            vec![self.path.clone()],
        )?)
    }

    fn diff_spec(&self, id: u8) -> TestResult<VerificationSpec> {
        Ok(VerificationSpec::diff_invariant(
            VerificationSpecId::from_bytes([id; 32]),
            VerificationRequirement::try_from_string("Verify the exact changed path".to_owned())?,
            self.invariant()?,
        ))
    }

    fn offers(&self, count: u8) -> TestResult<Vec<PlanStepOffer>> {
        (1..=count)
            .map(|id| {
                let criterion = vec![criterion_id(id)];
                let (addresses, proves) = if id == 1 {
                    (criterion, vec![])
                } else {
                    (vec![], criterion)
                };
                Ok(PlanStepOffer::new(
                    step_id(id),
                    addresses,
                    proves,
                    self.diff_spec(id)?,
                ))
            })
            .collect()
    }
}

fn index(snapshot: SnapshotId, files: Vec<FileRevision>) -> TestResult<PublishedIndex> {
    let file_count = files.len().try_into()?;
    let graph = LinkedGraph::new(snapshot, files, Vec::new(), Vec::new(), Vec::new())?;
    let ranking = RankProjection::new(snapshot, RankingPolicyVersion::v1(), Vec::new())?;
    let policy = ModulePolicyVersion::v1();
    let card = RepositoryCard::new(
        snapshot,
        policy,
        Vec::new(),
        Vec::new(),
        ModuleSymbolSet::empty(),
        file_count,
        0,
    )?;
    let modules = ModuleProjection::new(snapshot, policy, Vec::new(), Vec::new(), card)?;
    Ok(PublishedIndex::new(
        IndexRunRecord::new(
            IndexRunId::from_bytes([66; 32]),
            snapshot,
            RankingPolicyVersion::v1(),
            IndexRunSequence::new(1)?,
            IndexRunStatus::Published,
        ),
        IndexPublication::new(graph, ranking, Vec::new(), modules)?,
    )?)
}

fn page(revision: FileRevision, source: &str) -> TestResult<AgentSourcePage> {
    let rows = source.bytes().filter(|value| *value == b'\n').count();
    let last = source.rsplit('\n').next().ok_or("missing last row")?;
    Ok(AgentSourcePage::new(
        revision,
        SourceRange::new(
            0,
            source.len(),
            SourcePosition::new(0, 0),
            SourcePosition::new(u32::try_from(rows)?, u32::try_from(last.len())?),
        )?,
        AgentFileStartLine::new(1)?,
        source.to_owned(),
        None,
        false,
    )?)
}

fn profile(grounding: ModelPromptSchemaGrounding) -> TestResult<ModelProfile> {
    Ok(ModelProfile::from_probe(
        ModelProviderId::try_from_string("plan-fixture".to_owned())?,
        ModelId::try_from_string("not-invoked".to_owned())?,
        ModelProfileSettings::new(
            ModelContextLimit::new(4096)?,
            ModelOutputLimit::new(1024)?,
            ModelTokenCountingStrategy::ConservativeUtf8BytesV1,
            ModelParallelismLimit::new(1)?,
            ModelSamplingProfile::new(
                ModelTemperature::from_milli(0)?,
                ModelTopP::from_milli(1000)?,
            ),
            ModelStopSequences::empty(),
            grounding,
        )?,
        ModelCapabilities::new(
            ModelStructuredOutputCapability::Verified,
            ModelToolCallMode::Disabled,
        ),
    ))
}

fn criterion_id(id: u8) -> AcceptanceCriterionId {
    AcceptanceCriterionId::from_bytes([id; 32])
}
fn step_id(id: u8) -> TaskStepId {
    TaskStepId::from_bytes([id; 32])
}

// Exercise the real projection and capacity boundary without transport. This fixture is
// an encoder contract only, not a claim about actual Ollama output or task correctness.
#[derive(Debug)]
struct Projector(ModelProviderId);

impl ModelProvider for Projector {
    fn provider_id(&self) -> &ModelProviderId {
        &self.0
    }

    fn stream<'a>(
        &'a self,
        _request: &'a ModelProviderRequest,
        _timeout: ModelRequestTimeout,
        _control: &'a dyn ModelOperationControl,
    ) -> ModelProviderFuture<'a> {
        Box::pin(async { Err(ModelProviderFailure::Unavailable) })
    }

    fn project_wire_request(
        &self,
        request: &ModelProviderRequest,
    ) -> Result<ModelWireProjection, ModelProviderFailure> {
        let messages = request
            .messages()
            .iter()
            .map(ModelMessage::content)
            .collect::<Vec<_>>();
        let schema = request
            .structured_output()
            .ok_or(ModelProviderFailure::Rejected)?
            .value();
        let body = serde_json::to_vec(&json!({"messages":messages,"format":schema,"think":false}))
            .map_err(|_| ModelProviderFailure::Rejected)?;
        ModelWireProjection::from_serialized(
            &body,
            messages.iter().map(|text| text.len()).sum(),
            schema.to_string().len(),
            request.profile().settings().context_limit().get(),
            request.profile().settings().output_limit().get(),
            ModelWireThinking::Disabled,
        )
    }
}
