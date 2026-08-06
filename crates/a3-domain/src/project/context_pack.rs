use super::ModelProfile;
use std::error::Error;
use std::fmt;

const REFERENCE_CONTEXT_TOKENS: u32 = 16_384;
const SYSTEM_REFERENCE_TOKENS: u32 = 900;
const GOAL_LEDGER_REFERENCE_TOKENS: u32 = 900;
const PROJECT_MAP_REFERENCE_TOKENS: u32 = 1_200;
const CODE_EVIDENCE_REFERENCE_TOKENS: u32 = 7_000;
const TOOL_RESULTS_REFERENCE_TOKENS: u32 = 1_500;
const SAFETY_REFERENCE_TOKENS: u32 = 900;
const OUTPUT_REFERENCE_TOKENS: u32 = 3_500;
const OUTPUT_PERCENT_NUMERATOR: u64 = 22;
const OUTPUT_PERCENT_DENOMINATOR: u64 = 100;

/// Version of deterministic context budgeting, ordering, rendering, and digest semantics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ContextCompilerPolicyVersion(u32);

impl ContextCompilerPolicyVersion {
    /// Initial H7 Context Compiler policy.
    pub const V1: Self = Self(1);

    /// M6 policy with a compact untruncatable L0 anchor before ranked L1/L2 detail.
    pub const V2: Self = Self(2);

    /// Policy emitted by the current deterministic compiler implementation.
    pub const CURRENT: Self = Self::V2;

    /// Returns the stable persisted integer.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// Domain-separated digest of one normalized Context Pack and all governing versions.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ContextDigest([u8; 32]);

impl ContextDigest {
    /// Constructs a digest from a versioned compiler implementation.
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Returns the canonical binary representation.
    #[must_use]
    pub const fn as_bytes(self) -> [u8; 32] {
        self.0
    }
}

impl fmt::Display for ContextDigest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            write!(formatter, "{byte:02x}")?;
        }
        Ok(())
    }
}

impl fmt::Debug for ContextDigest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "ContextDigest({self})")
    }
}

/// Fixed Context Pack areas whose allowances are independently enforced.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ContextSection {
    /// Static system, tool-schema grounding, and security contract.
    SystemAndTools,
    /// Unabridged Goal Contract and current Task Ledger step.
    GoalAndLedger,
    /// Repository and module-level project-map material.
    ProjectMap,
    /// Symbol, file, span, claim, and other structured evidence.
    CodeAndEvidence,
    /// Current bounded read-only tool observations.
    ToolResults,
}

/// Proportionally scaled V1 allowances plus non-packable safety and output reserves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContextBudgetPlan {
    context_limit: u32,
    system_and_tools: u32,
    goal_and_ledger: u32,
    project_map: u32,
    code_and_evidence: u32,
    tool_results: u32,
    safety_reserve: u32,
    output_reserve: u32,
}

impl ContextBudgetPlan {
    /// Derives the exact V1 budget from a ModelProfile without tokenizer heuristics.
    pub fn for_profile(profile: &ModelProfile) -> Result<Self, ContextBudgetError> {
        let context_limit = profile.settings().context_limit().get();
        let output_reserve =
            scaled(OUTPUT_REFERENCE_TOKENS, context_limit).max(percentage_ceiling(
                context_limit,
                OUTPUT_PERCENT_NUMERATOR,
                OUTPUT_PERCENT_DENOMINATOR,
            )?);
        let supported_output = profile.settings().output_limit().get();
        if supported_output < output_reserve {
            return Err(ContextBudgetError::OutputCapabilityTooSmall {
                required: output_reserve,
                supported: supported_output,
            });
        }
        let plan = Self {
            context_limit,
            system_and_tools: scaled_non_zero(SYSTEM_REFERENCE_TOKENS, context_limit),
            goal_and_ledger: scaled_non_zero(GOAL_LEDGER_REFERENCE_TOKENS, context_limit),
            project_map: scaled_non_zero(PROJECT_MAP_REFERENCE_TOKENS, context_limit),
            code_and_evidence: scaled_non_zero(CODE_EVIDENCE_REFERENCE_TOKENS, context_limit),
            tool_results: scaled_non_zero(TOOL_RESULTS_REFERENCE_TOKENS, context_limit),
            safety_reserve: scaled_non_zero(SAFETY_REFERENCE_TOKENS, context_limit),
            output_reserve,
        };
        if plan.maximum_accounted_tokens()? > context_limit {
            return Err(ContextBudgetError::AllocationOverflow);
        }
        Ok(plan)
    }

    /// Returns the effective model context window.
    #[must_use]
    pub const fn context_limit(self) -> u32 {
        self.context_limit
    }

    /// Returns one independently enforced pack-area allowance.
    #[must_use]
    pub const fn allowance(self, section: ContextSection) -> u32 {
        match section {
            ContextSection::SystemAndTools => self.system_and_tools,
            ContextSection::GoalAndLedger => self.goal_and_ledger,
            ContextSection::ProjectMap => self.project_map,
            ContextSection::CodeAndEvidence => self.code_and_evidence,
            ContextSection::ToolResults => self.tool_results,
        }
    }

    /// Returns tokens that no context item may consume.
    #[must_use]
    pub const fn safety_reserve(self) -> u32 {
        self.safety_reserve
    }

    /// Returns the reserved provider response budget, always at least 22 percent.
    #[must_use]
    pub const fn output_reserve(self) -> u32 {
        self.output_reserve
    }

    /// Returns the sum of all area ceilings and mandatory reserves.
    pub fn maximum_accounted_tokens(self) -> Result<u32, ContextBudgetError> {
        [
            self.system_and_tools,
            self.goal_and_ledger,
            self.project_map,
            self.code_and_evidence,
            self.tool_results,
            self.safety_reserve,
            self.output_reserve,
        ]
        .into_iter()
        .try_fold(0_u32, |total, value| total.checked_add(value))
        .ok_or(ContextBudgetError::AllocationOverflow)
    }
}

/// Actual deterministic token usage of all packable areas.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContextBudgetUsage {
    system_and_tools: u32,
    goal_and_ledger: u32,
    project_map: u32,
    code_and_evidence: u32,
    tool_results: u32,
    prompt_total: u32,
}

impl ContextBudgetUsage {
    /// Validates each hard area ceiling and the total context equation.
    pub fn new(
        plan: ContextBudgetPlan,
        system_and_tools: u32,
        goal_and_ledger: u32,
        project_map: u32,
        code_and_evidence: u32,
        tool_results: u32,
    ) -> Result<Self, ContextBudgetError> {
        let values = [
            (ContextSection::SystemAndTools, system_and_tools),
            (ContextSection::GoalAndLedger, goal_and_ledger),
            (ContextSection::ProjectMap, project_map),
            (ContextSection::CodeAndEvidence, code_and_evidence),
            (ContextSection::ToolResults, tool_results),
        ];
        for (section, actual) in values {
            let allowance = plan.allowance(section);
            if actual > allowance {
                return Err(ContextBudgetError::SectionExceeded {
                    section,
                    actual,
                    allowance,
                });
            }
        }
        let prompt_total = values
            .into_iter()
            .map(|(_, value)| value)
            .try_fold(0_u32, |total, value| total.checked_add(value))
            .ok_or(ContextBudgetError::AllocationOverflow)?;
        let accounted = prompt_total
            .checked_add(plan.safety_reserve())
            .and_then(|value| value.checked_add(plan.output_reserve()))
            .ok_or(ContextBudgetError::AllocationOverflow)?;
        if accounted > plan.context_limit() {
            return Err(ContextBudgetError::ContextExceeded {
                actual: accounted,
                limit: plan.context_limit(),
            });
        }
        Ok(Self {
            system_and_tools,
            goal_and_ledger,
            project_map,
            code_and_evidence,
            tool_results,
            prompt_total,
        })
    }

    /// Returns actual usage for one packable area.
    #[must_use]
    pub const fn section(self, section: ContextSection) -> u32 {
        match section {
            ContextSection::SystemAndTools => self.system_and_tools,
            ContextSection::GoalAndLedger => self.goal_and_ledger,
            ContextSection::ProjectMap => self.project_map,
            ContextSection::CodeAndEvidence => self.code_and_evidence,
            ContextSection::ToolResults => self.tool_results,
        }
    }

    /// Returns total input tokens before safety and output reserves.
    #[must_use]
    pub const fn prompt_total(self) -> u32 {
        self.prompt_total
    }
}

/// Invalid Context Compiler budget or observed usage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextBudgetError {
    /// The configured model cannot emit the mandatory output reserve.
    OutputCapabilityTooSmall {
        /// Required V1 response tokens.
        required: u32,
        /// Configured provider output limit.
        supported: u32,
    },
    /// One independently budgeted area exceeded its hard ceiling.
    SectionExceeded {
        /// Rejected area.
        section: ContextSection,
        /// Actual deterministic cost.
        actual: u32,
        /// Effective scaled allowance.
        allowance: u32,
    },
    /// Prompt plus mandatory reserves exceeded the model context.
    ContextExceeded {
        /// Accounted total.
        actual: u32,
        /// Effective context limit.
        limit: u32,
    },
    /// Fixed-width budget arithmetic overflowed.
    AllocationOverflow,
}

impl fmt::Display for ContextBudgetError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OutputCapabilityTooSmall {
                required,
                supported,
            } => write!(
                formatter,
                "model output limit {supported} is below the required Context Pack reserve {required}"
            ),
            Self::SectionExceeded {
                section,
                actual,
                allowance,
            } => write!(
                formatter,
                "Context Pack section {section:?} uses {actual} tokens; allowance is {allowance}"
            ),
            Self::ContextExceeded { actual, limit } => write!(
                formatter,
                "Context Pack accounts for {actual} tokens; model context limit is {limit}"
            ),
            Self::AllocationOverflow => {
                formatter.write_str("Context Pack budget arithmetic overflowed")
            }
        }
    }
}

impl Error for ContextBudgetError {}

const fn scaled(reference: u32, context_limit: u32) -> u32 {
    ((reference as u64 * context_limit as u64) / REFERENCE_CONTEXT_TOKENS as u64) as u32
}

const fn scaled_non_zero(reference: u32, context_limit: u32) -> u32 {
    let value = scaled(reference, context_limit);
    if value == 0 { 1 } else { value }
}

fn percentage_ceiling(
    value: u32,
    numerator: u64,
    denominator: u64,
) -> Result<u32, ContextBudgetError> {
    let multiplied = u64::from(value)
        .checked_mul(numerator)
        .ok_or(ContextBudgetError::AllocationOverflow)?;
    let rounded = multiplied
        .checked_add(denominator - 1)
        .ok_or(ContextBudgetError::AllocationOverflow)?
        / denominator;
    u32::try_from(rounded).map_err(|_| ContextBudgetError::AllocationOverflow)
}

#[cfg(test)]
mod tests {
    use super::{
        ContextBudgetError, ContextBudgetPlan, ContextBudgetUsage, ContextCompilerPolicyVersion,
        ContextDigest, ContextSection,
    };
    use crate::{
        ModelCapabilities, ModelContextLimit, ModelId, ModelOutputLimit, ModelParallelismLimit,
        ModelProfile, ModelProfileSettings, ModelPromptSchemaGrounding, ModelProviderId,
        ModelSamplingProfile, ModelStopSequences, ModelStructuredOutputCapability,
        ModelTemperature, ModelTokenCountingStrategy, ModelToolCallMode, ModelTopP,
    };
    use std::error::Error;

    fn profile(context: u32, output: u32) -> Result<ModelProfile, Box<dyn Error>> {
        let settings = ModelProfileSettings::new(
            ModelContextLimit::new(context)?,
            ModelOutputLimit::new(output)?,
            ModelTokenCountingStrategy::ConservativeUtf8BytesV1,
            ModelParallelismLimit::new(1)?,
            ModelSamplingProfile::new(
                ModelTemperature::from_milli(0)?,
                ModelTopP::from_milli(1_000)?,
            ),
            ModelStopSequences::empty(),
            ModelPromptSchemaGrounding::FormatFieldOnly,
        )?;
        Ok(ModelProfile::from_probe(
            ModelProviderId::try_from_string("fixture".to_owned())?,
            ModelId::try_from_string("fixture-model".to_owned())?,
            settings,
            ModelCapabilities::new(
                ModelStructuredOutputCapability::Verified,
                ModelToolCallMode::Disabled,
            ),
        ))
    }

    #[test]
    fn sixteen_k_budget_matches_the_documented_v1_profile() -> Result<(), Box<dyn Error>> {
        assert_eq!(
            ContextCompilerPolicyVersion::CURRENT,
            ContextCompilerPolicyVersion::V2
        );
        let plan = ContextBudgetPlan::for_profile(&profile(16_384, 4_096)?)?;
        assert_eq!(plan.allowance(ContextSection::SystemAndTools), 900);
        assert_eq!(plan.allowance(ContextSection::GoalAndLedger), 900);
        assert_eq!(plan.allowance(ContextSection::ProjectMap), 1_200);
        assert_eq!(plan.allowance(ContextSection::CodeAndEvidence), 7_000);
        assert_eq!(plan.allowance(ContextSection::ToolResults), 1_500);
        assert_eq!(plan.safety_reserve(), 900);
        assert_eq!(plan.output_reserve(), 3_605);
        assert!(plan.maximum_accounted_tokens()? <= plan.context_limit());
        Ok(())
    }

    #[test]
    fn output_and_section_limits_are_hard_boundaries() -> Result<(), Box<dyn Error>> {
        assert!(matches!(
            ContextBudgetPlan::for_profile(&profile(16_384, 3_500)?),
            Err(ContextBudgetError::OutputCapabilityTooSmall { .. })
        ));
        let plan = ContextBudgetPlan::for_profile(&profile(16_384, 4_096)?)?;
        assert!(matches!(
            ContextBudgetUsage::new(plan, 901, 1, 1, 1, 1),
            Err(ContextBudgetError::SectionExceeded {
                section: ContextSection::SystemAndTools,
                ..
            })
        ));
        Ok(())
    }

    #[test]
    fn digest_has_stable_hex_form() {
        let digest = ContextDigest::from_bytes([0xab; 32]);
        assert_eq!(digest.to_string(), "ab".repeat(32));
        assert_eq!(digest.as_bytes(), [0xab; 32]);
    }
}
