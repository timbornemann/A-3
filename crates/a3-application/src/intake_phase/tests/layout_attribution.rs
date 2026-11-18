use super::*;
use crate::{PrepareGoalFromIntakeReviewFailure, prepare_goal_from_intake_review};
use a3_domain::GoalIntakeError;

#[test]
fn sole_literal_finding_owns_ascii_layout_with_exact_raw_bytes() -> TestResult {
    let quote = "Rename Einf\u{fc}hrung.";
    let raw = format!(" \t{quote}\r\n\t ");
    let fixture = Fixture::new(&raw, 0, raw.len())?;
    let phase = fixture.prepare(None)?;
    assert_eq!(
        phase.request().messages()[1].content().as_bytes(),
        raw.as_bytes()
    );

    let value = reply(&phase, json!([{"kind":"must","quote":quote}]), json!([]));
    let batch = fixture.complete(&phase, value)?;
    let [finding] = batch.findings() else {
        return Err("expected one finding".into());
    };
    assert_eq!(finding.range(), fixture.window);
    assert_eq!(finding.statement(), quote);
    assert_eq!(
        fixture.original().resolve(finding.range())?.as_bytes(),
        raw.as_bytes()
    );

    let pending = fixture.stored.pending();
    let review = GoalIntakeReview::begin(pending, batch)?;
    let goal = prepare_goal_from_intake_review(pending, &review)?;
    assert_eq!(
        goal.original().ok_or("goal original")?.as_bytes(),
        raw.as_bytes()
    );

    let question_quote = "Einf\u{fc}hrung";
    let question_start = raw.find(question_quote).ok_or("question quote")?;
    let question_range = fixture
        .original()
        .range(question_start, question_start + question_quote.len())?;
    let with_question = fixture.complete(
        &phase,
        reply(
            &phase,
            json!([{"kind":"must","quote":quote}]),
            json!([{"kind":"missingUserChoice","quote":question_quote,"text":"Which translation?"}]),
        ),
    )?;
    assert_eq!(with_question.findings()[0].range(), fixture.window);
    assert_eq!(with_question.questions()[0].ranges(), &[question_range]);
    Ok(())
}

#[test]
fn punctuation_unicode_layout_and_omitted_clauses_remain_unassigned() -> TestResult {
    let quote = "Rename docs.";
    for (prefix, tail) in [
        (" \t", "!\r\n"),
        (" \t", "\u{a0}"),
        ("Keep the existing parser. ", " Keep source files.\r\n"),
    ] {
        let raw = format!("{prefix}{quote}{tail}");
        let fixture = Fixture::new(&raw, 0, raw.len())?;
        let phase = fixture.prepare(None)?;
        let batch = fixture.complete(
            &phase,
            reply(&phase, json!([{"kind":"must","quote":quote}]), json!([])),
        )?;
        assert_eq!(
            batch.findings()[0].range(),
            fixture
                .original()
                .range(prefix.len(), prefix.len() + quote.len())?
        );
        let pending = fixture.stored.pending();
        let review = GoalIntakeReview::begin(pending, batch)?;
        assert!(review.is_fully_reviewed(pending)?);
        assert_eq!(
            prepare_goal_from_intake_review(pending, &review),
            Err(PrepareGoalFromIntakeReviewFailure::Promotion(
                GoalIntakeError::IntakeIncomplete
            ))
        );
    }
    Ok(())
}

#[test]
fn whitespace_only_or_multiple_findings_never_gain_layout_attribution() -> TestResult {
    let layout = " \t\r\n";
    let fixture = Fixture::new(layout, 0, layout.len())?;
    let phase = fixture.prepare(None)?;
    assert!(matches!(
        fixture.complete(
            &phase,
            reply(&phase, json!([{"kind":"must","quote":layout}]), json!([])),
        ),
        Err(IntakePhaseFailure::QuoteNotFound)
    ));
    let empty = fixture.complete(&phase, reply(&phase, json!([]), json!([])))?;
    assert!(empty.findings().is_empty());

    let first = "Rename docs.";
    let second = "Keep source files.";
    let raw = format!("{layout}{first}\r\n{second}\r\n");
    let fixture = Fixture::new(&raw, 0, raw.len())?;
    let phase = fixture.prepare(None)?;
    let batch = fixture.complete(
        &phase,
        reply(
            &phase,
            json!([{"kind":"must","quote":first},{"kind":"constraint","quote":second}]),
            json!([]),
        ),
    )?;
    let first_start = layout.len();
    let second_start = first_start + first.len() + "\r\n".len();
    assert_eq!(
        batch.findings()[0].range(),
        fixture
            .original()
            .range(first_start, first_start + first.len())?
    );
    assert_eq!(
        batch.findings()[1].range(),
        fixture
            .original()
            .range(second_start, second_start + second.len())?
    );
    Ok(())
}

#[test]
fn existing_proof_remains_exact_during_initial_and_repair_layout_attribution() -> TestResult {
    let raw = format!(" \t{DOC}\r\n");
    let fixture = Fixture::new(&raw, 0, raw.len())?;
    let quoted = fixture.original().range(2, 2 + DOC.len())?;
    let proof = CriterionEvidenceRequirement::ChangeOnly(DiffInvariantVerification::new(
        DiffInvariantMode::ExactPaths,
        vec![RepositoryPath::try_from_bytes(b"README.md".to_vec())?],
    )?);

    for expected_range in [quoted, fixture.window] {
        let phase = fixture.prepare(Some((expected_range, proof.clone())))?;
        let repair = phase.for_structural_repair()?;
        assert_eq!(phase.anchor(), repair.anchor());
        assert_eq!(
            phase.request().messages()[1],
            repair.request().messages()[1]
        );
        for attempt in [&phase, &repair] {
            let batch = fixture.complete(
                attempt,
                reply(attempt, json!([{"kind":"must","quote":DOC}]), json!([])),
            )?;
            assert_eq!(batch.findings()[0].range(), expected_range);
            assert_eq!(
                batch.findings()[0].kind(),
                &GoalIntakeFindingKind::Outcome {
                    requirement: AcceptanceCriterionRequirement::Must,
                    evidence_requirement: proof.clone(),
                },
            );
        }
    }
    Ok(())
}
