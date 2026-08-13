use a3_domain::{AgentRunTimestamp, RunEventId};
use std::error::Error;
use std::fmt;
use std::time::{SystemTime, UNIX_EPOCH};

/// Operating-system-backed event identity and wall clock retained inside the desktop Core.
#[derive(Debug, Default)]
pub(crate) struct SystemAgentRecoveryMetadata;

impl SystemAgentRecoveryMetadata {
    /// Generates one non-WebView-controlled append-only recovery event identity.
    pub(crate) fn next_event_id(&self) -> Result<RunEventId, AgentRecoveryMetadataFailure> {
        let mut bytes = [0_u8; 32];
        getrandom::fill(&mut bytes)
            .map_err(|_| AgentRecoveryMetadataFailure::IdentityUnavailable)?;
        Ok(RunEventId::from_bytes(bytes))
    }

    /// Returns one persisted wall-clock timestamp for recovery inspection or commit.
    pub(crate) fn now(&self) -> Result<AgentRunTimestamp, AgentRecoveryMetadataFailure> {
        let millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| AgentRecoveryMetadataFailure::ClockUnavailable)?
            .as_millis();
        let millis =
            u64::try_from(millis).map_err(|_| AgentRecoveryMetadataFailure::ClockUnavailable)?;
        AgentRunTimestamp::from_unix_millis(millis)
            .map_err(|_| AgentRecoveryMetadataFailure::ClockUnavailable)
    }
}

/// Core-owned recovery metadata could not be generated without trusting the WebView.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AgentRecoveryMetadataFailure {
    /// The operating-system random source was unavailable.
    IdentityUnavailable,
    /// Wall-clock time could not be represented by durable storage.
    ClockUnavailable,
}

impl fmt::Display for AgentRecoveryMetadataFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::IdentityUnavailable => "Agent recovery event identity is unavailable",
            Self::ClockUnavailable => "Agent recovery wall clock is unavailable",
        })
    }
}

impl Error for AgentRecoveryMetadataFailure {}

#[cfg(test)]
mod tests {
    use super::SystemAgentRecoveryMetadata;
    use std::error::Error;

    #[test]
    fn recovery_metadata_stays_unique_and_core_owned() -> Result<(), Box<dyn Error>> {
        let source = SystemAgentRecoveryMetadata;
        assert_ne!(source.next_event_id()?, source.next_event_id()?);
        assert!(source.now()?.unix_millis() > 0);
        Ok(())
    }
}
