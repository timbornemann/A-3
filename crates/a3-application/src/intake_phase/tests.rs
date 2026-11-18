mod next_phase;

use super::*;
use crate::GoalIntakeStoreVersion;
use a3_domain::{
    CanonicalDirectory, DiffInvariantMode, DiffInvariantVerification, GitHead, GitReferenceName,
    GoalContractReference, GoalContractTimestamp, ModelCapabilities, ModelContextLimit, ModelId,
    ModelOutputLimit, ModelParallelismLimit, ModelProfileSettings, ModelProviderId,
    ModelSamplingProfile, ModelStopSequences, ModelStructuredOutputCapability, ModelTemperature,
    ModelTokenCountingStrategy, ModelToolCallMode, ModelTopP, PendingGoalIntake, ProjectIdentity,
    RepositoryId, RepositoryIdentity, RepositoryPath, TaskId, WorktreeAnchorId, WorktreeId,
    WorktreeIdentity,
};
use serde_json::Value;

type TestResult<T = ()> = Result<T, Box<dyn Error>>;
const DOC: &str = "Rename the README heading to Guide.";

#[test]
fn intake_is_deterministic_and_only_exposes_the_exact_selected_user_source() -> TestResult {
    let prefix = "Outside the delivered window.\r\n";
    let source = "Rename Einführung.\r\nKeep λ unchanged.";
    let fixture = Fixture::new(&format!("{prefix}{source}"), prefix.len(), source.len())?;
    let first = fixture.prepare(None)?;
    let second = fixture.prepare(None)?;
    assert_eq!(first.anchor(), second.anchor());
    assert_eq!(first.request(), second.request());
    let messages = first.request().messages();
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].role(), ModelMessageRole::System);
    assert!(!messages[0].content().contains(source));
    assert!(!messages[0].content().contains(" Schema: "));
    assert_eq!(messages[1].role(), ModelMessageRole::User);
    assert_eq!(messages[1].content(), source);
    let schema = first.request().structured_output().ok_or("no schema")?;
    assert_eq!(schema.value()["additionalProperties"], false);
    assert_eq!(schema.value()["properties"]["window"]["const"], first.alias);
    for (name, bound) in [("findings", 4), ("questions", 1)] {
        assert_eq!(schema.value()["properties"][name]["maxItems"], bound);
        assert_eq!(
            schema.value()["properties"][name]["items"]["additionalProperties"],
            false
        );
    }
    let repeat = profile(ModelPromptSchemaGrounding::RepeatSchemaInPrompt)?;
    let anchor = fixture.anchor_for(&repeat, None)?;
    let repeated = PreparedIntakePhase::new(anchor, &fixture.stored, None, repeat, None)?;
    let schema = repeated
        .request()
        .structured_output()
        .ok_or("no repeated schema")?;
    assert!(
        repeated.request().messages()[0]
            .content()
            .ends_with(&schema.value().to_string())
    );
    assert_eq!(repeated.request().messages()[1].content(), source);
    Ok(())
}

#[test]
fn intake_binds_literal_unicode_ranges_and_only_the_exact_core_proof() -> TestResult {
    let source = format!("{DOC}\r\nPrefer Grüße.\r\nKeep source files.\r\nExclude screenshots.");
    let fixture = Fixture::new(&source, 0, source.len())?;
    let proof_range = fixture.original().range(0, DOC.len())?;
    let proof = CriterionEvidenceRequirement::ChangeOnly(DiffInvariantVerification::new(
        DiffInvariantMode::ExactPaths,
        vec![RepositoryPath::try_from_bytes(b"README.md".to_vec())?],
    )?);
    let phase = fixture.prepare(Some((proof_range, proof.clone())))?;
    let findings = json!([
        {"kind":"must","quote":DOC},
        {"kind":"should","quote":"Prefer Grüße."},
        {"kind":"constraint","quote":"Keep source files."},
        {"kind":"nonGoal","quote":"Exclude screenshots."},
    ]);
    let batch = fixture.complete(&phase, reply(&phase, findings, json!([])))?;
    let [must, should, constraint, non_goal] = batch.findings() else {
        return Err("wrong findings".into());
    };
    assert_eq!(must.range(), proof_range);
    assert_eq!(
        must.kind(),
        &GoalIntakeFindingKind::Outcome {
            requirement: AcceptanceCriterionRequirement::Must,
            evidence_requirement: proof,
        }
    );
    assert_eq!(
        should.kind(),
        &GoalIntakeFindingKind::Outcome {
            requirement: AcceptanceCriterionRequirement::Should,
            evidence_requirement: CriterionEvidenceRequirement::RuntimeNeedsRecipe,
        }
    );
    assert_eq!(fixture.original().resolve(should.range())?, "Prefer Grüße.");
    assert_eq!(constraint.kind(), &GoalIntakeFindingKind::Constraint);
    assert_eq!(non_goal.kind(), &GoalIntakeFindingKind::NonGoal);
    let review = GoalIntakeReview::begin(fixture.stored.pending(), batch)?;
    assert_eq!(review.findings().count(), 4);
    Ok(())
}

#[test]
fn intake_preserves_a_choice_and_a_source_bound_open_question() -> TestResult {
    let source = "Use the existing README. Select a new title.";
    let fixture = Fixture::new(source, 0, source.len())?;
    let phase = fixture.prepare(None)?;
    let batch = fixture.complete(&phase, reply(&phase,
        json!([{"kind":"decision","quote":"Use the existing README."}]),
        json!([{"kind":"missingUserChoice","quote":"Select a new title.","text":"Which title should the README use?"}]),
    ))?;
    assert_eq!(
        batch.findings()[0].kind(),
        &GoalIntakeFindingKind::ExistingUserDecision
    );
    let review = GoalIntakeReview::begin(fixture.stored.pending(), batch)?;
    let question = review.questions().next().ok_or("missing open question")?;
    assert_eq!(question.id().get(), 1);
    assert_eq!(
        question.draft().kind(),
        GoalIntakeQuestionKind::MissingUserChoice
    );
    assert_eq!(
        fixture.original().resolve(question.draft().ranges()[0])?,
        "Select a new title."
    );
    Ok(())
}

#[test]
fn intake_rejects_untrusted_shapes_and_duplicate_quote_classifications() -> TestResult {
    let fixture = Fixture::new(DOC, 0, DOC.len())?;
    let phase = fixture.prepare(None)?;
    let exact = json!({"kind":"must","quote":DOC});
    let question = json!({"kind":"missingUserChoice","quote":DOC,"text":"Which title?"});
    let malformed = [
        json!([1, phase.alias, [], []]),
        reply(&phase, json!([["must", DOC]]), json!([])),
        reply(
            &phase,
            json!([]),
            json!([["missingUserChoice", DOC, "Which title?"]]),
        ),
        reply(
            &phase,
            json!([{"kind":{"must":null},"quote":DOC}]),
            json!([]),
        ),
        reply(
            &phase,
            json!([{"kind":"must","quote":DOC,"offset":0}]),
            json!([]),
        ),
        reply(
            &phase,
            json!([{"kind":"must","quote":DOC,"evidence":"diff"}]),
            json!([]),
        ),
        reply(&phase, json!([{"kind":"unknown","quote":DOC}]), json!([])),
        reply(&phase, Value::Array(vec![exact.clone(); 5]), json!([])),
        reply(&phase, json!([]), json!([question.clone(), question])),
        reply(&phase, json!([]), Value::Null),
        json!({"version":1,"window":phase.alias,"findings":[]}),
        json!({"version":2,"window":phase.alias,"findings":[],"questions":[]}),
    ];
    for invalid in malformed {
        assert!(matches!(
            fixture.complete(&phase, invalid),
            Err(IntakePhaseFailure::InvalidReply)
        ));
    }
    let repeated_key = format!(
        r#"{{"version":1,"version":1,"window":"{}","findings":[],"questions":[]}}"#,
        phase.alias
    );
    assert!(matches!(
        phase.complete(
            repeated_key.as_bytes(),
            phase.anchor(),
            &fixture.stored,
            None
        ),
        Err(IntakePhaseFailure::InvalidReply)
    ));
    let huge = vec![b' '; MAX_INTAKE_PHASE_REPLY_BYTES + 1];
    assert!(matches!(
        phase.complete(&huge, phase.anchor(), &fixture.stored, None),
        Err(IntakePhaseFailure::ReplyTooLarge)
    ));
    assert!(matches!(
        fixture.complete(
            &phase,
            json!({"version":1,"window":"Iother","findings":[],"questions":[]})
        ),
        Err(IntakePhaseFailure::WrongWindow)
    ));
    assert!(matches!(
        fixture.complete(
            &phase,
            reply(
                &phase,
                json!([exact,{"kind":"constraint","quote":DOC}]),
                json!([])
            )
        ),
        Err(IntakePhaseFailure::DuplicateQuote)
    ));
    Ok(())
}

#[test]
fn intake_quote_matching_is_window_local_exact_and_includes_overlaps() -> TestResult {
    let prefix = "Do not expose.\n";
    let source = "λλλ.\r\nChange README.";
    let fixture = Fixture::new(&format!("{prefix}{source}"), prefix.len(), source.len())?;
    let phase = fixture.prepare(None)?;
    for quote in ["Do not expose.", "λλλ.\nChange README.", " "] {
        let value = reply(&phase, json!([{"kind":"must","quote":quote}]), json!([]));
        assert!(matches!(
            fixture.complete(&phase, value),
            Err(IntakePhaseFailure::QuoteNotFound)
        ));
    }
    let overlapping = reply(&phase, json!([{"kind":"must","quote":"λλ"}]), json!([]));
    assert!(matches!(
        fixture.complete(&phase, overlapping),
        Err(IntakePhaseFailure::AmbiguousQuote)
    ));
    let exact = reply(&phase, json!([{"kind":"must","quote":source}]), json!([]));
    let batch = fixture.complete(&phase, exact)?;
    assert_eq!(batch.findings()[0].range(), fixture.window);
    assert_eq!(
        fixture.original().resolve(batch.findings()[0].range())?,
        source
    );
    Ok(())
}

#[test]
fn intake_rechecks_current_capture_and_review_and_refuses_secrets() -> TestResult {
    let fixture = Fixture::new(DOC, 0, DOC.len())?;
    let phase = fixture.prepare(None)?;
    let bytes = serde_json::to_vec(&reply(
        &phase,
        json!([{"kind":"must","quote":DOC}]),
        json!([]),
    ))?;
    let changed = StoredPendingGoalIntake::new(
        fixture.stored.pending().clone(),
        GoalIntakeStoreVersion::new(2)?,
    );
    assert!(matches!(
        phase.complete(&bytes, phase.anchor(), &changed, None),
        Err(IntakePhaseFailure::Stale)
    ));
    let batch = phase.complete(&bytes, phase.anchor(), &fixture.stored, None)?;
    let review = GoalIntakeReview::begin(fixture.stored.pending(), batch)?;
    let newer_anchor = fixture.anchor_for(&fixture.profile, Some(&review))?;
    assert!(matches!(
        phase.complete(&bytes, newer_anchor, &fixture.stored, Some(&review)),
        Err(IntakePhaseFailure::Stale)
    ));
    assert!(matches!(
        PreparedIntakePhase::new(
            newer_anchor,
            &fixture.stored,
            Some(&review),
            fixture.profile.clone(),
            None
        ),
        Err(IntakePhaseFailure::Review(
            GoalIntakeReviewError::WindowAlreadyReviewed
        ))
    ));
    let secret = "password=fixture-secret-value\n";
    let source = format!("{secret}{DOC}");
    let unsafe_fixture = Fixture::new(&source, secret.len(), DOC.len())?;
    assert!(matches!(
        PreparedIntakePhase::new(
            unsafe_fixture.anchor_for(&unsafe_fixture.profile, None)?,
            &unsafe_fixture.stored,
            None,
            unsafe_fixture.profile.clone(),
            None,
        ),
        Err(IntakePhaseFailure::UnsafeContent)
    ));
    let proof = (
        unsafe_fixture.window,
        CriterionEvidenceRequirement::RuntimeNeedsRecipe,
    );
    assert!(matches!(
        PreparedIntakePhase::new(
            phase.anchor(),
            &fixture.stored,
            None,
            fixture.profile.clone(),
            Some(proof),
        ),
        Err(IntakePhaseFailure::InvalidProofRange)
    ));
    let question = reply(
        &phase,
        json!([]),
        json!([{"kind":"missingUserChoice","quote":DOC,"text":"authorization: bearer fixture-secret-value"}]),
    );
    assert!(matches!(
        fixture.complete(&phase, question),
        Err(IntakePhaseFailure::UnsafeContent)
    ));
    Ok(())
}

fn reply(phase: &PreparedIntakePhase, findings: Value, questions: Value) -> Value {
    json!({"version":1,"window":phase.alias,"findings":findings,"questions":questions})
}

struct Fixture {
    project: ProjectIdentity,
    stored: StoredPendingGoalIntake,
    window: GoalOriginalRange,
    profile: ModelProfile,
}

impl Fixture {
    fn new(original: &str, start: usize, bytes: usize) -> TestResult<Self> {
        let original = GoalOriginalRequest::try_from_bytes(
            GoalContractReference::initial(TaskId::from_bytes([31; 32])),
            original.as_bytes().to_vec(),
        )?;
        let window = original.range(start, start + bytes)?;
        let pending =
            PendingGoalIntake::new(original, GoalContractTimestamp::from_unix_millis(1)?)?;
        let root =
            CanonicalDirectory::from_canonicalized(std::env::current_dir()?.canonicalize()?)?;
        let id = RepositoryId::from_bytes([32; 32]);
        let project = ProjectIdentity::new(
            RepositoryIdentity::new(id, root.clone(), None),
            WorktreeIdentity::new(
                WorktreeId::from_bytes([33; 32]),
                WorktreeAnchorId::from_bytes([34; 32]),
                id,
                root,
            ),
            GitHead::Unborn {
                reference: GitReferenceName::try_from_full_name("refs/heads/main")?,
            },
        )?;
        Ok(Self {
            project,
            stored: StoredPendingGoalIntake::new(pending, GoalIntakeStoreVersion::INITIAL),
            window,
            profile: profile(ModelPromptSchemaGrounding::FormatFieldOnly)?,
        })
    }

    fn original(&self) -> &GoalOriginalRequest {
        self.stored.pending().original()
    }

    fn anchor_for(
        &self,
        profile: &ModelProfile,
        review: Option<&GoalIntakeReview>,
    ) -> TestResult<IntakePhaseAnchor> {
        Ok(IntakePhaseAnchor::new(
            &self.project,
            &self.stored,
            review.map(GoalIntakeReview::revision),
            self.window,
            profile.reference(),
        )?)
    }

    fn prepare(
        &self,
        proof: Option<(GoalOriginalRange, CriterionEvidenceRequirement)>,
    ) -> TestResult<PreparedIntakePhase> {
        Ok(PreparedIntakePhase::new(
            self.anchor_for(&self.profile, None)?,
            &self.stored,
            None,
            self.profile.clone(),
            proof,
        )?)
    }

    fn complete(
        &self,
        phase: &PreparedIntakePhase,
        value: Value,
    ) -> Result<GoalIntakeReviewBatchProposal, IntakePhaseFailure> {
        phase.complete(
            value.to_string().as_bytes(),
            phase.anchor(),
            &self.stored,
            None,
        )
    }
}

fn profile(grounding: ModelPromptSchemaGrounding) -> TestResult<ModelProfile> {
    Ok(ModelProfile::from_probe(
        ModelProviderId::try_from_string("intake-fixture".to_owned())?,
        ModelId::try_from_string("not-invoked".to_owned())?,
        ModelProfileSettings::new(
            ModelContextLimit::new(4_096)?,
            ModelOutputLimit::new(1_024)?,
            ModelTokenCountingStrategy::ConservativeUtf8BytesV1,
            ModelParallelismLimit::new(1)?,
            ModelSamplingProfile::new(
                ModelTemperature::from_milli(0)?,
                ModelTopP::from_milli(1_000)?,
            ),
            ModelStopSequences::empty(),
            grounding,
        )?,
        ModelCapabilities::new(
            ModelStructuredOutputCapability::Verified,
            ModelToolCallMode::NativeProviderReported,
        ),
    ))
}

#[test]
fn intake_repair_preserves_the_source_and_schema_for_the_same_job() -> TestResult {
    let fixture = Fixture::new(DOC, 0, DOC.len())?;
    let phase = fixture.prepare(None)?;
    let repair = phase.for_structural_repair()?;
    assert_eq!(phase.kind(), EditPhaseCallKind::Initial);
    assert_eq!(repair.kind(), EditPhaseCallKind::StructuralRepair);
    assert_eq!(repair.anchor(), phase.anchor());
    assert_eq!(repair.request().profile(), phase.request().profile());
    assert_eq!(
        repair.request().structured_output(),
        phase.request().structured_output()
    );
    assert_eq!(
        &repair.request().messages()[1..],
        &phase.request().messages()[1..]
    );
    assert_ne!(
        repair.request().messages()[0],
        phase.request().messages()[0]
    );
    assert!(matches!(
        repair.for_structural_repair(),
        Err(IntakePhaseFailure::RepairAlreadySelected)
    ));
    Ok(())
}

mod layout_attribution;
