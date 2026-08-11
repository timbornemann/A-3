use a3_application::{
    AgentGoalGeneratedIdentity, AgentGoalMetadataFailure, AgentGoalMetadataSource,
};
use a3_domain::GoalContractTimestamp;
use std::time::{SystemTime, UNIX_EPOCH};

/// Operating-system-backed identity and wall-clock source owned by the desktop Core.
#[derive(Debug, Default)]
pub(crate) struct SystemAgentGoalMetadata;

impl AgentGoalMetadataSource for SystemAgentGoalMetadata {
    fn next_identity(&self) -> Result<AgentGoalGeneratedIdentity, AgentGoalMetadataFailure> {
        let mut bytes = [0_u8; 32];
        getrandom::fill(&mut bytes).map_err(|_| AgentGoalMetadataFailure::IdentityUnavailable)?;
        Ok(AgentGoalGeneratedIdentity::from_bytes(bytes))
    }

    fn now(&self) -> Result<GoalContractTimestamp, AgentGoalMetadataFailure> {
        let millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| AgentGoalMetadataFailure::ClockUnavailable)?
            .as_millis();
        let millis =
            u64::try_from(millis).map_err(|_| AgentGoalMetadataFailure::ClockUnavailable)?;
        GoalContractTimestamp::from_unix_millis(millis)
            .map_err(|_| AgentGoalMetadataFailure::ClockUnavailable)
    }
}

#[cfg(test)]
mod tests {
    use super::SystemAgentGoalMetadata;
    use a3_application::AgentGoalMetadataSource;
    use std::error::Error;

    #[test]
    fn system_metadata_keeps_identity_and_time_inside_the_core() -> Result<(), Box<dyn Error>> {
        let source = SystemAgentGoalMetadata;

        assert_ne!(source.next_identity()?, source.next_identity()?);
        assert!(source.now()?.unix_millis() > 0);
        Ok(())
    }
}
