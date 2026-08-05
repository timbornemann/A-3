use a3_domain::{
    DeepMapPlanError, DeepMapPlanner, ExploreBudget, ExplorePlan, ExplorePolicyVersion,
    ModuleCardSchemaVersion, ModuleCoverageSnapshot, PublishedIndex,
};

/// Inbound model-free use case for deterministic Deep-Map planning.
#[derive(Debug, Clone, Copy)]
pub struct PlanDeepMap {
    planner: DeepMapPlanner,
}

impl PlanDeepMap {
    /// Uses the accepted version-one schema, coverage, ranking, budget, and stop policy.
    #[must_use]
    pub const fn version_one() -> Self {
        Self {
            planner: DeepMapPlanner::v1(),
        }
    }

    /// Plans bounded read-only exploration from an already published index only.
    pub fn execute(
        self,
        published: &PublishedIndex,
        coverage: &ModuleCoverageSnapshot,
        budget: ExploreBudget,
    ) -> Result<ExplorePlan, DeepMapPlanError> {
        self.planner.plan(published, coverage, budget)
    }

    #[must_use]
    /// Returns deterministic planning semantics retained in the result.
    pub const fn policy_version(self) -> ExplorePolicyVersion {
        ExplorePolicyVersion::V1
    }

    #[must_use]
    /// Returns the Module Card schema interpreted by the planner.
    pub const fn schema_version(self) -> ModuleCardSchemaVersion {
        ModuleCardSchemaVersion::V1
    }
}

#[cfg(test)]
mod tests {
    use super::PlanDeepMap;
    use a3_domain::{ExplorePolicyVersion, ModuleCardSchemaVersion};

    #[test]
    fn use_case_exposes_the_exact_schema_and_policy_versions() {
        let planner = PlanDeepMap::version_one();
        assert_eq!(planner.policy_version(), ExplorePolicyVersion::V1);
        assert_eq!(planner.schema_version(), ModuleCardSchemaVersion::V1);
    }
}
