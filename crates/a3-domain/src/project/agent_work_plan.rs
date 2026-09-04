use std::error::Error;
use std::fmt;

/// Maximum number of independently verifiable steps materialized from one reviewed plan.
pub const MAX_AGENT_WORK_PLAN_STEPS: usize = 64;

/// Closed verification intent selected by the deterministic plan compiler.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentWorkPlanVerificationIntent {
    /// Verify one implementation or documentation change with a discovered project check.
    Change,
    /// Prefer a discovered test command for an explicit test-plan step.
    Test,
}

/// One bounded, ordered step before Core-owned Task Ledger identities are assigned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentWorkPlanStep {
    outcome: String,
    rationale: String,
    expected_evidence: String,
    verification_intent: AgentWorkPlanVerificationIntent,
}

impl AgentWorkPlanStep {
    /// Returns the concrete user-visible result this step must produce.
    #[must_use]
    pub fn outcome(&self) -> &str {
        &self.outcome
    }

    /// Returns why this step is part of the reviewed plan.
    #[must_use]
    pub fn rationale(&self) -> &str {
        &self.rationale
    }

    /// Returns the evidence expected before the step may complete.
    #[must_use]
    pub fn expected_evidence(&self) -> &str {
        &self.expected_evidence
    }

    /// Returns the closed verification intent resolved later against the current command catalog.
    #[must_use]
    pub const fn verification_intent(&self) -> AgentWorkPlanVerificationIntent {
        self.verification_intent
    }
}

/// Core-validated ordered work plan compiled from one immutable reviewed conversation plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentWorkPlan {
    steps: Vec<AgentWorkPlanStep>,
}

impl AgentWorkPlan {
    /// Compiles the two authoritative plan sections without accepting model-supplied identities.
    pub fn from_reviewed_markdown(markdown: &str) -> Result<Self, AgentWorkPlanError> {
        let changes = section_items(markdown, "Implementation Changes");
        let tests = section_items(markdown, "Test Plan");
        if changes.is_empty() {
            return Err(AgentWorkPlanError::MissingImplementationSteps);
        }
        if tests.is_empty() {
            return Err(AgentWorkPlanError::MissingTestSteps);
        }

        let mut steps = Vec::new();
        for outcome in changes {
            push_unique(
                &mut steps,
                AgentWorkPlanStep {
                    rationale: "Setzt einen abgegrenzten Teil der freigegebenen Planrevision um."
                        .to_owned(),
                    expected_evidence:
                        "Aktuelle Änderungsevidence und die eigene typisierte Verifikation dieses Schritts"
                            .to_owned(),
                    outcome,
                    verification_intent: AgentWorkPlanVerificationIntent::Change,
                },
            );
        }
        for outcome in tests {
            push_unique(
                &mut steps,
                AgentWorkPlanStep {
                    rationale:
                        "Prüft die zuvor umgesetzten Ergebnisse mit einem ausdrücklichen Testziel."
                            .to_owned(),
                    expected_evidence: "Aktuelles Testergebnis für das im Plan benannte Verhalten"
                        .to_owned(),
                    outcome,
                    verification_intent: AgentWorkPlanVerificationIntent::Test,
                },
            );
        }
        if steps.len() > MAX_AGENT_WORK_PLAN_STEPS {
            return Err(AgentWorkPlanError::TooManySteps(steps.len()));
        }
        Ok(Self { steps })
    }

    /// Returns every step in reviewed execution order.
    #[must_use]
    pub fn steps(&self) -> &[AgentWorkPlanStep] {
        &self.steps
    }
}

fn push_unique(steps: &mut Vec<AgentWorkPlanStep>, candidate: AgentWorkPlanStep) {
    let normalized = normalized_outcome(&candidate.outcome);
    if !steps
        .iter()
        .any(|step| normalized_outcome(&step.outcome) == normalized)
    {
        steps.push(candidate);
    }
}

fn normalized_outcome(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn section_items(markdown: &str, section: &str) -> Vec<String> {
    let mut inside = false;
    let mut current = None::<String>;
    let mut items = Vec::new();
    let mut paragraph = Vec::new();

    for line in markdown.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') {
            if inside {
                break;
            }
            inside = trimmed
                .trim_start_matches('#')
                .trim()
                .eq_ignore_ascii_case(section);
            continue;
        }
        if !inside {
            continue;
        }
        if let Some(item) = top_level_item(line) {
            if let Some(previous) = current.take() {
                push_item(&mut items, previous);
            }
            current = Some(item.to_owned());
            continue;
        }
        if let Some(current) = current.as_mut() {
            if !trimmed.is_empty() && is_indented(line) {
                current.push(' ');
                current.push_str(trimmed.trim_start_matches(['-', '*']).trim());
            }
        } else if !trimmed.is_empty() {
            paragraph.push(trimmed);
        }
    }
    if let Some(previous) = current {
        push_item(&mut items, previous);
    }
    if items.is_empty() && !paragraph.is_empty() {
        push_item(&mut items, paragraph.join(" "));
    }
    items
}

fn is_indented(line: &str) -> bool {
    line.starts_with("  ") || line.starts_with('\t')
}

fn top_level_item(line: &str) -> Option<&str> {
    if is_indented(line) {
        return None;
    }
    let trimmed = line.trim();
    let bullet = trimmed
        .strip_prefix("- ")
        .or_else(|| trimmed.strip_prefix("* "))
        .or_else(|| trimmed.strip_prefix("+ "))
        .or_else(|| ordered_list_item(trimmed))?;
    Some(
        bullet
            .strip_prefix("[ ] ")
            .or_else(|| bullet.strip_prefix("[x] "))
            .or_else(|| bullet.strip_prefix("[X] "))
            .unwrap_or(bullet)
            .trim(),
    )
    .filter(|value| !value.is_empty())
}

fn ordered_list_item(value: &str) -> Option<&str> {
    let marker_end = value.find(['.', ')'])?;
    let (marker, remainder) = value.split_at(marker_end);
    if marker.is_empty() || !marker.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    remainder.get(1..)?.strip_prefix(' ')
}

fn push_item(items: &mut Vec<String>, value: String) {
    let normalized = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if !normalized.is_empty() {
        items.push(normalized);
    }
}

/// Reviewed plan could not be represented as a bounded executable work plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentWorkPlanError {
    /// The required implementation section contained no material result.
    MissingImplementationSteps,
    /// The required test section contained no verification result.
    MissingTestSteps,
    /// The plan exceeded the fixed step ceiling.
    TooManySteps(usize),
}

impl fmt::Display for AgentWorkPlanError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingImplementationSteps => {
                formatter.write_str("reviewed Agent plan has no implementation steps")
            }
            Self::MissingTestSteps => formatter.write_str("reviewed Agent plan has no test steps"),
            Self::TooManySteps(count) => write!(
                formatter,
                "reviewed Agent plan has {count} steps; maximum is {MAX_AGENT_WORK_PLAN_STEPS}"
            ),
        }
    }
}

impl Error for AgentWorkPlanError {}

#[cfg(test)]
mod tests {
    use super::{AgentWorkPlan, AgentWorkPlanError, AgentWorkPlanVerificationIntent};

    #[test]
    fn compiles_atomic_change_and_test_steps_in_reviewed_order()
    -> Result<(), Box<dyn std::error::Error>> {
        let plan = AgentWorkPlan::from_reviewed_markdown(
            "## Summary\nAPI\n## Implementation Changes\n- Vertrag definieren\n  - Fehler typisieren\n- Adapter anbinden\n## Interfaces\nIPC\n## Test Plan\n- Vertragstest schreiben\n- Tests real ausführen\n## Assumptions\nAktueller Index",
        )?;
        assert_eq!(plan.steps().len(), 4);
        assert_eq!(
            plan.steps()[0].outcome(),
            "Vertrag definieren Fehler typisieren"
        );
        assert_eq!(plan.steps()[1].outcome(), "Adapter anbinden");
        assert_eq!(
            plan.steps()[2].verification_intent(),
            AgentWorkPlanVerificationIntent::Test
        );
        Ok(())
    }

    #[test]
    fn accepts_one_explicit_paragraph_but_rejects_an_empty_change_section() {
        let one = AgentWorkPlan::from_reviewed_markdown(
            "## Implementation Changes\nBestehende Dokumentation aktualisieren.\n## Test Plan\nLinks prüfen.",
        );
        assert!(one.is_ok_and(|plan| plan.steps().len() == 2));
        assert_eq!(
            AgentWorkPlan::from_reviewed_markdown(
                "## Implementation Changes\n\n## Test Plan\n- Test ausführen"
            ),
            Err(AgentWorkPlanError::MissingImplementationSteps)
        );
        assert_eq!(
            AgentWorkPlan::from_reviewed_markdown(
                "## Implementation Changes\n- Änderung umsetzen\n## Test Plan\n\n## Assumptions\nAktuell"
            ),
            Err(AgentWorkPlanError::MissingTestSteps)
        );
    }

    #[test]
    fn nested_bullets_explain_the_parent_instead_of_becoming_parallel_work()
    -> Result<(), Box<dyn std::error::Error>> {
        let plan = AgentWorkPlan::from_reviewed_markdown(
            "## Implementation Changes\n- API bauen\n  - Handler ergänzen\n  - Fehler abbilden\n## Test Plan\nTesten",
        )?;
        assert_eq!(
            plan.steps()[0].outcome(),
            "API bauen Handler ergänzen Fehler abbilden"
        );
        Ok(())
    }

    #[test]
    fn accepts_ordered_and_plus_markers_as_independent_steps()
    -> Result<(), Box<dyn std::error::Error>> {
        let plan = AgentWorkPlan::from_reviewed_markdown(
            "## Implementation Changes\n1. API-Vertrag definieren\n2) Adapter implementieren\n+ Dokumentation aktualisieren\n## Test Plan\n1. Vertragstest ausführen",
        )?;
        assert_eq!(plan.steps().len(), 4);
        assert_eq!(plan.steps()[1].outcome(), "Adapter implementieren");
        assert_eq!(plan.steps()[2].outcome(), "Dokumentation aktualisieren");
        Ok(())
    }

    #[test]
    fn rejects_more_than_the_fixed_number_of_atomic_steps() {
        let changes = (1..=64)
            .map(|number| format!("{number}. Änderung {number}"))
            .collect::<Vec<_>>()
            .join("\n");
        let plan =
            format!("## Implementation Changes\n{changes}\n## Test Plan\n- Gesamttest ausführen");
        assert_eq!(
            AgentWorkPlan::from_reviewed_markdown(&plan),
            Err(AgentWorkPlanError::TooManySteps(65))
        );
    }
}
