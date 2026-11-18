use super::*;
use a3_domain::PlannedFileAddition;

fn setup() -> TestResult<(Fixture, GoalContract)> {
    let mut fixture = Fixture::new(SOURCE)?;
    fixture.path = RepositoryPath::try_from_bytes(b"server.py".to_vec())?;
    fixture.published = index(fixture.published.run().snapshot_id(), Vec::new())?;
    let goal = fixture.goal(1, fixture.diff_requirement()?, CONSTRAINT)?;
    Ok((fixture, goal))
}

fn offer(fixture: &Fixture) -> TestResult<PlanStepOffer> {
    Ok(fixture
        .offers(1)?
        .pop()
        .ok_or("offer missing")?
        .with_planned_file_addition(PlannedFileAddition::new(fixture.path.clone()))?)
}

#[test]
fn explicit_add_selection_binds_the_operation_and_core_renders_its_title() -> TestResult {
    let (fixture, goal) = setup()?;
    let current = fixture.state(&goal, None)?;
    let phase = PreparedPlanPhase::new(current, vec![offer(&fixture)?], None)?;
    let ordinary = PreparedPlanPhase::new(current, fixture.offers(1)?, None)?;
    assert!(phase.addition_selection());
    assert!(!ordinary.addition_selection());
    assert_ne!(phase.job_digest(), ordinary.job_digest());
    let view: Value = serde_json::from_str(phase.request().messages()[1].content())?;
    assert_eq!(view["offers"][0]["operation"], "add");
    assert_eq!(view["offers"][0]["target"], "server.py");
    assert_eq!(view["constraints"], json!([CONSTRAINT]));
    let schema = phase
        .request()
        .structured_output()
        .ok_or("schema missing")?
        .value();
    assert_eq!(schema["properties"]["version"]["const"], 3);
    assert!(!schema.to_string().contains("outcome"));
    let reply = json!({"version":3,"job":phase.alias,"decision":"add"});
    let PlanPhaseOutcome::Steps(plan) = phase.complete(current, &serde_json::to_vec(&reply)?)?
    else {
        return Err("selection did not create a plan proposal".into());
    };
    let step = plan
        .ledger()
        .step(step_id(1))
        .ok_or("selected step missing")?
        .definition();
    assert_eq!(step.intended_outcome().as_str(), "Create server.py");
    assert_eq!(
        step.planned_file_addition()
            .map(PlannedFileAddition::target),
        Some(&fixture.path)
    );
    assert_eq!(step.addressed_criteria(), &[criterion_id(1)]);
    assert!(step.acceptance_criteria().is_empty());
    assert_eq!(step.verification_spec(), &fixture.diff_spec(1)?);
    Ok(())
}

#[test]
fn shared_envelope_keeps_all_decisions_available_in_every_field_order() -> TestResult {
    let (fixture, goal) = setup()?;
    let current = fixture.state(&goal, None)?;
    let phase = PreparedPlanPhase::new(current, vec![offer(&fixture)?], None)?;
    let schema = phase
        .request()
        .structured_output()
        .ok_or("schema missing")?
        .value();
    assert!(schema.get("oneOf").is_none());
    assert_eq!(schema["type"], "object");
    assert_eq!(schema["additionalProperties"], false);
    assert_eq!(schema["required"], json!(["decision", "job", "version"]));
    let properties = schema["properties"]
        .as_object()
        .ok_or("properties missing")?;
    assert_eq!(properties.len(), 3);
    assert!(properties.contains_key("decision"));
    assert!(properties.contains_key("job"));
    assert!(properties.contains_key("version"));
    assert_eq!(properties["job"]["const"], phase.alias);
    assert_eq!(properties["version"]["const"], 3);
    let decisions = ["add", "refuse", "clarify", "tooLarge"];
    assert_eq!(properties["decision"]["enum"], json!(decisions));
    // Every permitted decision shares the same required keys and constant bindings.
    // A property prefix cannot choose a distinct outcome object before its value.
    for decision in decisions {
        let fields = [
            r#""version":3"#.to_owned(),
            format!(r#""job":{}"#, serde_json::to_string(&phase.alias)?),
            format!(r#""decision":{}"#, serde_json::to_string(decision)?),
        ];
        for order in [
            [0, 1, 2],
            [0, 2, 1],
            [1, 0, 2],
            [1, 2, 0],
            [2, 0, 1],
            [2, 1, 0],
        ] {
            let body = order.map(|position| fields[position].as_str()).join(",");
            let reply = format!("{{{body}}}");
            let outcome = phase.complete(current, reply.as_bytes())?;
            assert!(matches!(
                (decision, outcome),
                ("add", PlanPhaseOutcome::Steps(_))
                    | ("refuse", PlanPhaseOutcome::Halted(PlanPhaseHalt::Refuse))
                    | ("clarify", PlanPhaseOutcome::Halted(PlanPhaseHalt::Clarify))
                    | ("tooLarge", PlanPhaseOutcome::TooLarge)
            ));
        }
    }
    Ok(())
}

#[test]
fn add_selection_rejects_missing_foreign_mixed_and_legacy_documents() -> TestResult {
    let (fixture, goal) = setup()?;
    let current = fixture.state(&goal, None)?;
    let phase = PreparedPlanPhase::new(current, vec![offer(&fixture)?], None)?;
    for reply in [
        json!({"version":3,"job":phase.alias}),
        json!({"job":phase.alias,"decision":"add"}),
        json!({"version":3,"decision":"add"}),
        json!({"version":3,"job":phase.alias,"decision":null}),
        json!({"version":3,"job":phase.alias,"decision":true}),
        json!({"version":null,"job":phase.alias,"decision":"add"}),
        json!({"version":3,"job":null,"decision":"add"}),
        json!({"version":3,"job":phase.alias,"decision":"P1"}),
        json!({"version":3,"job":phase.alias,"decision":"needSource"}),
        json!({"version":3,"job":phase.alias,"decision":"add","target":"other.py"}),
        json!({"version":3,"job":phase.alias,"decision":"add","add":"P1"}),
        json!({"version":3,"job":phase.alias,"decision":"refuse","halt":"refuse"}),
        json!({"version":3,"job":phase.alias,"decision":"tooLarge","tooLarge":true}),
        json!({"version":3,"job":"foreign","decision":"add"}),
        json!({"version":2,"job":phase.alias,"decision":"add"}),
        json!({"version":2,"job":phase.alias,"add":"P1"}),
        json!({"version":2,"job":phase.alias,"halt":"refuse"}),
        json!({"version":2,"job":phase.alias,"tooLarge":true}),
        json!({"version":1,"job":phase.alias,"steps":[{"offer":"P1","outcome":"Create server.py"}]}),
    ] {
        assert!(
            phase
                .complete(current, &serde_json::to_vec(&reply)?)
                .is_err()
        );
    }
    for duplicate in [
        format!(
            r#"{{"version":3,"job":"{}","decision":"add","decision":"add"}}"#,
            phase.alias
        ),
        format!(
            r#"{{"version":3,"job":"{}","decision":"add","decision":"tooLarge"}}"#,
            phase.alias
        ),
        format!(
            r#"{{"version":3,"job":"{}","job":"{}","decision":"add"}}"#,
            phase.alias, phase.alias
        ),
        format!(
            r#"{{"version":3,"version":3,"job":"{}","decision":"add"}}"#,
            phase.alias
        ),
    ] {
        assert!(matches!(
            phase.complete(current, duplicate.as_bytes()),
            Err(PlanPhaseFailure::InvalidReply)
        ));
    }
    Ok(())
}

#[test]
fn explicit_add_cannot_rebind_stale_or_existing_inventory_or_mismatched_scope() -> TestResult {
    let (fixture, goal) = setup()?;
    let current = fixture.state(&goal, None)?;
    let phase = PreparedPlanPhase::new(current, vec![offer(&fixture)?], None)?;
    let populated = index(
        fixture.published.run().snapshot_id(),
        vec![fixture.page.revision().clone()],
    )?;
    assert_eq!(populated.run(), fixture.published.run());
    let changed = PlanPhaseState::new(&fixture.project, &goal, &populated, &fixture.profile, None)?;
    let reply = json!({"version":3,"job":phase.alias,"decision":"add"});
    assert!(matches!(
        phase.complete(changed, &serde_json::to_vec(&reply)?),
        Err(PlanPhaseFailure::Stale)
    ));
    assert!(matches!(
        PreparedPlanPhase::new(changed, vec![offer(&fixture)?], None),
        Err(PlanPhaseFailure::InvalidOffer)
    ));
    assert!(
        fixture
            .offers(1)?
            .pop()
            .ok_or("offer missing")?
            .with_planned_file_addition(PlannedFileAddition::new(RepositoryPath::try_from_bytes(
                b"other.py".to_vec()
            )?))
            .is_err()
    );
    Ok(())
}
