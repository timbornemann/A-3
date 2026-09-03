use crate::{
    DeepMapJournalEvent, DeepMapPhase, DeepMapPublicationAnchor, DeepMapPublicationResult,
    DeepMapRunModuleSummary, DeepMapRunSummary, DeepMapSafeAction, DeepMapTargetKind,
};
use a3_domain::{DeepMapDiagnosticCode, DeepMapRunState, DeepMapRunTimestamp, ModuleId};

/// Five user-facing phases of every Deep-Map run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeepMapDashboardPhase {
    /// Build a deterministic exploration plan.
    Planning,
    /// Inspect safe index targets.
    Exploring,
    /// Assemble verified field candidates into cards.
    CreatingCards,
    /// Verify claims and evidence.
    Verifying,
    /// Publish cards and project them into the Atlas.
    UpdatingAtlas,
}

impl DeepMapDashboardPhase {
    /// Returns all phases in stable product order.
    #[must_use]
    pub const fn all() -> [Self; 5] {
        [
            Self::Planning,
            Self::Exploring,
            Self::CreatingCards,
            Self::Verifying,
            Self::UpdatingAtlas,
        ]
    }

    const fn ordinal(self) -> u8 {
        match self {
            Self::Planning => 0,
            Self::Exploring => 1,
            Self::CreatingCards => 2,
            Self::Verifying => 3,
            Self::UpdatingAtlas => 4,
        }
    }
}

impl From<DeepMapPhase> for DeepMapDashboardPhase {
    fn from(value: DeepMapPhase) -> Self {
        match value {
            DeepMapPhase::Planning => Self::Planning,
            DeepMapPhase::Exploring => Self::Exploring,
            DeepMapPhase::Claiming => Self::CreatingCards,
            DeepMapPhase::Verifying => Self::Verifying,
            DeepMapPhase::Publishing => Self::UpdatingAtlas,
        }
    }
}

/// Presentation-safe progress state for one phase.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeepMapDashboardPhaseState {
    /// Work has not reached this phase.
    Pending,
    /// Work is currently in this phase.
    Active,
    /// The run passed this phase successfully.
    Completed,
    /// The run stopped while this phase was active.
    Stopped,
}

/// One phase and its Core-derived state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeepMapDashboardPhaseProgress {
    phase: DeepMapDashboardPhase,
    state: DeepMapDashboardPhaseState,
}

impl DeepMapDashboardPhaseProgress {
    /// Returns the product phase.
    #[must_use]
    pub const fn phase(self) -> DeepMapDashboardPhase {
        self.phase
    }
    /// Returns its derived state.
    #[must_use]
    pub const fn state(self) -> DeepMapDashboardPhaseState {
        self.state
    }
}

/// Human-facing overall outcome without provider or budget metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeepMapDashboardState {
    /// The worker is waiting to begin.
    Queued,
    /// Exploration or publication is progressing.
    Running,
    /// The current safe unit is completing before a pause.
    Pausing,
    /// A resumable checkpoint is retained.
    Paused,
    /// Cancellation is completing cooperatively.
    Cancelling,
    /// Verified cards were published.
    Completed,
    /// The current index was already fully mapped.
    AlreadyCurrent,
    /// The user cancelled the run.
    Cancelled,
    /// A safe failure diagnosis is available.
    Failed,
    /// A prior process ended before the run could complete.
    Interrupted,
}

/// Whether current cards and Atlas projections may be joined to this run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeepMapDashboardFreshness {
    /// The run describes the latest published index.
    Current,
    /// The run describes an older project state; current cards must not be shown.
    Historical,
}

/// Safe current activity pointer resolved separately against the same index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeepMapDashboardActivity {
    phase: DeepMapDashboardPhase,
    action: Option<DeepMapSafeAction>,
    target_kind: Option<DeepMapTargetKind>,
    module_id: Option<ModuleId>,
    step_position: Option<u64>,
}

impl DeepMapDashboardActivity {
    /// Returns the active product phase.
    #[must_use]
    pub const fn phase(self) -> DeepMapDashboardPhase {
        self.phase
    }
    /// Returns the closed safe activity class.
    #[must_use]
    pub const fn action(self) -> Option<DeepMapSafeAction> {
        self.action
    }
    /// Returns the closed target class.
    #[must_use]
    pub const fn target_kind(self) -> Option<DeepMapTargetKind> {
        self.target_kind
    }
    /// Returns the module to resolve, when available.
    #[must_use]
    pub const fn module_id(self) -> Option<ModuleId> {
        self.module_id
    }
    /// Returns the one-based plan step to resolve, when available.
    #[must_use]
    pub const fn step_position(self) -> Option<u64> {
        self.step_position
    }
}

/// Core-derived user-facing run projection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeepMapRunDashboard {
    state: DeepMapDashboardState,
    freshness: DeepMapDashboardFreshness,
    phases: [DeepMapDashboardPhaseProgress; 5],
    confirmed_steps: u64,
    total_steps: u64,
    started_at: DeepMapRunTimestamp,
    updated_at: DeepMapRunTimestamp,
    activity: Option<DeepMapDashboardActivity>,
    diagnostic: Option<DeepMapDiagnosticCode>,
    details_incomplete: bool,
}

impl DeepMapRunDashboard {
    /// Derives product state exclusively from the journal and latest publication anchor.
    #[must_use]
    pub fn derive(
        run: &DeepMapRunSummary,
        latest_event: Option<DeepMapJournalEvent>,
        current_anchor: Option<DeepMapPublicationAnchor>,
    ) -> Self {
        let state = dashboard_state(run);
        let freshness = if current_anchor == Some(run.start().anchor()) {
            DeepMapDashboardFreshness::Current
        } else {
            DeepMapDashboardFreshness::Historical
        };
        let active_phase = latest_event
            .and_then(DeepMapJournalEvent::phase)
            .map(DeepMapDashboardPhase::from)
            .or_else(|| {
                matches!(
                    run.state(),
                    DeepMapRunState::Queued | DeepMapRunState::Running
                )
                .then_some(DeepMapDashboardPhase::Planning)
            });
        let terminal_success = matches!(
            state,
            DeepMapDashboardState::Completed | DeepMapDashboardState::AlreadyCurrent
        );
        let terminal_stop = matches!(
            state,
            DeepMapDashboardState::Cancelled
                | DeepMapDashboardState::Failed
                | DeepMapDashboardState::Interrupted
        );
        let phases = DeepMapDashboardPhase::all().map(|phase| {
            let phase_state = if terminal_success {
                DeepMapDashboardPhaseState::Completed
            } else if let Some(active) = active_phase {
                if phase.ordinal() < active.ordinal() {
                    DeepMapDashboardPhaseState::Completed
                } else if phase == active {
                    if terminal_stop {
                        DeepMapDashboardPhaseState::Stopped
                    } else {
                        DeepMapDashboardPhaseState::Active
                    }
                } else {
                    DeepMapDashboardPhaseState::Pending
                }
            } else {
                DeepMapDashboardPhaseState::Pending
            };
            DeepMapDashboardPhaseProgress {
                phase,
                state: phase_state,
            }
        });
        let activity = matches!(
            state,
            DeepMapDashboardState::Queued
                | DeepMapDashboardState::Running
                | DeepMapDashboardState::Pausing
                | DeepMapDashboardState::Paused
                | DeepMapDashboardState::Cancelling
        )
        .then(|| {
            latest_event.and_then(|event| {
                event.phase().map(|phase| DeepMapDashboardActivity {
                    phase: phase.into(),
                    action: event.action(),
                    target_kind: event.target_kind(),
                    module_id: event.module_id(),
                    step_position: event.step_position(),
                })
            })
        })
        .flatten();
        Self {
            state,
            freshness,
            phases,
            confirmed_steps: run.confirmed_steps(),
            total_steps: run.total_steps(),
            started_at: run.start().created_at(),
            updated_at: run.updated_at(),
            activity,
            diagnostic: run.diagnostic(),
            details_incomplete: run.details_incomplete(),
        }
    }

    /// Returns the overall product state.
    #[must_use]
    pub const fn state(&self) -> DeepMapDashboardState {
        self.state
    }
    /// Returns whether current content can safely be joined.
    #[must_use]
    pub const fn freshness(&self) -> DeepMapDashboardFreshness {
        self.freshness
    }
    /// Returns the stable five-phase progress.
    #[must_use]
    pub const fn phases(&self) -> &[DeepMapDashboardPhaseProgress; 5] {
        &self.phases
    }
    /// Returns confirmed plan steps.
    #[must_use]
    pub const fn confirmed_steps(&self) -> u64 {
        self.confirmed_steps
    }
    /// Returns total plan steps.
    #[must_use]
    pub const fn total_steps(&self) -> u64 {
        self.total_steps
    }
    /// Returns the start timestamp.
    #[must_use]
    pub const fn started_at(&self) -> DeepMapRunTimestamp {
        self.started_at
    }
    /// Returns the latest update timestamp.
    #[must_use]
    pub const fn updated_at(&self) -> DeepMapRunTimestamp {
        self.updated_at
    }
    /// Returns the current safe activity pointer.
    #[must_use]
    pub const fn activity(&self) -> Option<DeepMapDashboardActivity> {
        self.activity
    }
    /// Returns the optional safe diagnosis.
    #[must_use]
    pub const fn diagnostic(&self) -> Option<DeepMapDiagnosticCode> {
        self.diagnostic
    }
    /// Returns whether chronological details may be incomplete.
    #[must_use]
    pub const fn details_incomplete(&self) -> bool {
        self.details_incomplete
    }
}

/// Product state of one module within a run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeepMapDashboardModuleState {
    /// Exploration has not reached the module.
    Planned,
    /// One or more targets are being explored.
    Exploring,
    /// Claims or evidence are being checked.
    Verifying,
    /// A current verified Card is available.
    Published,
    /// The run stopped without a current complete Card.
    Incomplete,
}

/// Derives a module state without delegating transition logic to the UI.
#[must_use]
pub fn derive_deep_map_module_state(
    run: &DeepMapRunSummary,
    latest_event: Option<DeepMapJournalEvent>,
    module: DeepMapRunModuleSummary,
    current_card_published: bool,
) -> DeepMapDashboardModuleState {
    if current_card_published {
        return DeepMapDashboardModuleState::Published;
    }
    let terminal = matches!(
        run.state(),
        DeepMapRunState::Succeeded
            | DeepMapRunState::Failed
            | DeepMapRunState::Cancelled
            | DeepMapRunState::Interrupted
    );
    if terminal {
        return DeepMapDashboardModuleState::Incomplete;
    }
    let current = latest_event.is_some_and(|event| event.module_id() == Some(module.module_id()));
    if current
        && latest_event
            .and_then(DeepMapJournalEvent::phase)
            .is_some_and(|phase| {
                matches!(
                    phase,
                    DeepMapPhase::Claiming | DeepMapPhase::Verifying | DeepMapPhase::Publishing
                )
            })
    {
        DeepMapDashboardModuleState::Verifying
    } else if current || module.confirmed_steps() > 0 {
        DeepMapDashboardModuleState::Exploring
    } else {
        DeepMapDashboardModuleState::Planned
    }
}

fn dashboard_state(run: &DeepMapRunSummary) -> DeepMapDashboardState {
    match run.state() {
        DeepMapRunState::Queued => DeepMapDashboardState::Queued,
        DeepMapRunState::Running => DeepMapDashboardState::Running,
        DeepMapRunState::Pausing => DeepMapDashboardState::Pausing,
        DeepMapRunState::Paused => DeepMapDashboardState::Paused,
        DeepMapRunState::Cancelling => DeepMapDashboardState::Cancelling,
        DeepMapRunState::Succeeded => match run.publication_result() {
            Some(DeepMapPublicationResult::AlreadyCurrent) => DeepMapDashboardState::AlreadyCurrent,
            _ => DeepMapDashboardState::Completed,
        },
        DeepMapRunState::Failed => DeepMapDashboardState::Failed,
        DeepMapRunState::Cancelled => DeepMapDashboardState::Cancelled,
        DeepMapRunState::Interrupted => DeepMapDashboardState::Interrupted,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DeepMapEventResult, DeepMapModelDescriptor, DeepMapRunStart};
    use a3_domain::{
        DeepMapEventSequence, DeepMapMode, DeepMapRunId, IndexRunId, ModelProfileId,
        ModelProfileReference, ModelProfileVersion, SnapshotId,
    };

    #[test]
    fn dashboard_derives_five_product_phases_and_current_activity_in_core()
    -> Result<(), Box<dyn std::error::Error>> {
        let anchor = anchor();
        let module_id = ModuleId::from_bytes([8; 32]);
        let run = run_summary(DeepMapRunState::Running, 2, 5, None, None, anchor)?;
        let event = DeepMapJournalEvent::new(
            DeepMapEventSequence::new(4)?,
            DeepMapRunTimestamp::new(1_200)?,
            DeepMapRunState::Running,
            Some(DeepMapPhase::Verifying),
            Some(DeepMapTargetKind::Symbol),
            Some(DeepMapSafeAction::VerifyEvidence),
            Some(module_id),
            Some(3),
            Some(5),
            false,
            DeepMapEventResult::Pending,
            None,
        )?;

        let dashboard = DeepMapRunDashboard::derive(&run, Some(event), Some(anchor));

        assert_eq!(dashboard.state(), DeepMapDashboardState::Running);
        assert_eq!(dashboard.freshness(), DeepMapDashboardFreshness::Current);
        assert_eq!(
            dashboard
                .phases()
                .iter()
                .map(|phase| phase.state())
                .collect::<Vec<_>>(),
            vec![
                DeepMapDashboardPhaseState::Completed,
                DeepMapDashboardPhaseState::Completed,
                DeepMapDashboardPhaseState::Completed,
                DeepMapDashboardPhaseState::Active,
                DeepMapDashboardPhaseState::Pending,
            ]
        );
        let activity = dashboard.activity().ok_or("activity missing")?;
        assert_eq!(activity.module_id(), Some(module_id));
        assert_eq!(activity.step_position(), Some(3));
        Ok(())
    }

    #[test]
    fn successful_and_stale_runs_do_not_expose_current_card_state()
    -> Result<(), Box<dyn std::error::Error>> {
        let run_anchor = anchor();
        let run = run_summary(
            DeepMapRunState::Succeeded,
            5,
            5,
            None,
            Some(DeepMapPublicationResult::Published),
            run_anchor,
        )?;
        let final_event = DeepMapJournalEvent::new(
            DeepMapEventSequence::new(4)?,
            DeepMapRunTimestamp::new(1_200)?,
            DeepMapRunState::Succeeded,
            Some(DeepMapPhase::Publishing),
            Some(DeepMapTargetKind::Project),
            Some(DeepMapSafeAction::PublishCards),
            None,
            None,
            None,
            true,
            DeepMapEventResult::Published,
            None,
        )?;
        let dashboard = DeepMapRunDashboard::derive(
            &run,
            Some(final_event),
            Some(DeepMapPublicationAnchor::new(
                IndexRunId::from_bytes([21; 32]),
                SnapshotId::from_bytes([22; 32]),
            )),
        );
        assert_eq!(dashboard.state(), DeepMapDashboardState::Completed);
        assert_eq!(dashboard.freshness(), DeepMapDashboardFreshness::Historical);
        assert_eq!(dashboard.activity(), None);
        assert!(
            dashboard
                .phases()
                .iter()
                .all(|phase| phase.state() == DeepMapDashboardPhaseState::Completed)
        );

        let module = DeepMapRunModuleSummary::new(ModuleId::from_bytes([9; 32]), 2, 2)?;
        assert_eq!(
            derive_deep_map_module_state(&run, None, module, false),
            DeepMapDashboardModuleState::Incomplete
        );
        assert_eq!(
            derive_deep_map_module_state(&run, None, module, true),
            DeepMapDashboardModuleState::Published
        );
        Ok(())
    }

    fn anchor() -> DeepMapPublicationAnchor {
        DeepMapPublicationAnchor::new(
            IndexRunId::from_bytes([3; 32]),
            SnapshotId::from_bytes([4; 32]),
        )
    }

    fn run_summary(
        state: DeepMapRunState,
        confirmed_steps: u64,
        total_steps: u64,
        diagnostic: Option<DeepMapDiagnosticCode>,
        publication_result: Option<DeepMapPublicationResult>,
        anchor: DeepMapPublicationAnchor,
    ) -> Result<DeepMapRunSummary, Box<dyn std::error::Error>> {
        let model = DeepMapModelDescriptor::from_stored_parts(
            ModelProfileReference::new(
                ModelProfileId::from_bytes([5; 32]),
                ModelProfileVersion::V1,
            ),
            "local".to_owned(),
            "mapper".to_owned(),
            32_000,
            4_096,
        )?;
        Ok(DeepMapRunSummary::new(
            DeepMapRunStart::new(
                DeepMapRunId::from_bytes([6; 32]),
                anchor,
                DeepMapMode::Standard,
                model,
                DeepMapRunTimestamp::new(1_000)?,
            ),
            state,
            DeepMapRunTimestamp::new(1_200)?,
            confirmed_steps,
            total_steps,
            diagnostic,
            false,
            DeepMapEventSequence::new(4)?,
            None,
            publication_result,
        )?)
    }
}
