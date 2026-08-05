use a3_domain::{
    FusedRetrievalResult, FusionError, FusionPolicy, FusionPolicyVersion, FusionResultLimit,
    RetrievalCandidateSets,
};

/// Inbound use case for deterministic cross-channel retrieval fusion.
#[derive(Debug, Clone, Copy)]
pub struct FuseRetrievalCandidates {
    policy: FusionPolicy,
}

impl FuseRetrievalCandidates {
    /// Creates the use case with the documented version-one policy.
    #[must_use]
    pub const fn version_one() -> Self {
        Self {
            policy: FusionPolicy::v1(),
        }
    }

    /// Returns the exact policy version that will be retained in every result.
    #[must_use]
    pub const fn policy_version(self) -> FusionPolicyVersion {
        self.policy.version()
    }

    /// Fuses already bounded candidate sets without performing storage or model access.
    pub fn execute(
        self,
        candidates: RetrievalCandidateSets,
        result_limit: FusionResultLimit,
    ) -> Result<FusedRetrievalResult, FusionError> {
        self.policy.fuse(candidates, result_limit)
    }
}

#[cfg(test)]
mod tests {
    use super::FuseRetrievalCandidates;
    use a3_domain::{FusionResultLimit, IndexRunId, RetrievalCandidateSets, SnapshotId};

    #[test]
    fn version_one_use_case_retains_publication_and_policy()
    -> Result<(), Box<dyn std::error::Error>> {
        let run_id = IndexRunId::from_bytes([1; 32]);
        let snapshot_id = SnapshotId::from_bytes([2; 32]);
        let candidates = RetrievalCandidateSets::new(run_id, snapshot_id, Vec::new())?;
        let fusion = FuseRetrievalCandidates::version_one();

        let result = fusion.execute(candidates, FusionResultLimit::DEFAULT)?;

        assert_eq!(result.index_run_id(), run_id);
        assert_eq!(result.snapshot_id(), snapshot_id);
        assert_eq!(result.policy_version(), fusion.policy_version());
        assert!(result.hits().is_empty());
        assert!(!result.truncated());
        Ok(())
    }
}
