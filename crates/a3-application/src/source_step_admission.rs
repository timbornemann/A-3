//! Original assignment and local verification admission for one Source step.

use a3_domain::{
    AcceptanceCriterionId, CriterionVerificationMismatch, GoalContract, GoalOriginalBindingError,
    TaskStepDefinition,
};
use std::error::Error;
use std::fmt;

/// Admits progress separately from a final original-criterion proof.
#[derive(Debug, Clone, Copy, Default)]
pub struct AdmitSourceTaskStep;

impl AdmitSourceTaskStep {
    /// Clean legacy goals retain their old path. A Source step needs original assignment
    /// and its own operational check; only Proves must match the global proof obligation.
    pub fn execute(
        self,
        goal: &GoalContract,
        step: &TaskStepDefinition,
    ) -> Result<(), SourceTaskStepAdmissionFailure> {
        goal.validate_original_binding()
            .map_err(SourceTaskStepAdmissionFailure::InvalidOriginalBinding)?;
        if goal.original().is_none() {
            return Ok(());
        }
        if step.addressed_criteria().is_empty() && step.acceptance_criteria().is_empty() {
            return Err(SourceTaskStepAdmissionFailure::MissingCriterionMapping);
        }
        if !step.verification_spec().is_operational() {
            return Err(SourceTaskStepAdmissionFailure::NonOperationalVerification);
        }
        for criterion_id in step
            .addressed_criteria()
            .iter()
            .chain(step.acceptance_criteria())
        {
            let criterion = goal
                .draft()
                .acceptance_criteria()
                .iter()
                .find(|criterion| criterion.id() == *criterion_id)
                .ok_or(SourceTaskStepAdmissionFailure::UnknownCriterion(
                    *criterion_id,
                ))?;
            let origin = criterion.origin().ok_or(
                SourceTaskStepAdmissionFailure::MissingCriterionOrigin(*criterion_id),
            )?;
            if step
                .acceptance_criteria()
                .binary_search(criterion_id)
                .is_ok()
            {
                origin
                    .evidence_requirement()
                    .validate_spec(step.verification_spec())
                    .map_err(|mismatch| SourceTaskStepAdmissionFailure::ProofMismatch {
                        criterion_id: *criterion_id,
                        mismatch,
                    })?;
            }
        }
        Ok(())
    }
}

/// Closed source-step admission reasons; no original, outcome or verification text is retained.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceTaskStepAdmissionFailure {
    /// The aggregate original binding is invalid.
    InvalidOriginalBinding(GoalOriginalBindingError),
    /// The step does not identify any original goal criterion.
    MissingCriterionMapping,
    /// A criterion does not belong to this immutable Goal.
    UnknownCriterion(AcceptanceCriterionId),
    /// A criterion has no retained original source range.
    MissingCriterionOrigin(AcceptanceCriterionId),
    /// The step cannot execute its own declared verification yet.
    NonOperationalVerification,
    /// A Proves role weakens or substitutes the original criterion's proof obligation.
    ProofMismatch {
        /// The criterion whose global obligation must remain unchanged.
        criterion_id: AcceptanceCriterionId,
        /// Closed proof mismatch reason without source text.
        mismatch: CriterionVerificationMismatch,
    },
}

impl fmt::Display for SourceTaskStepAdmissionFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidOriginalBinding(_) => "source step original binding is invalid",
            Self::MissingCriterionMapping => "source step has no original criterion assignment",
            Self::UnknownCriterion(_) => "source step criterion is outside its goal",
            Self::MissingCriterionOrigin(_) => "source step criterion has no original range",
            Self::NonOperationalVerification => "source step verification is not operational",
            Self::ProofMismatch { .. } => {
                "source step does not preserve the criterion proof obligation"
            }
        })
    }
}

impl Error for SourceTaskStepAdmissionFailure {}
