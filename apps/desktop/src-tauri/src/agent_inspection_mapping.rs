use crate::encode_hex;
use a3_application::{
    AcceptanceCriterionProofState, AgentChangeAttribution, AgentDiffContent, AgentDiffFile,
    AgentDiffFileOperation, AgentDiffHunk, AgentDiffLine, AgentDiffLineEnding, AgentDiffRow,
    AgentInspectionOverview, AgentPatchInspection, AgentProcessInspectionKind,
    AgentProcessInspectionSummary, AgentProcessLogPage, AgentProcessStreamSummary,
    TaskVerificationInspection, VerificationCommandInspection, VerificationEvidenceDetail,
    VerificationEvidenceInspection, VerificationProcessStreamInspection,
};
use a3_domain::{
    AcceptanceCriterionRequirement, DiffEvidenceSource, EvidenceFreshness,
    EvidenceFreshnessFailure, PatchLineEndings, PatchTextEncoding, ProcessOutputRedaction,
    ProcessStream, ProcessTermination, RepositoryPath, StepVerificationOutcome, TaskStepStaleCause,
    TaskStepStatus, TestCaseOutcome, VerificationEvidenceEvaluation, VerificationEvidenceFailure,
    VerificationMethod,
};
use a3_protocol::{
    AgentChangeAttributionV1, AgentCriterionInspectionV1, AgentCriterionProofStateV1,
    AgentCriterionProofV1, AgentCriterionRequirementV1, AgentDiffContentLineEndingsV1,
    AgentDiffContentV1, AgentDiffEncodingV1, AgentDiffEvidenceSourceV1, AgentDiffFileOperationV1,
    AgentDiffFileV1, AgentDiffHunkV1, AgentDiffLineEndingV1, AgentDiffLineV1, AgentDiffRowV1,
    AgentEvidenceFreshnessV1, AgentEvidenceStaleReasonV1, AgentInspectionLogPageV1,
    AgentInspectionPathV1, AgentInspectionProcessKindV1, AgentInspectionStreamV1,
    AgentInspectionV1, AgentPatchInspectionV1, AgentProcessInspectionV1, AgentProcessRedactionV1,
    AgentProcessStreamV1, AgentProcessTerminationV1, AgentStepStaleCauseV1,
    AgentStepVerificationOutcomeV1, AgentTestCaseOutcomeV1, AgentTestCaseV1,
    AgentVerificationAttemptV1, AgentVerificationCommandV1, AgentVerificationEvaluationV1,
    AgentVerificationEvidenceDetailV1, AgentVerificationEvidenceV1, AgentVerificationFailureV1,
    AgentVerificationInspectionV1, AgentVerificationMethodV1, AgentVerificationProcessStreamV1,
    AgentVerificationStepStatusV1, AgentVerificationStepV1,
};

pub(crate) fn map_agent_inspection_to_v1(
    volatile: Option<&AgentInspectionOverview>,
    verification: &TaskVerificationInspection,
) -> AgentInspectionV1 {
    AgentInspectionV1::new(
        volatile.map(|value| value.revision().get().to_string()),
        volatile
            .and_then(AgentInspectionOverview::patch)
            .map(map_patch_to_v1),
        volatile.map_or_else(Vec::new, |value| {
            value
                .processes()
                .iter()
                .copied()
                .map(map_process_to_v1)
                .collect()
        }),
        map_verification_to_v1(verification),
    )
}

pub(crate) fn map_agent_log_page_to_v1(page: &AgentProcessLogPage) -> AgentInspectionLogPageV1 {
    AgentInspectionLogPageV1::new(
        page.text().to_owned(),
        page.offset().get(),
        page.next_offset().map(|offset| offset.get()),
        page.page_truncated(),
        page.source_truncated(),
        page.redaction().map(map_redaction_to_v1),
    )
}

pub(crate) const fn map_inspection_stream_from_v1(value: AgentInspectionStreamV1) -> ProcessStream {
    match value {
        AgentInspectionStreamV1::Stdout => ProcessStream::Stdout,
        AgentInspectionStreamV1::Stderr => ProcessStream::Stderr,
    }
}

fn map_patch_to_v1(value: &AgentPatchInspection) -> AgentPatchInspectionV1 {
    let context = value.context();
    AgentPatchInspectionV1::new(
        encode_hex(&value.id().as_bytes()),
        context.run_id().to_string(),
        context.step_id().to_string(),
        context.verification_spec_id().to_string(),
        context.snapshot_id().to_string(),
        value.retained_bytes().to_string(),
        value.files().iter().map(map_diff_file_to_v1).collect(),
    )
}

fn map_diff_file_to_v1(value: &AgentDiffFile) -> AgentDiffFileV1 {
    AgentDiffFileV1::new(
        match value.operation() {
            AgentDiffFileOperation::Add => AgentDiffFileOperationV1::Add,
            AgentDiffFileOperation::Update => AgentDiffFileOperationV1::Update,
            AgentDiffFileOperation::Move => AgentDiffFileOperationV1::Move,
            AgentDiffFileOperation::Delete => AgentDiffFileOperationV1::Delete,
        },
        value.source_path().map(map_path_to_v1),
        value.target_path().map(map_path_to_v1),
        value.before().map(map_diff_content_to_v1),
        value.after().map(map_diff_content_to_v1),
        value.hunks().iter().map(map_diff_hunk_to_v1).collect(),
        value.added_lines(),
        value.removed_lines(),
        match value.attribution() {
            AgentChangeAttribution::ProposedAgent => AgentChangeAttributionV1::ProposedAgent,
            AgentChangeAttribution::AppliedAgent => AgentChangeAttributionV1::AppliedAgent,
            AgentChangeAttribution::External => AgentChangeAttributionV1::External,
            AgentChangeAttribution::Unattributed => AgentChangeAttributionV1::Unattributed,
        },
        value.content_truncated(),
    )
}

fn map_path_to_v1(value: &RepositoryPath) -> AgentInspectionPathV1 {
    AgentInspectionPathV1::new(
        encode_hex(value.as_bytes()),
        safe_path_display(value.as_bytes()),
    )
}

fn safe_path_display(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes)
        .chars()
        .flat_map(|character| character.escape_default())
        .collect()
}

fn map_diff_content_to_v1(value: &AgentDiffContent) -> AgentDiffContentV1 {
    AgentDiffContentV1::new(
        value.text().len().to_string(),
        value.total_bytes().to_string(),
        encode_hex(value.content_hash().as_bytes()),
        match value.encoding() {
            PatchTextEncoding::Utf8 => AgentDiffEncodingV1::Utf8,
            PatchTextEncoding::Utf8Bom => AgentDiffEncodingV1::Utf8Bom,
        },
        match value.line_endings() {
            PatchLineEndings::None => AgentDiffContentLineEndingsV1::None,
            PatchLineEndings::Lf => AgentDiffContentLineEndingsV1::Lf,
            PatchLineEndings::Crlf => AgentDiffContentLineEndingsV1::Crlf,
            PatchLineEndings::Cr => AgentDiffContentLineEndingsV1::Cr,
            PatchLineEndings::Mixed => AgentDiffContentLineEndingsV1::Mixed,
        },
        value.truncated(),
    )
}

fn map_diff_hunk_to_v1(value: &AgentDiffHunk) -> AgentDiffHunkV1 {
    AgentDiffHunkV1::new(
        value.before_start(),
        value.before_count(),
        value.after_start(),
        value.after_count(),
        value.rows().iter().map(map_diff_row_to_v1).collect(),
    )
}

fn map_diff_row_to_v1(value: &AgentDiffRow) -> AgentDiffRowV1 {
    match value {
        AgentDiffRow::Context {
            before_line,
            after_line,
            line,
        } => AgentDiffRowV1::Context {
            before_line: *before_line,
            after_line: *after_line,
            line: map_diff_line_to_v1(line),
        },
        AgentDiffRow::Removed { before_line, line } => AgentDiffRowV1::Removed {
            before_line: *before_line,
            line: map_diff_line_to_v1(line),
        },
        AgentDiffRow::Added { after_line, line } => AgentDiffRowV1::Added {
            after_line: *after_line,
            line: map_diff_line_to_v1(line),
        },
    }
}

fn map_diff_line_to_v1(value: &AgentDiffLine) -> AgentDiffLineV1 {
    AgentDiffLineV1::new(
        value.text().to_owned(),
        match value.ending() {
            AgentDiffLineEnding::Lf => AgentDiffLineEndingV1::Lf,
            AgentDiffLineEnding::Crlf => AgentDiffLineEndingV1::Crlf,
            AgentDiffLineEnding::Cr => AgentDiffLineEndingV1::Cr,
            AgentDiffLineEnding::None => AgentDiffLineEndingV1::None,
        },
    )
}

fn map_process_to_v1(value: AgentProcessInspectionSummary) -> AgentProcessInspectionV1 {
    let context = value.context();
    AgentProcessInspectionV1::new(
        encode_hex(&value.id().as_bytes()),
        context.run_id().to_string(),
        context.step_id().to_string(),
        context.verification_spec_id().to_string(),
        context.snapshot_id().to_string(),
        match value.kind() {
            AgentProcessInspectionKind::Test => AgentInspectionProcessKindV1::Test,
            AgentProcessInspectionKind::Build => AgentInspectionProcessKindV1::Build,
            AgentProcessInspectionKind::Diagnostic => AgentInspectionProcessKindV1::Diagnostic,
            AgentProcessInspectionKind::Lint => AgentInspectionProcessKindV1::Lint,
            AgentProcessInspectionKind::Format => AgentInspectionProcessKindV1::Format,
            AgentProcessInspectionKind::Command => AgentInspectionProcessKindV1::Command,
        },
        map_termination_to_v1(value.termination()),
        value.duration().as_millis().to_string(),
        map_stream_summary_to_v1(value.stdout()),
        map_stream_summary_to_v1(value.stderr()),
    )
}

fn map_stream_summary_to_v1(value: AgentProcessStreamSummary) -> AgentProcessStreamV1 {
    AgentProcessStreamV1::new(
        encode_hex(&value.digest().as_bytes()),
        value.observed_bytes().to_string(),
        value.retained_bytes().to_string(),
        value.retained_limit(),
        value.source_truncated(),
        value.redaction().map(map_redaction_to_v1),
    )
}

fn map_termination_to_v1(value: ProcessTermination) -> AgentProcessTerminationV1 {
    match value {
        ProcessTermination::Exited(exit) => AgentProcessTerminationV1::Exited {
            code: exit.code(),
            success: exit.success(),
        },
        ProcessTermination::TimedOut => AgentProcessTerminationV1::TimedOut,
        ProcessTermination::Cancelled => AgentProcessTerminationV1::Cancelled,
    }
}

const fn map_redaction_to_v1(value: ProcessOutputRedaction) -> AgentProcessRedactionV1 {
    match value {
        ProcessOutputRedaction::InvalidUtf8 => AgentProcessRedactionV1::InvalidUtf8,
        ProcessOutputRedaction::SecretCandidate => AgentProcessRedactionV1::SecretCandidate,
        ProcessOutputRedaction::UnsafeControl => AgentProcessRedactionV1::UnsafeControl,
    }
}

fn map_verification_to_v1(value: &TaskVerificationInspection) -> AgentVerificationInspectionV1 {
    AgentVerificationInspectionV1::new(
        value.goal_contract().revision().get(),
        value.task_ledger().ledger().revision().get(),
        value.task_ledger().version().get().to_string(),
        value.published_snapshot_id().to_string(),
        value
            .criteria()
            .iter()
            .map(|inspection| {
                let criterion = inspection.criterion();
                AgentCriterionInspectionV1::new(
                    criterion.id().to_string(),
                    criterion.statement().as_str().to_owned(),
                    match criterion.requirement() {
                        AcceptanceCriterionRequirement::Must => AgentCriterionRequirementV1::Must,
                        AcceptanceCriterionRequirement::Should => {
                            AgentCriterionRequirementV1::Should
                        }
                    },
                    match inspection.state() {
                        AcceptanceCriterionProofState::Proven => AgentCriterionProofStateV1::Proven,
                        AcceptanceCriterionProofState::Pending => {
                            AgentCriterionProofStateV1::Pending
                        }
                        AcceptanceCriterionProofState::Failed => AgentCriterionProofStateV1::Failed,
                        AcceptanceCriterionProofState::Stale => AgentCriterionProofStateV1::Stale,
                        AcceptanceCriterionProofState::Missing => {
                            AgentCriterionProofStateV1::Missing
                        }
                    },
                    inspection
                        .proofs()
                        .iter()
                        .map(|proof| {
                            AgentCriterionProofV1::new(
                                proof.step_id().to_string(),
                                proof
                                    .evidence_ids()
                                    .iter()
                                    .map(ToString::to_string)
                                    .collect(),
                            )
                        })
                        .collect(),
                )
            })
            .collect(),
        value
            .steps()
            .iter()
            .map(|step| {
                let definition = step.definition();
                AgentVerificationStepV1::new(
                    definition.id().to_string(),
                    definition.intended_outcome().as_str().to_owned(),
                    map_step_status_to_v1(step.status()),
                    step.stale_cause().map(map_stale_cause_to_v1),
                    definition.verification_spec().id().to_string(),
                    map_verification_method_to_v1(definition.verification_spec().method()),
                    step.attempts()
                        .iter()
                        .map(|attempt| {
                            AgentVerificationAttemptV1::new(
                                attempt.number().get(),
                                match attempt.outcome() {
                                    StepVerificationOutcome::Passed => {
                                        AgentStepVerificationOutcomeV1::Passed
                                    }
                                    StepVerificationOutcome::Failed { summary } => {
                                        AgentStepVerificationOutcomeV1::Failed {
                                            summary: summary.as_str().to_owned(),
                                        }
                                    }
                                },
                                attempt.evidence().iter().map(map_evidence_to_v1).collect(),
                            )
                        })
                        .collect(),
                )
            })
            .collect(),
    )
}

const fn map_step_status_to_v1(value: TaskStepStatus) -> AgentVerificationStepStatusV1 {
    match value {
        TaskStepStatus::Pending => AgentVerificationStepStatusV1::Pending,
        TaskStepStatus::Ready => AgentVerificationStepStatusV1::Ready,
        TaskStepStatus::InProgress => AgentVerificationStepStatusV1::InProgress,
        TaskStepStatus::Blocked => AgentVerificationStepStatusV1::Blocked,
        TaskStepStatus::AwaitingApproval => AgentVerificationStepStatusV1::AwaitingApproval,
        TaskStepStatus::Verifying => AgentVerificationStepStatusV1::Verifying,
        TaskStepStatus::Completed => AgentVerificationStepStatusV1::Completed,
        TaskStepStatus::Failed => AgentVerificationStepStatusV1::Failed,
        TaskStepStatus::Cancelled => AgentVerificationStepStatusV1::Cancelled,
        TaskStepStatus::Stale => AgentVerificationStepStatusV1::Stale,
    }
}

fn map_stale_cause_to_v1(value: &TaskStepStaleCause) -> AgentStepStaleCauseV1 {
    match value {
        TaskStepStaleCause::VerificationEvidence(ids) => {
            AgentStepStaleCauseV1::VerificationEvidence {
                evidence_ids: ids.iter().map(ToString::to_string).collect(),
            }
        }
        TaskStepStaleCause::Dependency(step_id) => AgentStepStaleCauseV1::Dependency {
            step_id: step_id.to_string(),
        },
    }
}

fn map_evidence_to_v1(value: &VerificationEvidenceInspection) -> AgentVerificationEvidenceV1 {
    AgentVerificationEvidenceV1::new(
        value.id().to_string(),
        value.run_id().to_string(),
        value.snapshot_id().to_string(),
        map_verification_method_to_v1(value.method()),
        match value.semantic() {
            VerificationEvidenceEvaluation::Passed => AgentVerificationEvaluationV1::Passed,
            VerificationEvidenceEvaluation::Failed(reason) => {
                AgentVerificationEvaluationV1::Failed {
                    reason: map_evidence_failure_to_v1(reason),
                }
            }
        },
        match value.freshness() {
            EvidenceFreshness::Fresh => AgentEvidenceFreshnessV1::Fresh,
            EvidenceFreshness::Stale(reason) => AgentEvidenceFreshnessV1::Stale {
                reason: match reason {
                    EvidenceFreshnessFailure::SnapshotChanged => {
                        AgentEvidenceStaleReasonV1::SnapshotChanged
                    }
                    EvidenceFreshnessFailure::DependencyChanged => {
                        AgentEvidenceStaleReasonV1::DependencyChanged
                    }
                },
            },
        },
        map_evidence_detail_to_v1(value.detail()),
    )
}

const fn map_verification_method_to_v1(value: VerificationMethod) -> AgentVerificationMethodV1 {
    match value {
        VerificationMethod::Command => AgentVerificationMethodV1::Command,
        VerificationMethod::Test => AgentVerificationMethodV1::Test,
        VerificationMethod::DiffInvariant => AgentVerificationMethodV1::DiffInvariant,
        VerificationMethod::Diagnostic => AgentVerificationMethodV1::Diagnostic,
        VerificationMethod::UserConfirm => AgentVerificationMethodV1::UserConfirm,
    }
}

const fn map_evidence_failure_to_v1(
    value: VerificationEvidenceFailure,
) -> AgentVerificationFailureV1 {
    match value {
        VerificationEvidenceFailure::LegacySpecification => {
            AgentVerificationFailureV1::LegacySpecification
        }
        VerificationEvidenceFailure::SpecificationMismatch => {
            AgentVerificationFailureV1::SpecificationMismatch
        }
        VerificationEvidenceFailure::EvidenceKindMismatch => {
            AgentVerificationFailureV1::EvidenceKindMismatch
        }
        VerificationEvidenceFailure::CommandMismatch => AgentVerificationFailureV1::CommandMismatch,
        VerificationEvidenceFailure::ProcessUnsuccessful => {
            AgentVerificationFailureV1::ProcessUnsuccessful
        }
        VerificationEvidenceFailure::MissingStructuredTestCases => {
            AgentVerificationFailureV1::MissingStructuredTestCases
        }
        VerificationEvidenceFailure::TooFewPassingTestCases => {
            AgentVerificationFailureV1::TooFewPassingTestCases
        }
        VerificationEvidenceFailure::SelectedTestCaseFailed => {
            AgentVerificationFailureV1::SelectedTestCaseFailed
        }
        VerificationEvidenceFailure::IncompleteChangeSet => {
            AgentVerificationFailureV1::IncompleteChangeSet
        }
        VerificationEvidenceFailure::DiffInvariantMismatch => {
            AgentVerificationFailureV1::DiffInvariantMismatch
        }
        VerificationEvidenceFailure::ErrorDiagnosticsPresent => {
            AgentVerificationFailureV1::ErrorDiagnosticsPresent
        }
        VerificationEvidenceFailure::WarningDiagnosticsPresent => {
            AgentVerificationFailureV1::WarningDiagnosticsPresent
        }
        VerificationEvidenceFailure::ConfirmationScopeMismatch => {
            AgentVerificationFailureV1::ConfirmationScopeMismatch
        }
    }
}

fn map_evidence_detail_to_v1(
    value: &VerificationEvidenceDetail,
) -> AgentVerificationEvidenceDetailV1 {
    match value {
        VerificationEvidenceDetail::Command(command) => {
            AgentVerificationEvidenceDetailV1::Command {
                command: map_verification_command_to_v1(command),
            }
        }
        VerificationEvidenceDetail::Test(test) => AgentVerificationEvidenceDetailV1::Test {
            command: map_verification_command_to_v1(test.command()),
            passed: test.passed().to_string(),
            failed: test.failed().to_string(),
            ignored: test.ignored().to_string(),
            cases: test
                .visible_cases()
                .iter()
                .map(|case| {
                    AgentTestCaseV1::new(
                        case.name().as_str().to_owned(),
                        match case.outcome() {
                            TestCaseOutcome::Passed => AgentTestCaseOutcomeV1::Passed,
                            TestCaseOutcome::Failed => AgentTestCaseOutcomeV1::Failed,
                            TestCaseOutcome::Ignored => AgentTestCaseOutcomeV1::Ignored,
                        },
                    )
                })
                .collect(),
            cases_truncated: test.cases_truncated(),
        },
        VerificationEvidenceDetail::Diff(diff) => AgentVerificationEvidenceDetailV1::Diff {
            source: match diff.source() {
                DiffEvidenceSource::Patch { .. } => AgentDiffEvidenceSourceV1::PatchChangeSet,
                DiffEvidenceSource::PublishedIndexes { .. } => {
                    AgentDiffEvidenceSourceV1::PublishedIndexes
                }
            },
            base_snapshot_id: diff.base_snapshot_id().to_string(),
            snapshot_id: diff.snapshot_id().to_string(),
            changed_paths: diff.changed_paths().iter().map(map_path_to_v1).collect(),
            complete: diff.complete(),
        },
        VerificationEvidenceDetail::Diagnostic(diagnostic) => {
            AgentVerificationEvidenceDetailV1::Diagnostic {
                command: map_verification_command_to_v1(diagnostic.command()),
                errors: diagnostic.errors().get(),
                warnings: diagnostic.warnings().get(),
            }
        }
        VerificationEvidenceDetail::UserConfirmation {
            scope_id,
            confirmed_at,
        } => AgentVerificationEvidenceDetailV1::UserConfirmation {
            scope_id: scope_id.to_string(),
            confirmed_at_unix_millis: confirmed_at.unix_millis().to_string(),
        },
    }
}

fn map_verification_command_to_v1(
    value: &VerificationCommandInspection,
) -> AgentVerificationCommandV1 {
    AgentVerificationCommandV1::new(
        value.command_id().to_string(),
        map_termination_to_v1(value.termination()),
        value.duration().as_millis().to_string(),
        map_verification_stream_to_v1(value.stdout()),
        map_verification_stream_to_v1(value.stderr()),
    )
}

fn map_verification_stream_to_v1(
    value: VerificationProcessStreamInspection,
) -> AgentVerificationProcessStreamV1 {
    AgentVerificationProcessStreamV1::new(
        encode_hex(&value.digest().as_bytes()),
        value.observed_bytes().to_string(),
        value.retained_limit(),
        value.truncated(),
        value.redaction().map(map_redaction_to_v1),
    )
}

#[cfg(test)]
mod tests {
    use super::safe_path_display;

    #[test]
    fn path_display_never_emits_raw_control_or_invalid_bytes() {
        assert_eq!(
            safe_path_display(b"src/line\n\xff.rs"),
            "src/line\\n\\u{fffd}.rs"
        );
    }
}
