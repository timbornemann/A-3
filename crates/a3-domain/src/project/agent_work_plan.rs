use std::error::Error;
use std::fmt;

/// Maximum number of independently verifiable steps materialized from one reviewed plan.
pub const MAX_AGENT_WORK_PLAN_STEPS: usize = 64;
const GREENFIELD_SLICE_MAX_BYTES: usize = 1_536;

/// Closed research result meaning that no separate operational check was requested or selected.
pub const NO_ADDITIONAL_AGENT_CHECKS: &str = "No additional checks are required.";

/// Closed verification intent selected by the deterministic plan compiler.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentWorkPlanVerificationIntent {
    /// Prove an implementation change; run an existing check when available.
    Change,
    /// Execute a reviewed operational check; never authorize test creation.
    Check,
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
        Self::from_items(changes, tests)
    }

    /// Compiles already admitted Core research decisions without treating their text as
    /// Markdown headings. Top-level list items remain independent executable steps; plain
    /// prose remains one bounded step for backwards compatibility.
    pub fn from_research_decisions(
        implementation: &str,
        tests: &str,
    ) -> Result<Self, AgentWorkPlanError> {
        Self::from_items(
            plan_items(implementation.lines()),
            plan_items(tests.lines()),
        )
    }

    /// Batches explicit implementation work without inventing tests or test scaffolding.
    /// Operational checks remain the original separate steps; their absence is valid.
    pub fn into_greenfield_execution(self) -> Result<Self, AgentWorkPlanError> {
        let (changes, tests): (Vec<_>, Vec<_>) = self
            .steps
            .into_iter()
            .partition(|step| step.verification_intent == AgentWorkPlanVerificationIntent::Change);
        let mut steps = batched_steps(
            changes.into_iter().map(|step| step.outcome),
            "Setze diesen freigegebenen Greenfield-Änderungsslice vollständig um:",
            "Setzt zusammengehörige Ergebnisse des freigegebenen Greenfield-Plans in einem begrenzten Patch um.",
            "Aktuelle Änderungsevidence für alle im Slice genannten Ergebnisse",
        );
        steps.extend(tests);
        if steps.len() > MAX_AGENT_WORK_PLAN_STEPS {
            return Err(AgentWorkPlanError::TooManySteps(steps.len()));
        }
        Ok(Self { steps })
    }

    fn from_items(changes: Vec<String>, tests: Vec<String>) -> Result<Self, AgentWorkPlanError> {
        if changes.is_empty() {
            return Err(AgentWorkPlanError::MissingImplementationSteps);
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
        for outcome in tests
            .into_iter()
            .filter(|item| item != NO_ADDITIONAL_AGENT_CHECKS)
        {
            push_unique(
                &mut steps,
                AgentWorkPlanStep {
                    rationale:
                        "Führt die im Plan ausgewählte vorhandene oder kurze Funktionsprüfung aus."
                            .to_owned(),
                    expected_evidence:
                        "Aktuelle operationale Evidence für die im Plan benannte Prüfung".to_owned(),
                    outcome,
                    verification_intent: AgentWorkPlanVerificationIntent::Check,
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

fn batched_steps(
    outcomes: impl IntoIterator<Item = String>,
    prefix: &str,
    rationale: &str,
    expected_evidence: &str,
) -> Vec<AgentWorkPlanStep> {
    let mut groups = Vec::<Vec<String>>::new();
    for outcome in outcomes {
        let numbered_len = outcome.len().saturating_add(8);
        let fits = groups.last().is_some_and(|group| {
            prefix
                .len()
                .saturating_add(1)
                .saturating_add(
                    group
                        .iter()
                        .map(|item| item.len().saturating_add(8))
                        .sum::<usize>(),
                )
                .saturating_add(numbered_len)
                <= GREENFIELD_SLICE_MAX_BYTES
        });
        if fits {
            if let Some(group) = groups.last_mut() {
                group.push(outcome);
            }
        } else {
            groups.push(vec![outcome]);
        }
    }
    groups
        .into_iter()
        .map(|group| AgentWorkPlanStep {
            outcome: format!(
                "{prefix} {}",
                group
                    .iter()
                    .enumerate()
                    .map(|(index, item)| format!("{}. {item}", index + 1))
                    .collect::<Vec<_>>()
                    .join(" ")
            ),
            rationale: rationale.to_owned(),
            expected_evidence: expected_evidence.to_owned(),
            verification_intent: AgentWorkPlanVerificationIntent::Change,
        })
        .collect()
}

fn push_unique(steps: &mut Vec<AgentWorkPlanStep>, candidate: AgentWorkPlanStep) {
    let normalized = normalized_outcome(&candidate.outcome);
    if !steps.iter().any(|step| {
        step.verification_intent == candidate.verification_intent
            && normalized_outcome(&step.outcome) == normalized
    }) {
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
    let mut lines = Vec::new();

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
        lines.push(line);
    }
    plan_items(lines)
}

fn plan_items<'a>(lines: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    let mut current = None::<String>;
    let mut items = Vec::new();
    let mut paragraph = Vec::new();
    for line in lines {
        let trimmed = line.trim();
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
    /// The plan exceeded the fixed step ceiling.
    TooManySteps(usize),
}

impl fmt::Display for AgentWorkPlanError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingImplementationSteps => {
                formatter.write_str("reviewed Agent plan has no implementation steps")
            }
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
            AgentWorkPlanVerificationIntent::Check
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
        assert!(AgentWorkPlan::from_reviewed_markdown(
            "## Implementation Changes\n- Änderung umsetzen\n## Test Plan\n\n## Assumptions\nAktuell"
        ).is_ok_and(|plan| plan.steps().len() == 1));
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
    fn admitted_research_lists_keep_atomic_steps_without_heading_authority()
    -> Result<(), Box<dyn std::error::Error>> {
        let plan = AgentWorkPlan::from_research_decisions(
            "1. Servermodul anlegen\n2. Fehlerantworten ergänzen\n## Test Plan\nUnerlaubte Überschrift",
            "- Start prüfen\n- GET und Fehlerfälle prüfen",
        )?;
        assert_eq!(plan.steps().len(), 4);
        assert_eq!(plan.steps()[0].outcome(), "Servermodul anlegen");
        assert_eq!(plan.steps()[1].outcome(), "Fehlerantworten ergänzen");
        assert_eq!(plan.steps()[2].outcome(), "Start prüfen");
        assert_eq!(
            plan.steps()[3].verification_intent(),
            AgentWorkPlanVerificationIntent::Check
        );
        Ok(())
    }

    #[test]
    fn same_outcome_remains_separate_when_change_and_test_intents_differ()
    -> Result<(), Box<dyn std::error::Error>> {
        let plan = AgentWorkPlan::from_research_decisions(
            "Serververtrag umsetzen",
            "Serververtrag umsetzen",
        )?;
        assert_eq!(plan.steps().len(), 2);
        assert_eq!(
            plan.steps()[0].verification_intent(),
            AgentWorkPlanVerificationIntent::Change
        );
        assert_eq!(
            plan.steps()[1].verification_intent(),
            AgentWorkPlanVerificationIntent::Check
        );
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

    #[test]
    fn greenfield_execution_preserves_checks_without_creating_test_implementation()
    -> Result<(), Box<dyn std::error::Error>> {
        let plan = AgentWorkPlan::from_reviewed_markdown(
            "## Implementation Changes\n1. server.py anlegen\n2. Fehlerantworten ergänzen\n## Test Plan\n1. GET prüfen\n2. POST prüfen",
        )?
        .into_greenfield_execution()?;

        assert_eq!(plan.steps().len(), 3);
        assert!(plan.steps()[0].outcome().contains("server.py anlegen"));
        assert!(
            plan.steps()[0]
                .outcome()
                .contains("Fehlerantworten ergänzen")
        );
        assert_eq!(
            plan.steps()[0].verification_intent(),
            AgentWorkPlanVerificationIntent::Change
        );
        assert!(plan.steps()[1].outcome().contains("GET prüfen"));
        assert!(plan.steps()[2].outcome().contains("POST prüfen"));
        assert_eq!(
            plan.steps()[1].verification_intent(),
            AgentWorkPlanVerificationIntent::Check
        );
        assert_eq!(
            plan.steps()[2].verification_intent(),
            AgentWorkPlanVerificationIntent::Check
        );
        Ok(())
    }

    #[test]
    fn small_greenfield_server_does_not_acquire_a_testsuite()
    -> Result<(), Box<dyn std::error::Error>> {
        for checks in ["", super::NO_ADDITIONAL_AGENT_CHECKS] {
            let plan = AgentWorkPlan::from_research_decisions("1. server.py anlegen", checks)?
                .into_greenfield_execution()?;
            assert_eq!(plan.steps().len(), 1);
            assert!(plan.steps()[0].outcome().contains("server.py"));
            assert!(!plan.steps()[0].outcome().contains("Tests"));
        }
        Ok(())
    }

    #[test]
    fn requested_and_risk_justified_test_changes_remain_distinct_from_running_checks()
    -> Result<(), Box<dyn std::error::Error>> {
        for implementation in [
            "1. Requested unit tests for the response mapping implementieren",
            "1. Regressionstest für den konkreten Fehler anlegen: negative Werte wurden falsch akzeptiert",
        ] {
            let plan = AgentWorkPlan::from_research_decisions(
                implementation,
                "1. Vorhandenen begrenzten Testcommand ausführen",
            )?
            .into_greenfield_execution()?;
            assert_eq!(plan.steps().len(), 2);
            assert!(
                plan.steps()[0]
                    .outcome()
                    .contains(implementation.trim_start_matches("1. "))
            );
            assert_eq!(
                plan.steps()[0].verification_intent(),
                AgentWorkPlanVerificationIntent::Change
            );
            assert_eq!(
                plan.steps()[1].verification_intent(),
                AgentWorkPlanVerificationIntent::Check
            );
        }
        Ok(())
    }
}
