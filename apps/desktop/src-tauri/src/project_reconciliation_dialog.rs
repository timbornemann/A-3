use a3_application::{
    ProjectPathDisplay, ProjectReconciliationChoice, ProjectReconciliationConfirmationError,
    ProjectReconciliationConfirmer, ProjectReconciliationEvidence, ProjectReconciliationProposal,
};
use std::fmt;
use tauri::AppHandle;
use tauri_plugin_dialog::{
    DialogExt, MessageDialogButtons, MessageDialogKind, MessageDialogResult,
};

const RECONCILE_LABEL: &str = "Lokale Daten übernehmen";
const OPEN_SEPARATELY_LABEL: &str = "Separat öffnen";
const CANCEL_LABEL: &str = "Abbrechen";

/// Native confirmation boundary for one evidence-backed worktree move proposal.
#[derive(Clone)]
pub(crate) struct NativeProjectReconciliationConfirmer {
    app: AppHandle,
}

impl NativeProjectReconciliationConfirmer {
    /// Binds native message-dialog access to the privileged desktop process.
    pub(crate) const fn new(app: AppHandle) -> Self {
        Self { app }
    }
}

impl fmt::Debug for NativeProjectReconciliationConfirmer {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("NativeProjectReconciliationConfirmer")
            .finish_non_exhaustive()
    }
}

impl ProjectReconciliationConfirmer for NativeProjectReconciliationConfirmer {
    fn choose_reconciliation(
        &self,
        proposal: &ProjectReconciliationProposal,
        new_root_display: &ProjectPathDisplay,
    ) -> Result<ProjectReconciliationChoice, ProjectReconciliationConfirmationError> {
        let evidence = match proposal.evidence() {
            ProjectReconciliationEvidence::RepositoryAndWorktreeAnchor => {
                "Git-Repository und Worktree-Metadaten stimmen überein."
            }
            ProjectReconciliationEvidence::RemoteAndWorktreeAnchor => {
                "Remote-Fingerprint und Worktree-Metadaten stimmen überein."
            }
        };
        let message = format!(
            "A^3 hat einen möglichen Worktree-Umzug erkannt.\n\nBisher: {}\nNeu: {}\n\n{}\n\nSollen die bisherigen lokalen A^3-Daten übernommen werden?",
            proposal.previous_root_display().as_str(),
            new_root_display.as_str(),
            evidence
        );
        let result = self
            .app
            .dialog()
            .message(message)
            .title("A^3 Worktree-Umzug")
            .kind(MessageDialogKind::Warning)
            .buttons(MessageDialogButtons::YesNoCancelCustom(
                RECONCILE_LABEL.to_owned(),
                OPEN_SEPARATELY_LABEL.to_owned(),
                CANCEL_LABEL.to_owned(),
            ))
            .blocking_show_with_result();

        choice_from_dialog_result(result)
    }
}

fn choice_from_dialog_result(
    result: MessageDialogResult,
) -> Result<ProjectReconciliationChoice, ProjectReconciliationConfirmationError> {
    match result {
        MessageDialogResult::Custom(label) if label == RECONCILE_LABEL => {
            Ok(ProjectReconciliationChoice::Reconcile)
        }
        MessageDialogResult::Custom(label) if label == OPEN_SEPARATELY_LABEL => {
            Ok(ProjectReconciliationChoice::OpenSeparately)
        }
        MessageDialogResult::Custom(label) if label == CANCEL_LABEL => {
            Ok(ProjectReconciliationChoice::Cancel)
        }
        MessageDialogResult::Yes => Ok(ProjectReconciliationChoice::Reconcile),
        MessageDialogResult::No => Ok(ProjectReconciliationChoice::OpenSeparately),
        MessageDialogResult::Cancel => Ok(ProjectReconciliationChoice::Cancel),
        MessageDialogResult::Ok | MessageDialogResult::Custom(_) => {
            Err(ProjectReconciliationConfirmationError::InvalidResponse)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{CANCEL_LABEL, OPEN_SEPARATELY_LABEL, RECONCILE_LABEL, choice_from_dialog_result};
    use a3_application::{ProjectReconciliationChoice, ProjectReconciliationConfirmationError};
    use tauri_plugin_dialog::MessageDialogResult;

    #[test]
    fn native_dialog_maps_only_the_offered_choices() {
        assert_eq!(
            choice_from_dialog_result(MessageDialogResult::Custom(RECONCILE_LABEL.to_owned())),
            Ok(ProjectReconciliationChoice::Reconcile)
        );
        assert_eq!(
            choice_from_dialog_result(MessageDialogResult::Custom(
                OPEN_SEPARATELY_LABEL.to_owned()
            )),
            Ok(ProjectReconciliationChoice::OpenSeparately)
        );
        assert_eq!(
            choice_from_dialog_result(MessageDialogResult::Custom(CANCEL_LABEL.to_owned())),
            Ok(ProjectReconciliationChoice::Cancel)
        );
        assert_eq!(
            choice_from_dialog_result(MessageDialogResult::Custom("unexpected".to_owned())),
            Err(ProjectReconciliationConfirmationError::InvalidResponse)
        );
        assert_eq!(
            choice_from_dialog_result(MessageDialogResult::Ok),
            Err(ProjectReconciliationConfirmationError::InvalidResponse)
        );
    }

    #[test]
    fn native_fallback_buttons_keep_the_same_semantics() {
        assert_eq!(
            choice_from_dialog_result(MessageDialogResult::Yes),
            Ok(ProjectReconciliationChoice::Reconcile)
        );
        assert_eq!(
            choice_from_dialog_result(MessageDialogResult::No),
            Ok(ProjectReconciliationChoice::OpenSeparately)
        );
        assert_eq!(
            choice_from_dialog_result(MessageDialogResult::Cancel),
            Ok(ProjectReconciliationChoice::Cancel)
        );
    }
}
