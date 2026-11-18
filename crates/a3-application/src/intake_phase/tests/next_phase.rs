use super::*;
use crate::{
    ModelOperationControl, ModelProvider, ModelProviderFailure, ModelProviderFuture,
    ModelRequestTimeout, ModelWireProjection, ModelWireThinking, PhaseWireBudget,
    PhaseWireBudgetError, PrepareNextIntakePhaseFailure as Failure, prepare_next_intake_phase,
};
use std::sync::atomic::{AtomicUsize, Ordering};

#[test]
fn intake_shrinks_actual_wire_to_natural_window_and_resumes_after_commit() -> TestResult {
    let paragraph = format!("{DOC}\r\n\r\n");
    let late = "Keep late project docs.";
    let text = format!("{paragraph}{late}\r\n{}", "plain continuation ".repeat(91));
    let fixture = Fixture::new(&text, 0, text.len())?;
    let projector = WireProjector::new(fixture.profile.provider_id().clone());
    let full = fixture.prepare(None)?;
    assert_eq!(
        PhaseWireBudget::fit(
            projector.project_wire_request(full.request())?,
            &fixture.profile
        ),
        Err(PhaseWireBudgetError::Capacity)
    );
    projector.projections.store(0, Ordering::Relaxed);

    let proof_range = fixture
        .original()
        .range(paragraph.len(), paragraph.len() + late.len())?;
    let proof = CriterionEvidenceRequirement::ChangeOnly(DiffInvariantVerification::new(
        DiffInvariantMode::ExactPaths,
        vec![RepositoryPath::try_from_bytes(b"README.md".to_vec())?],
    )?);
    let first = prepare(&fixture, &projector, None, Some((proof_range, &proof)))?
        .ok_or("no fitted first window")?;
    assert_eq!(first.request().messages()[1].content(), paragraph);
    assert!((2..=12).contains(&projector.projections.load(Ordering::Relaxed)));
    PhaseWireBudget::fit(
        projector.project_wire_request(first.request())?,
        &fixture.profile,
    )?;
    let first_batch = fixture.complete(
        &first,
        reply(&first, json!([{"kind":"must","quote":DOC}]), json!([])),
    )?;
    assert_eq!(
        first_batch.findings()[0].kind(),
        &GoalIntakeFindingKind::Outcome {
            requirement: AcceptanceCriterionRequirement::Must,
            evidence_requirement: CriterionEvidenceRequirement::RuntimeNeedsRecipe,
        },
    );
    let review = GoalIntakeReview::begin(fixture.stored.pending(), first_batch)?;

    let second = prepare(
        &fixture,
        &projector,
        Some(&review),
        Some((proof_range, &proof)),
    )?
    .ok_or("no fitted next window")?;
    assert_eq!(
        second.anchor().window().start_byte(),
        paragraph.len() as u32
    );
    assert_eq!(second.anchor().review_revision(), Some(review.revision()));
    assert_eq!(
        second.request().messages()[1].content(),
        format!("{late}\r\n")
    );
    let second_batch = second.complete(
        reply(&second, json!([{"kind":"must","quote":late}]), json!([]))
            .to_string()
            .as_bytes(),
        second.anchor(),
        &fixture.stored,
        Some(&review),
    )?;
    assert_eq!(second_batch.findings()[0].range(), proof_range);
    assert_eq!(
        second_batch.findings()[0].kind(),
        &GoalIntakeFindingKind::Outcome {
            requirement: AcceptanceCriterionRequirement::Must,
            evidence_requirement: proof,
        },
    );
    assert_eq!(projector.streams.load(Ordering::Relaxed), 0);
    Ok(())
}

#[test]
fn intake_fit_can_reach_the_smallest_utf8_window_or_halt() -> TestResult {
    let first_character = "\u{20ac}";
    let fixture = Fixture::new("\u{20ac}abcd", 0, first_character.len())?;
    let mut projector = WireProjector::new(fixture.profile.provider_id().clone());
    let smallest = fixture.prepare(None)?;
    let overhead = projector
        .project_wire_request(smallest.request())?
        .request_bytes();
    projector.padding_bytes =
        usize::try_from(2688_u32.checked_sub(overhead).ok_or("test ceiling")?)?;

    let fitting = prepare(&fixture, &projector, None, None)?.ok_or("no UTF-8 window")?;
    assert_eq!(fitting.request().messages()[1].content(), first_character);
    assert_eq!(
        fitting.anchor().window().end_byte(),
        first_character.len() as u32
    );
    projector.padding_bytes += 1;
    assert!(matches!(
        prepare(&fixture, &projector, None, None),
        Err(Failure::NoFit)
    ));
    assert_eq!(projector.streams.load(Ordering::Relaxed), 0);
    Ok(())
}

#[test]
fn intake_only_shrinks_after_capacity_and_never_projects_when_review_is_complete() -> TestResult {
    let fixture = Fixture::new(DOC, 0, DOC.len())?;
    let mut projector = WireProjector::new(fixture.profile.provider_id().clone());
    projector.failure = Some(ModelProviderFailure::Unavailable);
    assert!(matches!(
        prepare(&fixture, &projector, None, None),
        Err(Failure::Provider(ModelProviderFailure::Unavailable))
    ));
    assert_eq!(projector.projections.load(Ordering::Relaxed), 1);

    projector.failure = None;
    projector.thinking = ModelWireThinking::Enabled;
    assert!(matches!(
        prepare(&fixture, &projector, None, None),
        Err(Failure::WireBudget(
            PhaseWireBudgetError::UnverifiedContract
        ))
    ));
    assert_eq!(projector.projections.load(Ordering::Relaxed), 2);

    let review = GoalIntakeReview::begin(
        fixture.stored.pending(),
        GoalIntakeReviewBatchProposal::new(fixture.window, Vec::new(), Vec::new())?,
    )?;
    assert!(prepare(&fixture, &projector, Some(&review), None)?.is_none());
    assert_eq!(projector.projections.load(Ordering::Relaxed), 2);
    assert_eq!(projector.streams.load(Ordering::Relaxed), 0);
    Ok(())
}

fn prepare(
    fixture: &Fixture,
    provider: &WireProjector,
    review: Option<&GoalIntakeReview>,
    proof: Option<(GoalOriginalRange, &CriterionEvidenceRequirement)>,
) -> Result<Option<PreparedIntakePhase>, Failure> {
    prepare_next_intake_phase(
        &fixture.project,
        &fixture.stored,
        review,
        &fixture.profile,
        provider,
        proof,
    )
}

#[derive(Debug)]
struct WireProjector {
    id: ModelProviderId,
    padding_bytes: usize,
    failure: Option<ModelProviderFailure>,
    thinking: ModelWireThinking,
    projections: AtomicUsize,
    streams: AtomicUsize,
}

impl WireProjector {
    fn new(id: ModelProviderId) -> Self {
        Self {
            id,
            padding_bytes: 0,
            failure: None,
            thinking: ModelWireThinking::Disabled,
            projections: AtomicUsize::new(0),
            streams: AtomicUsize::new(0),
        }
    }
}

impl ModelProvider for WireProjector {
    fn provider_id(&self) -> &ModelProviderId {
        &self.id
    }

    fn project_wire_request(
        &self,
        request: &ModelProviderRequest,
    ) -> Result<ModelWireProjection, ModelProviderFailure> {
        self.projections.fetch_add(1, Ordering::Relaxed);
        if let Some(failure) = self.failure {
            return Err(failure);
        }
        let messages = request
            .messages()
            .iter()
            .map(ModelMessage::content)
            .collect::<Vec<_>>();
        let schema = request
            .structured_output()
            .ok_or(ModelProviderFailure::Rejected)?
            .value();
        let body = serde_json::to_vec(&json!({
            "messages":messages,
            "format":schema,
            "padding":"x".repeat(self.padding_bytes),
        }))
        .map_err(|_| ModelProviderFailure::Rejected)?;
        let settings = request.profile().settings();
        ModelWireProjection::from_serialized(
            &body,
            messages.iter().map(|message| message.len()).sum(),
            schema.to_string().len(),
            settings.context_limit().get(),
            settings.output_limit().get(),
            self.thinking,
        )
    }

    fn stream<'a>(
        &'a self,
        _request: &'a ModelProviderRequest,
        _timeout: ModelRequestTimeout,
        _control: &'a dyn ModelOperationControl,
    ) -> ModelProviderFuture<'a> {
        self.streams.fetch_add(1, Ordering::Relaxed);
        Box::pin(std::future::ready(Err(ModelProviderFailure::Rejected)))
    }

    fn stream_projected<'a>(
        &'a self,
        request: &'a ModelProviderRequest,
        _projection: ModelWireProjection,
        timeout: ModelRequestTimeout,
        control: &'a dyn ModelOperationControl,
    ) -> ModelProviderFuture<'a> {
        self.stream(request, timeout, control)
    }
}
