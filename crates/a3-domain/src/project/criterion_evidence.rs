use super::{
    DiffInvariantVerification, DiscoveredCommandId, GoalOriginalRange, LoopbackHttpRecipe,
    MinimumTestCaseCount, PythonIntegerTranslation, TestCaseSelector, VerificationScope,
    VerificationSpec, VerificationTarget,
};
use std::error::Error;
use std::fmt;

/// Immutable original-request reference and required proof owned by one criterion.
///
/// The Goal aggregate must additionally validate this range against its own original bytes.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CriterionOrigin {
    original_range: GoalOriginalRange,
    evidence_requirement: CriterionEvidenceRequirement,
}

impl CriterionOrigin {
    /// Binds a validated original range to a closed proof requirement, not to a success claim.
    #[must_use]
    pub const fn new(
        original_range: GoalOriginalRange,
        evidence_requirement: CriterionEvidenceRequirement,
    ) -> Self {
        Self {
            original_range,
            evidence_requirement,
        }
    }

    /// Returns the exact original, goal revision and UTF-8 byte-range binding.
    #[must_use]
    pub const fn original_range(&self) -> GoalOriginalRange {
        self.original_range
    }

    /// Returns the proof obligation that later plans may not weaken.
    #[must_use]
    pub const fn evidence_requirement(&self) -> &CriterionEvidenceRequirement {
        &self.evidence_requirement
    }
}

/// Closed minimum proof required by an original-bound criterion.
///
/// This type does not prove that natural-language intake selected the right requirement.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum CriterionEvidenceRequirement {
    /// A particular existing test command must report matching executed passing cases.
    ExistingTest {
        /// Stable discovered command that is relevant to the criterion.
        command: DiscoveredCommandId,
        /// Exact case semantics that cannot be changed by replanning.
        selector: TestCaseSelector,
        /// At least this many selected cases must execute and pass.
        minimum_cases: MinimumTestCaseCount,
        /// Minimum declared breadth; a narrower replacement is not sufficient.
        scope: VerificationScope,
        /// Closed original-bound source obligation in addition to the real test.
        python_integer_translation: Option<PythonIntegerTranslation>,
    },
    /// Only this declared change relation is required; no runtime effect is asserted.
    ChangeOnly(DiffInvariantVerification),
    /// One exact local service lifecycle must prove the original HTTP expectations.
    LoopbackHttp(LoopbackHttpRecipe),
    /// A runtime outcome needs an operational recipe that this slice does not yet implement.
    RuntimeNeedsRecipe,
}

impl CriterionEvidenceRequirement {
    /// Rejects specifications that weaken or replace this criterion's required proof.
    ///
    /// Evidence semantics, freshness and ownership still require the existing verifiers.
    pub fn validate_spec(
        &self,
        spec: &VerificationSpec,
    ) -> Result<(), CriterionVerificationMismatch> {
        match self {
            Self::ExistingTest {
                command,
                selector,
                minimum_cases,
                scope,
                python_integer_translation,
            } => {
                let VerificationTarget::Test {
                    command_id: actual_command,
                    selector: actual_selector,
                    minimum_cases: actual_minimum,
                    scope: actual_scope,
                } = spec.target()
                else {
                    return Err(CriterionVerificationMismatch::EvidenceKindMismatch);
                };
                if actual_command != command {
                    return Err(CriterionVerificationMismatch::CommandMismatch);
                }
                if actual_selector != selector {
                    return Err(CriterionVerificationMismatch::SelectorMismatch);
                }
                if actual_minimum < minimum_cases {
                    return Err(CriterionVerificationMismatch::MinimumCasesTooSmall);
                }
                if actual_scope < scope {
                    return Err(CriterionVerificationMismatch::ScopeTooNarrow);
                }
                if spec.python_integer_translation() != python_integer_translation.as_ref() {
                    return Err(CriterionVerificationMismatch::SourceRequirementMismatch);
                }
                Ok(())
            }
            Self::ChangeOnly(required) => match spec.target() {
                VerificationTarget::DiffInvariant(actual) if actual == required => Ok(()),
                VerificationTarget::DiffInvariant(_) => {
                    Err(CriterionVerificationMismatch::DiffInvariantMismatch)
                }
                _ => Err(CriterionVerificationMismatch::EvidenceKindMismatch),
            },
            Self::LoopbackHttp(required) => match spec.target() {
                VerificationTarget::LoopbackHttp(actual) if actual == required => Ok(()),
                VerificationTarget::LoopbackHttp(_) => {
                    Err(CriterionVerificationMismatch::LoopbackHttpMismatch)
                }
                _ => Err(CriterionVerificationMismatch::EvidenceKindMismatch),
            },
            Self::RuntimeNeedsRecipe => {
                Err(CriterionVerificationMismatch::RuntimeRecipeUnavailable)
            }
        }
    }
}

/// A proposed step specification does not retain its criterion's immutable proof obligation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CriterionVerificationMismatch {
    /// The specification uses a different proof kind, including deferred or legacy methods.
    EvidenceKindMismatch,
    /// Another discovered command cannot substitute for the required runtime test.
    CommandMismatch,
    /// Another selected case set cannot substitute for the declared test semantics.
    SelectorMismatch,
    /// The replacement requires fewer executed passing cases.
    MinimumCasesTooSmall,
    /// The replacement declares less breadth than the original proof obligation.
    ScopeTooNarrow,
    /// The exact additional source-proof obligation did not survive into the step.
    SourceRequirementMismatch,
    /// The changed-path relation differs from the exact original obligation.
    DiffInvariantMismatch,
    /// The exact original HTTP expectations were changed.
    LoopbackHttpMismatch,
    /// The required operational runtime proof is unavailable and must remain open.
    RuntimeRecipeUnavailable,
}

impl fmt::Display for CriterionVerificationMismatch {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::EvidenceKindMismatch => "criterion requires another evidence kind",
            Self::CommandMismatch => "criterion requires another test command",
            Self::SelectorMismatch => "criterion requires another test selection",
            Self::MinimumCasesTooSmall => "criterion requires more executed passing test cases",
            Self::ScopeTooNarrow => "criterion requires a broader verification scope",
            Self::SourceRequirementMismatch => "criterion requires its exact source proof",
            Self::DiffInvariantMismatch => "criterion requires another change relation",
            Self::LoopbackHttpMismatch => "criterion requires its exact local HTTP recipe",
            Self::RuntimeRecipeUnavailable => {
                "criterion runtime verification recipe is unavailable"
            }
        })
    }
}

impl Error for CriterionVerificationMismatch {}
