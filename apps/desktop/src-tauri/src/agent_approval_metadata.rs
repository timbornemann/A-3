use a3_application::AgentApprovalControlMetadata;
use a3_domain::{AgentRunTimestamp, ApprovalId, RunEventId};
use std::error::Error;
use std::fmt;
use std::time::{SystemTime, UNIX_EPOCH};

/// OS-backed identities and time for approval controls; none originate in the WebView.
#[derive(Debug, Default)]
pub(crate) struct SystemAgentApprovalMetadata;

impl SystemAgentApprovalMetadata {
    pub(crate) fn next(
        &self,
    ) -> Result<AgentApprovalControlMetadata, AgentApprovalMetadataFailure> {
        let mut approval = [0_u8; 32];
        let mut event = [0_u8; 32];
        getrandom::fill(&mut approval)
            .map_err(|_| AgentApprovalMetadataFailure::IdentityUnavailable)?;
        getrandom::fill(&mut event)
            .map_err(|_| AgentApprovalMetadataFailure::IdentityUnavailable)?;
        let millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| AgentApprovalMetadataFailure::ClockUnavailable)?
            .as_millis();
        let millis =
            u64::try_from(millis).map_err(|_| AgentApprovalMetadataFailure::ClockUnavailable)?;
        let observed_at = AgentRunTimestamp::from_unix_millis(millis)
            .map_err(|_| AgentApprovalMetadataFailure::ClockUnavailable)?;
        Ok(AgentApprovalControlMetadata::new(
            ApprovalId::from_bytes(approval),
            RunEventId::from_bytes(event),
            observed_at,
        ))
    }

    pub(crate) fn now(&self) -> Result<AgentRunTimestamp, AgentApprovalMetadataFailure> {
        let millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| AgentApprovalMetadataFailure::ClockUnavailable)?
            .as_millis();
        let millis =
            u64::try_from(millis).map_err(|_| AgentApprovalMetadataFailure::ClockUnavailable)?;
        AgentRunTimestamp::from_unix_millis(millis)
            .map_err(|_| AgentApprovalMetadataFailure::ClockUnavailable)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AgentApprovalMetadataFailure {
    IdentityUnavailable,
    ClockUnavailable,
}

impl fmt::Display for AgentApprovalMetadataFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::IdentityUnavailable => "Agent approval identity is unavailable",
            Self::ClockUnavailable => "Agent approval wall clock is unavailable",
        })
    }
}

impl Error for AgentApprovalMetadataFailure {}

#[cfg(test)]
mod tests {
    use super::SystemAgentApprovalMetadata;
    use std::error::Error;

    #[test]
    fn metadata_is_core_generated() -> Result<(), Box<dyn Error>> {
        let source = SystemAgentApprovalMetadata;
        assert_ne!(source.next()?, source.next()?);
        assert!(source.now()?.unix_millis() > 0);
        Ok(())
    }
}
