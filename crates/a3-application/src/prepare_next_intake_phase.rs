//! Chooses a current original window that can fit the exact Intake wire.

use crate::{
    IntakePhaseAnchor, IntakePhaseAnchorFailure, IntakePhaseFailure, IntakeWindowByteLimit,
    IntakeWindowSelectionFailure, ModelProvider, ModelProviderFailure, PhaseWireBudget,
    PhaseWireBudgetError, PreparedIntakePhase, StoredPendingGoalIntake, select_intake_window,
};
use a3_domain::{
    CriterionEvidenceRequirement, GoalIntakeReview, GoalOriginalRange,
    MAX_GOAL_ORIGINAL_PART_BYTES, ModelProfile, ProjectIdentity,
};
use std::{error::Error, fmt};

/// Prepares the first unreviewed original range that fits the provider's checked wire.
///
/// Capacity rejection permits a bounded search with smaller source ceilings. Each
/// admitted range must independently fit the actual serialization, and bytes past
/// the chosen range remain unreviewed. The optional Core proof belongs only to
/// its exact original range; a partial or later range cannot inherit it.
///
/// Returns `None` when every byte has already been reviewed. This prepares an
/// initial request without delivery or reservation. The executor rechecks the
/// wire and the durable store still rejects overlaps with any prior attempt.
pub fn prepare_next_intake_phase(
    project: &ProjectIdentity,
    stored: &StoredPendingGoalIntake,
    review: Option<&GoalIntakeReview>,
    profile: &ModelProfile,
    provider: &dyn ModelProvider,
    proof: Option<(GoalOriginalRange, &CriterionEvidenceRequirement)>,
) -> Result<Option<PreparedIntakePhase>, PrepareNextIntakePhaseFailure> {
    use PrepareNextIntakePhaseFailure as Failure;

    if provider.provider_id() != profile.provider_id() {
        return Err(Failure::ProviderMismatch);
    }
    if let Some((range, _)) = proof
        && stored.pending().original().resolve(range).is_err()
    {
        return Err(Failure::Phase(IntakePhaseFailure::InvalidProofRange));
    }

    let revision = review.map(GoalIntakeReview::revision);
    let mut lower = 1;
    let mut upper = MAX_GOAL_ORIGINAL_PART_BYTES;
    let mut ceiling = upper;
    let mut best: Option<PreparedIntakePhase> = None;
    loop {
        let limit = IntakeWindowByteLimit::new(ceiling).map_err(Failure::Selection)?;
        match select_intake_window(stored.pending(), review, limit) {
            Ok(None) => return Ok(None),
            Err(IntakeWindowSelectionFailure::WindowTooSmall) => lower = ceiling + 1,
            Err(error) => return Err(Failure::Selection(error)),
            Ok(Some(window))
                if best
                    .as_ref()
                    .is_some_and(|phase| phase.anchor().window() == window) =>
            {
                lower = ceiling + 1;
            }
            Ok(Some(window)) => {
                let anchor =
                    IntakePhaseAnchor::new(project, stored, revision, window, profile.reference())
                        .map_err(Failure::Anchor)?;
                let contained_proof = proof
                    .filter(|(range, _)| {
                        range.start_byte() >= window.start_byte()
                            && range.end_byte() <= window.end_byte()
                    })
                    .map(|(range, requirement)| (range, requirement.clone()));
                let phase = PreparedIntakePhase::new(
                    anchor,
                    stored,
                    review,
                    profile.clone(),
                    contained_proof,
                )
                .map_err(Failure::Phase)?;
                let projection = provider
                    .project_wire_request(phase.request())
                    .map_err(Failure::Provider)?;
                match PhaseWireBudget::fit(projection, phase.request().profile()) {
                    Ok(_) => {
                        best = Some(phase);
                        lower = ceiling + 1;
                    }
                    Err(PhaseWireBudgetError::Capacity) => {
                        upper = (window.end_byte() - window.start_byte()) as usize - 1;
                    }
                    Err(error) => return Err(Failure::WireBudget(error)),
                }
            }
        }
        if lower > upper {
            return best.map(Some).ok_or(Failure::NoFit);
        }
        ceiling = lower + (upper - lower) / 2;
    }
}

/// Closed preparation failures without original text, model output or encoded payloads.
#[derive(Debug)]
pub enum PrepareNextIntakePhaseFailure {
    /// The actual provider identity does not belong to the pinned profile.
    ProviderMismatch,
    /// The bounded search found no admissible nonempty original window.
    NoFit,
    /// The bound original or its committed review does not allow selection.
    Selection(IntakeWindowSelectionFailure),
    /// A selected range cannot anchor a job against the current capture.
    Anchor(IntakePhaseAnchorFailure),
    /// Intake request validation rejected the selected source or proof.
    Phase(IntakePhaseFailure),
    /// The provider could not quote the exact request.
    Provider(ModelProviderFailure),
    /// The projected wire violates the verified profile contract.
    WireBudget(PhaseWireBudgetError),
}

impl fmt::Display for PrepareNextIntakePhaseFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ProviderMismatch => f.write_str("intake provider does not match the profile"),
            Self::NoFit => f.write_str("no selected intake window fits the model wire budget"),
            Self::Selection(error) => write!(f, "cannot select intake window: {error}"),
            Self::Anchor(error) => write!(f, "cannot anchor intake window: {error}"),
            Self::Phase(error) => write!(f, "cannot prepare intake request: {error}"),
            Self::Provider(error) => write!(f, "cannot project intake wire: {error}"),
            Self::WireBudget(error) => write!(f, "intake wire contract is invalid: {error}"),
        }
    }
}

impl Error for PrepareNextIntakePhaseFailure {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Selection(error) => Some(error),
            Self::Anchor(error) => Some(error),
            Self::Phase(error) => Some(error),
            Self::Provider(error) => Some(error),
            Self::WireBudget(error) => Some(error),
            Self::ProviderMismatch | Self::NoFit => None,
        }
    }
}
