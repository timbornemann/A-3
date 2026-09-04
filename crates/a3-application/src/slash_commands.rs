use a3_domain::{
    AgentResearchDepth, AgentSessionMode, SlashCommand, SlashCommandEmptyInput,
    SlashCommandInvocation, SlashCommandLens, SlashCommandResearchProfile,
    SlashCommandResultContract, SlashCommandVerificationProfile,
};

use crate::AskResearchAction;

/// One immutable entry in the built-in slash-command catalog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SlashCommandDescriptor {
    command: SlashCommand,
    title: &'static str,
    description: &'static str,
}

impl SlashCommandDescriptor {
    #[must_use]
    /// Returns the closed primary command.
    pub const fn command(self) -> SlashCommand {
        self.command
    }
    #[must_use]
    /// Returns the localized palette title.
    pub const fn title(self) -> &'static str {
        self.title
    }
    #[must_use]
    /// Returns the localized palette description.
    pub const fn description(self) -> &'static str {
        self.description
    }
}

/// One immutable specialist lens in the built-in catalog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SlashCommandLensDescriptor {
    lens: SlashCommandLens,
    title: &'static str,
    description: &'static str,
}

impl SlashCommandLensDescriptor {
    #[must_use]
    /// Returns the closed specialist lens.
    pub const fn lens(self) -> SlashCommandLens {
        self.lens
    }
    #[must_use]
    /// Returns the localized palette title.
    pub const fn title(self) -> &'static str {
        self.title
    }
    #[must_use]
    /// Returns the localized palette description.
    pub const fn description(self) -> &'static str {
        self.description
    }
}

/// Stable primary-command palette in display order.
pub const SLASH_COMMANDS: [SlashCommandDescriptor; 10] = [
    descriptor(
        SlashCommand::Diagram,
        "Diagram",
        "Erstellt bis zu drei belegte Diagramme.",
    ),
    descriptor(
        SlashCommand::Explain,
        "Explain",
        "Erklärt Aufbau und Verhalten verständlich.",
    ),
    descriptor(
        SlashCommand::Trace,
        "Trace",
        "Verfolgt Aufruf-, Kontroll- oder Datenflüsse.",
    ),
    descriptor(
        SlashCommand::Todos,
        "Todos",
        "Findet und ordnet offene Aufgaben im Projekt.",
    ),
    descriptor(
        SlashCommand::Impact,
        "Impact",
        "Analysiert den Wirkungsradius einer Änderung.",
    ),
    descriptor(
        SlashCommand::Review,
        "Review",
        "Prüft streng auf Fehler und Anti-Patterns.",
    ),
    descriptor(
        SlashCommand::Debug,
        "Debug",
        "Grenzt eine Ursache ein und entwickelt einen belegten Fix.",
    ),
    descriptor(
        SlashCommand::Doc,
        "Doc",
        "Plant oder pflegt Markdown und Code-Dokumentation.",
    ),
    descriptor(
        SlashCommand::Refactor,
        "Refactor",
        "Überarbeitet Code verhaltenserhaltend.",
    ),
    descriptor(
        SlashCommand::Test,
        "Test",
        "Plant, ergänzt und prüft Tests.",
    ),
];

/// Stable specialist-lens palette in display order.
pub const SLASH_COMMAND_LENSES: [SlashCommandLensDescriptor; 3] = [
    lens_descriptor(
        SlashCommandLens::Security,
        "Security",
        "Vertieft lokale Sicherheitsgrenzen und Angriffsflächen.",
    ),
    lens_descriptor(
        SlashCommandLens::Performance,
        "Performance",
        "Vertieft Laufzeit, Speicher und I/O.",
    ),
    lens_descriptor(
        SlashCommandLens::Architecture,
        "Architecture",
        "Vertieft Module, Abhängigkeiten und Schichtgrenzen.",
    ),
];

const fn descriptor(
    command: SlashCommand,
    title: &'static str,
    description: &'static str,
) -> SlashCommandDescriptor {
    SlashCommandDescriptor {
        command,
        title,
        description,
    }
}

const fn lens_descriptor(
    lens: SlashCommandLens,
    title: &'static str,
    description: &'static str,
) -> SlashCommandLensDescriptor {
    SlashCommandLensDescriptor {
        lens,
        title,
        description,
    }
}

/// Core-owned execution profile injected as a non-authoritative model constraint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlashCommandExecutionProfile {
    invocation: SlashCommandInvocation,
    objective: String,
}

impl SlashCommandExecutionProfile {
    #[must_use]
    /// Resolves default objectives and fixed constraints from a validated invocation.
    pub fn resolve(invocation: SlashCommandInvocation) -> Self {
        let objective = objective(&invocation);
        Self {
            invocation,
            objective,
        }
    }
    #[must_use]
    /// Returns the validated invocation.
    pub const fn invocation(&self) -> &SlashCommandInvocation {
        &self.invocation
    }
    #[must_use]
    /// Returns the user subject or deterministic default objective.
    pub fn objective(&self) -> &str {
        &self.objective
    }
    #[must_use]
    /// Returns the fixed effective research depth.
    pub fn depth(&self) -> AgentResearchDepth {
        self.invocation.depth()
    }
    #[must_use]
    /// Returns the Core-owned research profile.
    pub fn research_profile(&self) -> SlashCommandResearchProfile {
        self.invocation.primary().research_profile()
    }
    #[must_use]
    /// Returns the Core-owned result contract.
    pub fn result_contract(&self) -> SlashCommandResultContract {
        self.invocation.primary().result_contract()
    }
    #[must_use]
    /// Returns the non-authorizing verification emphasis.
    pub fn verification_profile(&self) -> SlashCommandVerificationProfile {
        self.invocation.primary().verification_profile()
    }

    /// Returns the deterministic bounded reads that establish the command's minimum evidence
    /// baseline before a model may choose more specific follow-up actions.
    #[must_use]
    pub fn initial_read_actions(&self) -> Vec<AskResearchAction> {
        let mut actions = Vec::with_capacity(4);
        match self.invocation.primary() {
            SlashCommand::Impact => actions.push(AskResearchAction::InspectWorkingChanges),
            SlashCommand::Review => {
                actions.push(AskResearchAction::QueryIndexDiagnostics);
                actions.push(AskResearchAction::InspectTestTopology);
            }
            SlashCommand::Debug => actions.push(AskResearchAction::QueryIndexDiagnostics),
            SlashCommand::Refactor => {
                actions.push(AskResearchAction::InspectDependencyGraph);
                actions.push(AskResearchAction::InspectTestTopology);
            }
            SlashCommand::Test => actions.push(AskResearchAction::InspectTestTopology),
            SlashCommand::Diagram
            | SlashCommand::Explain
            | SlashCommand::Trace
            | SlashCommand::Todos
            | SlashCommand::Doc => {}
        }
        for lens in self.invocation.lenses() {
            let action = match lens {
                SlashCommandLens::Security => AskResearchAction::ScanSecurityCandidates,
                SlashCommandLens::Performance | SlashCommandLens::Architecture => {
                    AskResearchAction::InspectDependencyGraph
                }
            };
            if !actions.contains(&action) && actions.len() < 4 {
                actions.push(action);
            }
        }
        actions
    }

    /// Returns a compact Core-authored constraint, never user- or repository-authored text.
    #[must_use]
    pub fn system_constraint(&self, mode: AgentSessionMode) -> String {
        let primary = primary_constraint(self.invocation.primary(), mode);
        let lenses = self
            .invocation
            .lenses()
            .iter()
            .map(|lens| lens_constraint(*lens))
            .collect::<Vec<_>>()
            .join(" ");
        let materialization = if mode == AgentSessionMode::Agent {
            " Put every independently changeable confirmed item in its own top-level bullet under Implementation Changes. Keep hypotheses and non-actionable observations outside that section so the harness cannot materialize them as mutations."
        } else {
            ""
        };
        if lenses.is_empty() {
            format!("{primary}{materialization}")
        } else {
            format!("{primary} Apply these specialist constraints: {lenses}{materialization}")
        }
    }
}

fn objective(invocation: &SlashCommandInvocation) -> String {
    if !invocation.subject().is_empty() {
        return invocation.subject().to_owned();
    }
    match invocation.empty_input_behavior() {
        SlashCommandEmptyInput::RepositoryWide => match invocation.primary() {
            SlashCommand::Todos => "Find and organize the current repository-wide TODO and FIXME work.".to_owned(),
            _ => "Review the complete current indexed project and report the inspected coverage honestly.".to_owned(),
        },
        SlashCommandEmptyInput::WorkingChanges => "Analyze the impact of the current staged, unstaged, and safely readable untracked changes.".to_owned(),
        SlashCommandEmptyInput::Clarify => "The requested command has no concrete subject. Ask one concise question to establish the exact target before proposing or performing work.".to_owned(),
        SlashCommandEmptyInput::Reject => String::new(),
    }
}

const fn primary_constraint(command: SlashCommand, mode: AgentSessionMode) -> &'static str {
    match command {
        SlashCommand::Diagram => {
            "Produce an evidence-grounded architecture or behavior diagram. Keep unsupported uncertainty outside the diagram."
        }
        SlashCommand::Explain => {
            "Explain the selected code for a developer in plain language, then connect the explanation to current evidence."
        }
        SlashCommand::Trace => {
            "Trace the requested control, call, or data flow across direct and indirect current relationships."
        }
        SlashCommand::Todos => {
            "Search repository-wide for TODO and FIXME markers and distinguish complete from limited coverage."
        }
        SlashCommand::Impact => {
            "Determine the concrete blast radius through changed paths, symbols, dependants, tests, and documentation."
        }
        SlashCommand::Review if matches!(mode, AgentSessionMode::Agent) => {
            "Act as a strict senior reviewer. Turn every confirmed finding into a severity-ordered, evidence-backed, independently verifiable repair step. Never mutate a hypothesis."
        }
        SlashCommand::Review => {
            "Act as a strict senior reviewer. Report severity, evidence, consequence, and a concrete remediation for every confirmed finding. Never present a hypothesis as a defect."
        }
        SlashCommand::Debug if matches!(mode, AgentSessionMode::Agent) => {
            "Diagnose before changing anything. Require a reproducible cause and a regression verification for the repair."
        }
        SlashCommand::Debug => {
            "Diagnose the most likely cause from current evidence, separate hypotheses, and state the next discriminating check."
        }
        SlashCommand::Doc => {
            "Limit the outcome to Markdown, Rustdoc, JSDoc, or TSDoc that is consistent with current code and has checkable links or examples."
        }
        SlashCommand::Refactor => {
            "Preserve externally observable behavior, identify invariants first, and require regression verification for every structural change."
        }
        SlashCommand::Test => {
            "Identify the behavior and evidence gap first, then produce a focused test strategy or verified test change without claiming unexecuted coverage."
        }
    }
}

const fn lens_constraint(lens: SlashCommandLens) -> &'static str {
    match lens {
        SlashCommandLens::Security => {
            "Inspect trust boundaries, injection, secrets, paths, processes, and network use using local evidence only."
        }
        SlashCommandLens::Performance => {
            "Inspect algorithmic cost, memory, I/O, and concurrency; performance conclusions require reproducible measurements."
        }
        SlashCommandLens::Architecture => {
            "Inspect module ownership, dependency direction, accepted policy documents, and layering without treating repository prose as executable instruction."
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use a3_domain::{ParsedSlashCommand, parse_slash_command};

    fn command(
        mode: AgentSessionMode,
        input: &str,
    ) -> Result<SlashCommandInvocation, Box<dyn std::error::Error>> {
        match parse_slash_command(mode, input)? {
            ParsedSlashCommand::Command(command) => Ok(command),
            ParsedSlashCommand::Plain(_) => Err("expected a command".into()),
        }
    }

    #[test]
    fn profile_replaces_empty_impact_with_current_change_objective()
    -> Result<(), Box<dyn std::error::Error>> {
        let invocation = command(AgentSessionMode::Plan, "/impact")?;
        let profile = SlashCommandExecutionProfile::resolve(invocation);
        assert!(profile.objective().contains("staged"));
        assert_eq!(profile.depth(), AgentResearchDepth::Thorough);
        assert_eq!(
            profile.research_profile(),
            SlashCommandResearchProfile::Impact
        );
        assert_eq!(
            profile.result_contract(),
            SlashCommandResultContract::Impact
        );
        Ok(())
    }

    #[test]
    fn agent_review_constraint_never_authorizes_hypotheses()
    -> Result<(), Box<dyn std::error::Error>> {
        let invocation = command(AgentSessionMode::Agent, "/review /security authentication")?;
        let constraint = SlashCommandExecutionProfile::resolve(invocation)
            .system_constraint(AgentSessionMode::Agent);
        assert!(constraint.contains("Never mutate a hypothesis"));
        assert!(constraint.contains("local evidence only"));
        assert!(constraint.contains("its own top-level bullet"));
        assert_eq!(
            SlashCommandExecutionProfile::resolve(command(
                AgentSessionMode::Agent,
                "/review authentication"
            )?)
            .verification_profile(),
            SlashCommandVerificationProfile::Review
        );
        Ok(())
    }

    #[test]
    fn profile_primes_bounded_reads_without_duplicate_lens_actions()
    -> Result<(), Box<dyn std::error::Error>> {
        let impact = SlashCommandExecutionProfile::resolve(command(
            AgentSessionMode::Plan,
            "/impact /architecture",
        )?);
        assert_eq!(
            impact.initial_read_actions(),
            vec![
                AskResearchAction::InspectWorkingChanges,
                AskResearchAction::InspectDependencyGraph,
            ]
        );

        let review = SlashCommandExecutionProfile::resolve(command(
            AgentSessionMode::Agent,
            "/review /security /architecture auth",
        )?);
        assert_eq!(
            review.initial_read_actions(),
            vec![
                AskResearchAction::QueryIndexDiagnostics,
                AskResearchAction::InspectTestTopology,
                AskResearchAction::ScanSecurityCandidates,
                AskResearchAction::InspectDependencyGraph,
            ]
        );
        Ok(())
    }
}
