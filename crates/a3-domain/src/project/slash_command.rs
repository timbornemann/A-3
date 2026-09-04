use super::{AgentResearchDepth, AgentSessionMode};
use std::error::Error;
use std::fmt;

const MAX_LENSES: usize = 2;
const MAX_SUBJECT_BYTES: usize = 16 * 1_024;

/// Version of the Core-owned slash-command catalog.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SlashCommandCatalogVersion {
    /// Initial built-in catalog.
    V1,
}

/// Closed primary slash command that determines the requested outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SlashCommand {
    /// Builds evidence-bound diagrams for an Ask response.
    Diagram,
    /// Explains current structure and behavior.
    Explain,
    /// Traces current control, call, or data flow.
    Trace,
    /// Finds and organizes repository tasks.
    Todos,
    /// Assesses a proposed or current working-tree change.
    Impact,
    /// Performs a strict senior engineering review.
    Review,
    /// Diagnoses and, where permitted, repairs a defect.
    Debug,
    /// Plans or updates supported documentation.
    Doc,
    /// Plans or performs behavior-preserving restructuring.
    Refactor,
    /// Plans, adds, and verifies tests.
    Test,
}

/// Closed research emphasis selected by one built-in command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SlashCommandResearchProfile {
    /// Evidence-backed diagram topology.
    Diagram,
    /// Plain-language structure and behavior explanation.
    Explanation,
    /// Control, call, or data-flow tracing.
    Trace,
    /// Repository-wide task-marker discovery.
    Todos,
    /// Change blast-radius analysis.
    Impact,
    /// Strict correctness and quality review.
    Review,
    /// Reproduction and root-cause analysis.
    Debug,
    /// Documentation coverage and consistency.
    Documentation,
    /// Behavior-preserving structural analysis.
    Refactor,
    /// Test topology and behavioral coverage.
    Test,
}

/// Closed result shape expected from one built-in command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SlashCommandResultContract {
    /// One to three typed evidence diagrams plus a cited explanation.
    Diagrams,
    /// A cited explanatory answer.
    Explanation,
    /// A cited ordered flow.
    Trace,
    /// Severity-ordered cited findings or a plan derived from them.
    Findings,
    /// A cited change-impact report or implementation plan.
    Impact,
    /// A cited diagnosis, repair plan, or verified repair.
    Repair,
    /// A decision-complete documentation plan or verified documentation change.
    Documentation,
    /// A behavior-preserving plan or verified refactor.
    Refactor,
    /// A test strategy or verified test change.
    Tests,
}

/// Verification emphasis used by Agent without granting command authority.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SlashCommandVerificationProfile {
    /// Current source citations are sufficient because the mode is read-only.
    EvidenceOnly,
    /// Prefer project tests, then diagnostics and build checks.
    Review,
    /// Prefer reproduction-oriented project tests before broader checks.
    Repair,
    /// Prefer documentation-aware diagnostics and builds.
    Documentation,
    /// Prefer regression tests and diagnostics proving preserved behavior.
    BehaviorPreservation,
    /// Prefer the current manifest-proven test command.
    Tests,
}

impl SlashCommand {
    /// Every built-in primary command in stable catalog order.
    pub const ALL: [Self; 10] = [
        Self::Diagram,
        Self::Explain,
        Self::Trace,
        Self::Todos,
        Self::Impact,
        Self::Review,
        Self::Debug,
        Self::Doc,
        Self::Refactor,
        Self::Test,
    ];

    #[must_use]
    /// Returns the lower-case command token without a leading slash.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Diagram => "diagram",
            Self::Explain => "explain",
            Self::Trace => "trace",
            Self::Todos => "todos",
            Self::Impact => "impact",
            Self::Review => "review",
            Self::Debug => "debug",
            Self::Doc => "doc",
            Self::Refactor => "refactor",
            Self::Test => "test",
        }
    }

    #[must_use]
    /// Returns the immutable research depth selected by this command.
    pub const fn depth(self) -> AgentResearchDepth {
        match self {
            Self::Diagram | Self::Explain | Self::Trace => AgentResearchDepth::Standard,
            Self::Todos
            | Self::Impact
            | Self::Review
            | Self::Debug
            | Self::Doc
            | Self::Refactor
            | Self::Test => AgentResearchDepth::Thorough,
        }
    }

    #[must_use]
    /// Returns whether this command is valid in the supplied conversation mode.
    pub const fn available_in(self, mode: AgentSessionMode) -> bool {
        match self {
            Self::Diagram | Self::Explain | Self::Trace => matches!(mode, AgentSessionMode::Ask),
            Self::Todos | Self::Impact => {
                matches!(mode, AgentSessionMode::Ask | AgentSessionMode::Plan)
            }
            Self::Review | Self::Debug => true,
            Self::Doc | Self::Refactor | Self::Test => {
                matches!(mode, AgentSessionMode::Plan | AgentSessionMode::Agent)
            }
        }
    }

    #[must_use]
    /// Returns the deterministic behavior when no free-text subject was supplied.
    pub const fn empty_input_behavior(self) -> SlashCommandEmptyInput {
        match self {
            Self::Review | Self::Todos => SlashCommandEmptyInput::RepositoryWide,
            Self::Impact => SlashCommandEmptyInput::WorkingChanges,
            Self::Doc | Self::Refactor | Self::Debug | Self::Test => {
                SlashCommandEmptyInput::Clarify
            }
            Self::Diagram | Self::Explain | Self::Trace => SlashCommandEmptyInput::Reject,
        }
    }

    #[must_use]
    /// Returns the immutable research emphasis for this catalog entry.
    pub const fn research_profile(self) -> SlashCommandResearchProfile {
        match self {
            Self::Diagram => SlashCommandResearchProfile::Diagram,
            Self::Explain => SlashCommandResearchProfile::Explanation,
            Self::Trace => SlashCommandResearchProfile::Trace,
            Self::Todos => SlashCommandResearchProfile::Todos,
            Self::Impact => SlashCommandResearchProfile::Impact,
            Self::Review => SlashCommandResearchProfile::Review,
            Self::Debug => SlashCommandResearchProfile::Debug,
            Self::Doc => SlashCommandResearchProfile::Documentation,
            Self::Refactor => SlashCommandResearchProfile::Refactor,
            Self::Test => SlashCommandResearchProfile::Test,
        }
    }

    #[must_use]
    /// Returns the immutable result contract for this catalog entry.
    pub const fn result_contract(self) -> SlashCommandResultContract {
        match self {
            Self::Diagram => SlashCommandResultContract::Diagrams,
            Self::Explain => SlashCommandResultContract::Explanation,
            Self::Trace => SlashCommandResultContract::Trace,
            Self::Todos | Self::Review => SlashCommandResultContract::Findings,
            Self::Impact => SlashCommandResultContract::Impact,
            Self::Debug => SlashCommandResultContract::Repair,
            Self::Doc => SlashCommandResultContract::Documentation,
            Self::Refactor => SlashCommandResultContract::Refactor,
            Self::Test => SlashCommandResultContract::Tests,
        }
    }

    #[must_use]
    /// Returns the verification emphasis without selecting or authorizing a process.
    pub const fn verification_profile(self) -> SlashCommandVerificationProfile {
        match self {
            Self::Diagram | Self::Explain | Self::Trace | Self::Todos | Self::Impact => {
                SlashCommandVerificationProfile::EvidenceOnly
            }
            Self::Review => SlashCommandVerificationProfile::Review,
            Self::Debug => SlashCommandVerificationProfile::Repair,
            Self::Doc => SlashCommandVerificationProfile::Documentation,
            Self::Refactor => SlashCommandVerificationProfile::BehaviorPreservation,
            Self::Test => SlashCommandVerificationProfile::Tests,
        }
    }

    /// Restores one immutable catalog command from its stable persisted name.
    #[must_use]
    pub fn from_stable_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|command| command.name() == name)
    }
}

/// Optional specialist lens applied to one primary slash command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SlashCommandLens {
    /// Focuses on trust boundaries, injection, secrets, paths, processes, and network use.
    Security,
    /// Focuses on runtime, memory, I/O, and reproducible measurements.
    Performance,
    /// Focuses on ownership, dependencies, accepted decisions, and layering.
    Architecture,
}

impl SlashCommandLens {
    /// Every built-in specialist lens in stable catalog order.
    pub const ALL: [Self; 3] = [Self::Security, Self::Performance, Self::Architecture];

    #[must_use]
    /// Returns the lower-case lens token without a leading slash.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Security => "security",
            Self::Performance => "performance",
            Self::Architecture => "architecture",
        }
    }

    /// Restores one immutable catalog lens from its stable persisted name.
    #[must_use]
    pub fn from_stable_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|lens| lens.name() == name)
    }
}

/// Deterministic behavior when a command has no free-text subject.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SlashCommandEmptyInput {
    /// Inspect the complete safely available repository scope.
    RepositoryWide,
    /// Inspect the current staged, unstaged, and safely readable untracked changes.
    WorkingChanges,
    /// Ask one concise question before continuing.
    Clarify,
    /// Reject the invocation because this command always requires a subject.
    Reject,
}

/// Parsed user input before it enters any model prompt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParsedSlashCommand {
    /// Ordinary chat text. A leading escaped slash has already been restored.
    Plain(String),
    /// A validated built-in invocation.
    Command(SlashCommandInvocation),
}

/// One validated primary command, ordered specialist lenses, and bounded subject text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlashCommandInvocation {
    primary: SlashCommand,
    lenses: Vec<SlashCommandLens>,
    subject: String,
}

impl SlashCommandInvocation {
    #[must_use]
    /// Returns the one primary command.
    pub const fn primary(&self) -> SlashCommand {
        self.primary
    }

    #[must_use]
    /// Returns zero to two distinct lenses in user-supplied order.
    pub fn lenses(&self) -> &[SlashCommandLens] {
        &self.lenses
    }

    #[must_use]
    /// Returns the bounded free-text subject without command tokens.
    pub fn subject(&self) -> &str {
        &self.subject
    }

    #[must_use]
    /// Returns the Core-owned depth after applying any specialist lenses.
    pub fn depth(&self) -> AgentResearchDepth {
        if self.lenses.is_empty() {
            self.primary.depth()
        } else {
            AgentResearchDepth::Thorough
        }
    }

    #[must_use]
    /// Returns the deterministic no-subject behavior.
    pub const fn empty_input_behavior(&self) -> SlashCommandEmptyInput {
        self.primary.empty_input_behavior()
    }

    #[must_use]
    /// Returns the immutable command-catalog version.
    pub const fn catalog_version(&self) -> SlashCommandCatalogVersion {
        SlashCommandCatalogVersion::V1
    }
}

/// Parses the leading command sequence and enforces the mode capability envelope.
pub fn parse_slash_command(
    mode: AgentSessionMode,
    input: &str,
) -> Result<ParsedSlashCommand, SlashCommandParseError> {
    let trimmed = input.trim();
    if !trimmed.starts_with('/') {
        return Ok(ParsedSlashCommand::Plain(trimmed.to_owned()));
    }
    if let Some(escaped) = trimmed.strip_prefix("//") {
        return Ok(ParsedSlashCommand::Plain(format!("/{escaped}")));
    }

    let mut tokens = trimmed.split_whitespace().peekable();
    let first = tokens
        .next()
        .ok_or(SlashCommandParseError::UnknownCommand)?;
    let first_name = command_name(first)?;
    let (primary, first_lens) = match (
        SlashCommand::from_stable_name(first_name),
        SlashCommandLens::from_stable_name(first_name),
    ) {
        (Some(primary), None) => (primary, None),
        (None, Some(lens)) => (SlashCommand::Review, Some(lens)),
        _ => return Err(SlashCommandParseError::UnknownCommand),
    };
    if !primary.available_in(mode) {
        return Err(SlashCommandParseError::UnavailableInMode);
    }

    let mut lenses = Vec::with_capacity(MAX_LENSES);
    if let Some(lens) = first_lens {
        lenses.push(lens);
    }
    let mut subject_tokens = Vec::new();
    while let Some(token) = tokens.next() {
        if subject_tokens.is_empty() && token.starts_with('/') {
            let name = command_name(token)?;
            if SlashCommand::from_stable_name(name).is_some() {
                return Err(SlashCommandParseError::MultiplePrimaryCommands);
            }
            let lens = SlashCommandLens::from_stable_name(name)
                .ok_or(SlashCommandParseError::UnknownCommand)?;
            if lenses.contains(&lens) {
                return Err(SlashCommandParseError::DuplicateLens);
            }
            if lenses.len() == MAX_LENSES {
                return Err(SlashCommandParseError::TooManyLenses);
            }
            lenses.push(lens);
        } else {
            subject_tokens.push(token);
            subject_tokens.extend(tokens);
            break;
        }
    }
    let subject = subject_tokens.join(" ");
    if subject.len() > MAX_SUBJECT_BYTES {
        return Err(SlashCommandParseError::SubjectTooLarge);
    }
    if subject.is_empty() && primary.empty_input_behavior() == SlashCommandEmptyInput::Reject {
        return Err(SlashCommandParseError::MissingSubject);
    }
    Ok(ParsedSlashCommand::Command(SlashCommandInvocation {
        primary,
        lenses,
        subject,
    }))
}

fn command_name(token: &str) -> Result<&str, SlashCommandParseError> {
    let name = token
        .strip_prefix('/')
        .ok_or(SlashCommandParseError::UnknownCommand)?;
    if name.is_empty()
        || name.len() > 32
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    {
        return Err(SlashCommandParseError::UnknownCommand);
    }
    Ok(name)
}

/// Stable, content-free reason an invocation was rejected before persistence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlashCommandParseError {
    /// The command or lens token is not in the built-in catalog.
    UnknownCommand,
    /// The primary command is not available in the selected mode.
    UnavailableInMode,
    /// More than one primary command was supplied.
    MultiplePrimaryCommands,
    /// A specialist lens was supplied more than once.
    DuplicateLens,
    /// More than two specialist lenses were supplied.
    TooManyLenses,
    /// A subject-required command had no free-text subject.
    MissingSubject,
    /// The free-text subject crossed the fixed command-input bound.
    SubjectTooLarge,
}

impl fmt::Display for SlashCommandParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::UnknownCommand => "slash command is unknown",
            Self::UnavailableInMode => "slash command is unavailable in this mode",
            Self::MultiplePrimaryCommands => "slash command input has multiple primary commands",
            Self::DuplicateLens => "slash command input repeats a lens",
            Self::TooManyLenses => "slash command input exceeds the lens limit",
            Self::MissingSubject => "slash command requires a subject",
            Self::SubjectTooLarge => "slash command subject exceeds its fixed limit",
        })
    }
}

impl Error for SlashCommandParseError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn command(
        mode: AgentSessionMode,
        input: &str,
    ) -> Result<SlashCommandInvocation, Box<dyn Error>> {
        match parse_slash_command(mode, input)? {
            ParsedSlashCommand::Command(command) => Ok(command),
            ParsedSlashCommand::Plain(_) => Err("expected a command".into()),
        }
    }

    #[test]
    fn parses_primary_lenses_and_subject_with_core_owned_depth() -> Result<(), Box<dyn Error>> {
        let command = command(
            AgentSessionMode::Agent,
            "/review /security /performance authentication",
        )?;
        assert_eq!(command.primary(), SlashCommand::Review);
        assert_eq!(
            command.lenses(),
            &[SlashCommandLens::Security, SlashCommandLens::Performance]
        );
        assert_eq!(command.subject(), "authentication");
        assert_eq!(command.depth(), AgentResearchDepth::Thorough);
        Ok(())
    }

    #[test]
    fn standalone_lens_becomes_review() -> Result<(), Box<dyn Error>> {
        let command = command(AgentSessionMode::Ask, "/architecture module boundaries")?;
        assert_eq!(command.primary(), SlashCommand::Review);
        assert_eq!(command.lenses(), &[SlashCommandLens::Architecture]);
        Ok(())
    }

    #[test]
    fn escaped_slash_is_plain_text() {
        assert_eq!(
            parse_slash_command(AgentSessionMode::Ask, "//review this syntax"),
            Ok(ParsedSlashCommand::Plain("/review this syntax".to_owned()))
        );
    }

    #[test]
    fn rejects_invalid_combinations_and_modes() {
        assert_eq!(
            parse_slash_command(AgentSessionMode::Agent, "/diagram architecture"),
            Err(SlashCommandParseError::UnavailableInMode)
        );
        assert_eq!(
            parse_slash_command(AgentSessionMode::Ask, "/review /debug auth"),
            Err(SlashCommandParseError::MultiplePrimaryCommands)
        );
        assert_eq!(
            parse_slash_command(AgentSessionMode::Ask, "/review /security /security"),
            Err(SlashCommandParseError::DuplicateLens)
        );
        assert_eq!(
            parse_slash_command(
                AgentSessionMode::Ask,
                "/review /security /performance /architecture"
            ),
            Err(SlashCommandParseError::TooManyLenses)
        );
        assert_eq!(
            parse_slash_command(AgentSessionMode::Ask, "/diagram"),
            Err(SlashCommandParseError::MissingSubject)
        );
        assert_eq!(
            parse_slash_command(AgentSessionMode::Ask, "/Review auth"),
            Err(SlashCommandParseError::UnknownCommand)
        );
        assert_eq!(
            parse_slash_command(
                AgentSessionMode::Ask,
                &format!("/explain {}", "x".repeat(MAX_SUBJECT_BYTES + 1))
            ),
            Err(SlashCommandParseError::SubjectTooLarge)
        );
    }

    #[test]
    fn records_empty_command_behaviors() -> Result<(), Box<dyn Error>> {
        let review = command(AgentSessionMode::Agent, "/review")?;
        assert_eq!(
            review.empty_input_behavior(),
            SlashCommandEmptyInput::RepositoryWide
        );
        let impact = command(AgentSessionMode::Plan, "/impact")?;
        assert_eq!(
            impact.empty_input_behavior(),
            SlashCommandEmptyInput::WorkingChanges
        );
        let doc = command(AgentSessionMode::Plan, "/doc")?;
        assert_eq!(doc.empty_input_behavior(), SlashCommandEmptyInput::Clarify);
        Ok(())
    }

    #[test]
    fn catalog_carries_closed_research_result_and_verification_profiles() {
        assert_eq!(
            SlashCommand::Diagram.research_profile(),
            SlashCommandResearchProfile::Diagram
        );
        assert_eq!(
            SlashCommand::Review.result_contract(),
            SlashCommandResultContract::Findings
        );
        assert_eq!(
            SlashCommand::Refactor.verification_profile(),
            SlashCommandVerificationProfile::BehaviorPreservation
        );
        assert_eq!(
            SlashCommand::Explain.verification_profile(),
            SlashCommandVerificationProfile::EvidenceOnly
        );
    }
}
