use crate::index_trace_runtime::{now, path_display};
use crate::repository_index_manager::RepositoryIndexTraceControlError;
use a3_application::{
    IndexTraceFileFilter, IndexTraceFilePage, IndexTraceFileQuery, IndexTraceSnapshot,
    RetainedIndexTraces,
};
use a3_domain::{
    IndexTraceDiagnosticCode, IndexTraceFileChange, IndexTraceHashOutcome, IndexTraceId,
    IndexTraceParseOutcome, IndexTracePhase, IndexTracePhaseState, IndexTraceRevision,
    IndexTraceState, IndexTraceTrigger, RepositoryPath, WorktreeId,
};
use a3_protocol::{
    ControlIndexRunRequestV1, IndexRunControlActionV1, IndexRunControlResponseV1,
    IndexRunControlResultV1, IndexRunCountsV1, IndexRunDetailV1, IndexRunEventKindV1,
    IndexRunEventV1, IndexRunFailureCodeV1, IndexRunFailureV1, IndexRunFileChangeV1,
    IndexRunFileFilterV1, IndexRunFileV1, IndexRunFilesResponseV1, IndexRunHashOutcomeV1,
    IndexRunInspectionResponseV1, IndexRunParseOutcomeV1, IndexRunPathV1, IndexRunPhaseProgressV1,
    IndexRunPhaseStateV1, IndexRunPhaseV1, IndexRunStateV1, IndexRunTriggerV1,
    QueryIndexRunFilesRequestV1,
};

const CURSOR_BYTES: usize = 40;
const CURSOR_DOMAIN: &[u8] = b"a3.index-trace-file-cursor.v1\0";

pub(crate) fn map_inspection(retained: RetainedIndexTraces) -> IndexRunInspectionResponseV1 {
    let Some(current) = retained.current.as_ref() else {
        return IndexRunInspectionResponseV1::no_runs();
    };
    IndexRunInspectionResponseV1::available(
        map_snapshot(current),
        retained.previous.as_ref().map(map_snapshot),
        now().unix_millis().to_string(),
    )
}

pub(crate) fn map_file_query(
    request: &QueryIndexRunFilesRequestV1,
    worktree_id: WorktreeId,
) -> Result<IndexTraceFileQuery, ()> {
    let trace_id = decode_trace_id(request.run_ref())?;
    let revision = decode_revision(request.revision())?;
    let filter = map_file_filter(request.filter());
    let search = request
        .search()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned);
    let offset = request.cursor().map_or(Ok(0), |cursor| {
        decode_cursor(
            cursor,
            worktree_id,
            trace_id,
            revision,
            search.as_deref(),
            filter,
        )
    })?;
    IndexTraceFileQuery::new(trace_id, revision, search, filter, offset).map_err(|_| ())
}

pub(crate) fn map_file_page(
    worktree_id: WorktreeId,
    query: &IndexTraceFileQuery,
    page: IndexTraceFilePage,
) -> IndexRunFilesResponseV1 {
    let previous_cursor = (page.offset > 0).then(|| {
        encode_cursor(
            worktree_id,
            query.trace_id,
            query.revision,
            query.search.as_deref(),
            query.filter,
            page.offset
                .saturating_sub(u64::from(a3_application::INDEX_TRACE_FILE_PAGE_LIMIT)),
        )
    });
    let next_cursor = page.has_more.then(|| {
        encode_cursor(
            worktree_id,
            query.trace_id,
            query.revision,
            query.search.as_deref(),
            query.filter,
            page.offset
                .saturating_add(u64::from(a3_application::INDEX_TRACE_FILE_PAGE_LIMIT)),
        )
    });
    IndexRunFilesResponseV1::page(
        query.trace_id.to_string(),
        query.revision.get().to_string(),
        page.files.iter().map(map_file).collect(),
        page.total.to_string(),
        previous_cursor,
        next_cursor,
    )
}

pub(crate) fn map_control_request(
    request: &ControlIndexRunRequestV1,
) -> Result<(IndexTraceId, IndexTraceRevision, IndexRunControlActionV1), ()> {
    Ok((
        decode_trace_id(request.run_ref())?,
        decode_revision(request.revision())?,
        request.action(),
    ))
}

pub(crate) fn map_control_error(
    error: RepositoryIndexTraceControlError,
) -> IndexRunControlResponseV1 {
    let result = match error {
        RepositoryIndexTraceControlError::NoActiveProject => IndexRunControlResultV1::NoProject,
        RepositoryIndexTraceControlError::UnknownTrace => IndexRunControlResultV1::NotFound,
        RepositoryIndexTraceControlError::StaleRevision => IndexRunControlResultV1::StaleRevision,
        RepositoryIndexTraceControlError::InvalidState => IndexRunControlResultV1::InvalidState,
        RepositoryIndexTraceControlError::Busy => IndexRunControlResultV1::Busy,
        RepositoryIndexTraceControlError::CoordinatorStopped => {
            IndexRunControlResultV1::Unavailable
        }
    };
    IndexRunControlResponseV1::new(result)
}

fn map_snapshot(snapshot: &IndexTraceSnapshot) -> IndexRunDetailV1 {
    let summary = snapshot.summary();
    let end = match summary.ended_at() {
        Some(end) => end,
        None => now(),
    };
    let duration = end
        .unix_millis()
        .saturating_sub(summary.started_at().unix_millis());
    IndexRunDetailV1 {
        run_ref: summary.id().to_string(),
        revision: summary.revision().get().to_string(),
        state: map_state(summary.state()),
        trigger: map_trigger(summary.trigger()),
        started_at_unix_millis: summary.started_at().unix_millis().to_string(),
        ended_at_unix_millis: summary
            .ended_at()
            .map(|value| value.unix_millis().to_string()),
        last_activity_at_unix_millis: summary.last_activity_at().unix_millis().to_string(),
        duration_millis: duration.to_string(),
        current_phase: summary.current_phase().map(map_phase),
        current_file: summary.current_file().map(map_path),
        details_incomplete: summary.details_incomplete(),
        previous_publication_available: summary.previous_publication_available(),
        counts: map_counts(summary.counts()),
        phases: snapshot
            .phases()
            .iter()
            .map(|phase| IndexRunPhaseProgressV1 {
                phase: map_phase(phase.phase()),
                state: map_phase_state(phase.state()),
                started_at_unix_millis: phase
                    .started_at()
                    .map(|value| value.unix_millis().to_string()),
                ended_at_unix_millis: phase
                    .ended_at()
                    .map(|value| value.unix_millis().to_string()),
                completed: phase.completed().map(|value| value.to_string()),
                total: phase.total().map(|value| value.to_string()),
            })
            .collect(),
        events: snapshot
            .events()
            .iter()
            .map(|event| IndexRunEventV1 {
                revision: event.revision().get().to_string(),
                occurred_at_unix_millis: event.occurred_at().unix_millis().to_string(),
                kind: map_event_kind(event.kind()),
                phase: event.phase().map(map_phase),
                file: event.path().map(map_path),
                failure: event.diagnostic().map(map_failure),
            })
            .collect(),
        failure: summary.diagnostic().map(map_failure),
    }
}

fn map_counts(counts: a3_application::IndexTraceCounts) -> IndexRunCountsV1 {
    IndexRunCountsV1 {
        discovered: counts.discovered.to_string(),
        pending: counts.pending.to_string(),
        new_files: counts.new_files.to_string(),
        changed: counts.changed.to_string(),
        unchanged: counts.unchanged.to_string(),
        deleted: counts.deleted.to_string(),
        hashed: counts.hashed.to_string(),
        hash_reused: counts.hash_reused.to_string(),
        structural: counts.structural.to_string(),
        parse_reused: counts.parse_reused.to_string(),
        generic: counts.generic.to_string(),
        failed: counts.failed.to_string(),
    }
}

fn map_file(file: &a3_application::IndexTraceFileRecord) -> IndexRunFileV1 {
    IndexRunFileV1 {
        path: map_path(file.path()),
        change: match file.change() {
            IndexTraceFileChange::Pending => IndexRunFileChangeV1::Pending,
            IndexTraceFileChange::New => IndexRunFileChangeV1::New,
            IndexTraceFileChange::Changed => IndexRunFileChangeV1::Changed,
            IndexTraceFileChange::Unchanged => IndexRunFileChangeV1::Unchanged,
            IndexTraceFileChange::Deleted => IndexRunFileChangeV1::Deleted,
        },
        hash: match file.hash() {
            IndexTraceHashOutcome::Pending => IndexRunHashOutcomeV1::Pending,
            IndexTraceHashOutcome::Hashed => IndexRunHashOutcomeV1::Hashed,
            IndexTraceHashOutcome::Reused => IndexRunHashOutcomeV1::Reused,
            IndexTraceHashOutcome::NotApplicable => IndexRunHashOutcomeV1::NotApplicable,
        },
        parse: file.parse().map(|outcome| match outcome {
            IndexTraceParseOutcome::Structural => IndexRunParseOutcomeV1::Structural,
            IndexTraceParseOutcome::Reused => IndexRunParseOutcomeV1::Reused,
            IndexTraceParseOutcome::Generic => IndexRunParseOutcomeV1::Generic,
            IndexTraceParseOutcome::Failed => IndexRunParseOutcomeV1::Failed,
            IndexTraceParseOutcome::NotApplicable => IndexRunParseOutcomeV1::NotApplicable,
        }),
        failures: file
            .diagnostics()
            .iter()
            .copied()
            .map(map_failure)
            .collect(),
        failures_truncated: file.diagnostics_truncated(),
    }
}

fn map_path(path: &RepositoryPath) -> IndexRunPathV1 {
    let unbounded = String::from_utf8_lossy(path.as_bytes());
    IndexRunPathV1 {
        display: path_display(path),
        truncated: unbounded.chars().count() > 512,
    }
}

fn map_failure(code: IndexTraceDiagnosticCode) -> IndexRunFailureV1 {
    let (protocol_code, explanation, recovery) = match code {
        IndexTraceDiagnosticCode::Discovery => (
            IndexRunFailureCodeV1::Discovery,
            "Die Repository-Dateien konnten nicht vollständig ermittelt werden.",
            "Projektzugriff und Ignore-Regeln prüfen und den Lauf erneut versuchen.",
        ),
        IndexTraceDiagnosticCode::SourceUnavailable => (
            IndexRunFailureCodeV1::SourceUnavailable,
            "Eine berücksichtigte Datei war während der Verarbeitung nicht mehr lesbar.",
            "Dateizugriff prüfen; A^3 übernimmt die Änderung beim nächsten vollständigen Lauf.",
        ),
        IndexTraceDiagnosticCode::RevisionChanged => (
            IndexRunFailureCodeV1::RevisionChanged,
            "Eine Datei hat sich während des Lesens geändert.",
            "Änderungen kurz ruhen lassen und den Lauf erneut versuchen.",
        ),
        IndexTraceDiagnosticCode::Parse => (
            IndexRunFailureCodeV1::Parse,
            "Die Datei konnte nicht strukturell analysiert werden.",
            "Syntax prüfen; A^3 verwendet soweit möglich eine sichere generische Analyse.",
        ),
        IndexTraceDiagnosticCode::Link => (
            IndexRunFailureCodeV1::Link,
            "Beziehungen zwischen analysierten Elementen konnten nicht vollständig verknüpft werden.",
            "Den vollständigen Lauf erneut versuchen; der veröffentlichte Index bleibt erhalten.",
        ),
        IndexTraceDiagnosticCode::Rank => (
            IndexRunFailureCodeV1::Rank,
            "Die Indexsignale konnten nicht vollständig gewichtet werden.",
            "Den vollständigen Lauf erneut versuchen.",
        ),
        IndexTraceDiagnosticCode::Publish => (
            IndexRunFailureCodeV1::Publish,
            "Der neue Index konnte nicht atomar veröffentlicht werden.",
            "Lokalen Speicher prüfen und erneut versuchen; der vorherige Index bleibt nutzbar.",
        ),
        IndexTraceDiagnosticCode::ResourceLimit => (
            IndexRunFailureCodeV1::ResourceLimit,
            "Ein lokales Ressourcenlimit wurde erreicht.",
            "Andere ressourcenintensive Aufgaben schließen und erneut versuchen.",
        ),
        IndexTraceDiagnosticCode::Timeout => (
            IndexRunFailureCodeV1::Timeout,
            "Ein begrenzter Verarbeitungsschritt hat sein Zeitlimit erreicht.",
            "Den Lauf erneut versuchen und wiederholt betroffene Dateien prüfen.",
        ),
        IndexTraceDiagnosticCode::ProgressUnavailable => (
            IndexRunFailureCodeV1::ProgressUnavailable,
            "Für einen Verarbeitungsschritt fehlen Detailinformationen.",
            "Der Indexlauf kann trotzdem gültig sein; bei Bedarf erneut versuchen.",
        ),
        IndexTraceDiagnosticCode::JournalIncomplete => (
            IndexRunFailureCodeV1::JournalIncomplete,
            "Das Diagnosejournal konnte nicht vollständig gespeichert werden.",
            "Freien Speicher und Datenbankzugriff prüfen. Das Indexergebnis ist davon unabhängig.",
        ),
        IndexTraceDiagnosticCode::WorkerUnavailable => (
            IndexRunFailureCodeV1::WorkerUnavailable,
            "Der lokale Index-Worker wurde unerwartet beendet.",
            "A^3 neu öffnen oder den Lauf erneut versuchen.",
        ),
        IndexTraceDiagnosticCode::Interrupted => (
            IndexRunFailureCodeV1::Interrupted,
            "Ein früherer A^3-Prozess endete während dieses Laufs.",
            "Den Lauf erneut versuchen; der zuletzt veröffentlichte Index bleibt nutzbar.",
        ),
    };
    IndexRunFailureV1 {
        code: protocol_code,
        explanation: explanation.to_owned(),
        recovery: recovery.to_owned(),
    }
}

fn map_state(state: IndexTraceState) -> IndexRunStateV1 {
    match state {
        IndexTraceState::Queued => IndexRunStateV1::Queued,
        IndexTraceState::Running => IndexRunStateV1::Running,
        IndexTraceState::Cancelling => IndexRunStateV1::Cancelling,
        IndexTraceState::Succeeded => IndexRunStateV1::Succeeded,
        IndexTraceState::Failed => IndexRunStateV1::Failed,
        IndexTraceState::Cancelled => IndexRunStateV1::Cancelled,
        IndexTraceState::Interrupted => IndexRunStateV1::Interrupted,
    }
}
fn map_trigger(trigger: IndexTraceTrigger) -> IndexRunTriggerV1 {
    match trigger {
        IndexTraceTrigger::InitialObservation => IndexRunTriggerV1::InitialObservation,
        IndexTraceTrigger::FileChanges => IndexRunTriggerV1::FileChanges,
        IndexTraceTrigger::RecoveryRescan => IndexRunTriggerV1::RecoveryRescan,
        IndexTraceTrigger::ManualRetry => IndexRunTriggerV1::ManualRetry,
    }
}
fn map_phase(phase: IndexTracePhase) -> IndexRunPhaseV1 {
    match phase {
        IndexTracePhase::Discover => IndexRunPhaseV1::Discover,
        IndexTracePhase::Hash => IndexRunPhaseV1::Hash,
        IndexTracePhase::Parse => IndexRunPhaseV1::Parse,
        IndexTracePhase::Link => IndexRunPhaseV1::Link,
        IndexTracePhase::Rank => IndexRunPhaseV1::Rank,
        IndexTracePhase::Publish => IndexRunPhaseV1::Publish,
    }
}
fn map_phase_state(state: IndexTracePhaseState) -> IndexRunPhaseStateV1 {
    match state {
        IndexTracePhaseState::Pending => IndexRunPhaseStateV1::Pending,
        IndexTracePhaseState::Running => IndexRunPhaseStateV1::Running,
        IndexTracePhaseState::Succeeded => IndexRunPhaseStateV1::Succeeded,
        IndexTracePhaseState::Failed => IndexRunPhaseStateV1::Failed,
        IndexTracePhaseState::Cancelled => IndexRunPhaseStateV1::Cancelled,
    }
}
fn map_event_kind(kind: a3_application::IndexTraceEventKind) -> IndexRunEventKindV1 {
    match kind {
        a3_application::IndexTraceEventKind::Queued => IndexRunEventKindV1::Queued,
        a3_application::IndexTraceEventKind::Started => IndexRunEventKindV1::Started,
        a3_application::IndexTraceEventKind::PhaseStarted => IndexRunEventKindV1::PhaseStarted,
        a3_application::IndexTraceEventKind::Progress => IndexRunEventKindV1::Progress,
        a3_application::IndexTraceEventKind::FileObserved => IndexRunEventKindV1::FileObserved,
        a3_application::IndexTraceEventKind::Diagnostic => IndexRunEventKindV1::Diagnostic,
        a3_application::IndexTraceEventKind::CancellationRequested => {
            IndexRunEventKindV1::CancellationRequested
        }
        a3_application::IndexTraceEventKind::Succeeded => IndexRunEventKindV1::Succeeded,
        a3_application::IndexTraceEventKind::Failed => IndexRunEventKindV1::Failed,
        a3_application::IndexTraceEventKind::Cancelled => IndexRunEventKindV1::Cancelled,
        a3_application::IndexTraceEventKind::Interrupted => IndexRunEventKindV1::Interrupted,
    }
}

fn map_file_filter(filter: IndexRunFileFilterV1) -> IndexTraceFileFilter {
    match filter {
        IndexRunFileFilterV1::All => IndexTraceFileFilter::All,
        IndexRunFileFilterV1::New => IndexTraceFileFilter::New,
        IndexRunFileFilterV1::Changed => IndexTraceFileFilter::Changed,
        IndexRunFileFilterV1::Unchanged => IndexTraceFileFilter::Unchanged,
        IndexRunFileFilterV1::Deleted => IndexTraceFileFilter::Deleted,
        IndexRunFileFilterV1::Hashed => IndexTraceFileFilter::Hashed,
        IndexRunFileFilterV1::HashReused => IndexTraceFileFilter::HashReused,
        IndexRunFileFilterV1::Structural => IndexTraceFileFilter::Structural,
        IndexRunFileFilterV1::ParseReused => IndexTraceFileFilter::ParseReused,
        IndexRunFileFilterV1::Generic => IndexTraceFileFilter::Generic,
        IndexRunFileFilterV1::Failed => IndexTraceFileFilter::Failed,
    }
}

fn decode_trace_id(value: &str) -> Result<IndexTraceId, ()> {
    let bytes = decode_hex(value)?;
    Ok(IndexTraceId::from_bytes(bytes))
}

fn decode_revision(value: &str) -> Result<IndexTraceRevision, ()> {
    if value.is_empty()
        || value == "0"
        || (value.len() > 1 && value.starts_with('0'))
        || !value.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(());
    }
    IndexTraceRevision::new(value.parse().map_err(|_| ())?).map_err(|_| ())
}

fn decode_hex(value: &str) -> Result<[u8; 32], ()> {
    if value.len() != 64 {
        return Err(());
    }
    let mut result = [0_u8; 32];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        let high = hex_nibble(pair[0]).ok_or(())?;
        let low = hex_nibble(pair[1]).ok_or(())?;
        result[index] = (high << 4) | low;
    }
    Ok(result)
}

const fn hex_nibble(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        _ => None,
    }
}

fn encode_cursor(
    worktree_id: WorktreeId,
    trace_id: IndexTraceId,
    revision: IndexTraceRevision,
    search: Option<&str>,
    filter: IndexTraceFileFilter,
    offset: u64,
) -> String {
    let mut bytes = [0_u8; CURSOR_BYTES];
    bytes[..8].copy_from_slice(&offset.to_be_bytes());
    bytes[8..].copy_from_slice(
        cursor_mac(worktree_id, trace_id, revision, search, filter, offset).as_bytes(),
    );
    encode_hex_bytes(&bytes)
}

fn decode_cursor(
    value: &str,
    worktree_id: WorktreeId,
    trace_id: IndexTraceId,
    revision: IndexTraceRevision,
    search: Option<&str>,
    filter: IndexTraceFileFilter,
) -> Result<u64, ()> {
    if value.len() != CURSOR_BYTES * 2 {
        return Err(());
    }
    let mut bytes = [0_u8; CURSOR_BYTES];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        bytes[index] = (hex_nibble(pair[0]).ok_or(())? << 4) | hex_nibble(pair[1]).ok_or(())?;
    }
    let offset = u64::from_be_bytes(bytes[..8].try_into().map_err(|_| ())?);
    let expected = cursor_mac(worktree_id, trace_id, revision, search, filter, offset);
    if bytes[8..] != expected.as_bytes()[..] {
        return Err(());
    }
    Ok(offset)
}

fn cursor_mac(
    worktree_id: WorktreeId,
    trace_id: IndexTraceId,
    revision: IndexTraceRevision,
    search: Option<&str>,
    filter: IndexTraceFileFilter,
    offset: u64,
) -> blake3::Hash {
    let mut hasher = blake3::Hasher::new();
    hasher.update(CURSOR_DOMAIN);
    hasher.update(worktree_id.as_bytes());
    hasher.update(trace_id.as_bytes());
    hasher.update(&revision.get().to_be_bytes());
    hasher.update(&[filter_code(filter)]);
    hasher.update(&offset.to_be_bytes());
    if let Some(search) = search {
        hasher.update(search.as_bytes());
    }
    hasher.finalize()
}

const fn filter_code(filter: IndexTraceFileFilter) -> u8 {
    match filter {
        IndexTraceFileFilter::All => 0,
        IndexTraceFileFilter::New => 1,
        IndexTraceFileFilter::Changed => 2,
        IndexTraceFileFilter::Unchanged => 3,
        IndexTraceFileFilter::Deleted => 4,
        IndexTraceFileFilter::Hashed => 5,
        IndexTraceFileFilter::HashReused => 6,
        IndexTraceFileFilter::Structural => 7,
        IndexTraceFileFilter::ParseReused => 8,
        IndexTraceFileFilter::Generic => 9,
        IndexTraceFileFilter::Failed => 10,
    }
}

fn encode_hex_bytes(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut result = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        result.push(char::from(HEX[usize::from(byte >> 4)]));
        result.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    result
}

#[cfg(test)]
mod tests {
    use super::{decode_cursor, encode_cursor, map_inspection};
    use a3_application::{
        IndexTraceCounts, IndexTraceEvent, IndexTraceEventKind, IndexTraceFileFilter,
        IndexTraceFileRecord, IndexTracePhaseProgress, IndexTraceRunSummary, IndexTraceSnapshot,
        RetainedIndexTraces,
    };
    use a3_domain::{
        IndexTraceFileChange, IndexTraceHashOutcome, IndexTraceId, IndexTracePhase,
        IndexTracePhaseState, IndexTraceRevision, IndexTraceState, IndexTraceTimestamp,
        IndexTraceTrigger, RepositoryPath, WorktreeId,
    };

    #[test]
    fn cursor_is_bound_to_query_and_rejects_tampering() -> Result<(), Box<dyn std::error::Error>> {
        let worktree = WorktreeId::from_bytes([1; 32]);
        let trace = IndexTraceId::from_bytes([2; 32]);
        let revision = IndexTraceRevision::new(3)?;
        let cursor = encode_cursor(
            worktree,
            trace,
            revision,
            Some("src"),
            IndexTraceFileFilter::All,
            100,
        );
        assert_eq!(
            decode_cursor(
                &cursor,
                worktree,
                trace,
                revision,
                Some("src"),
                IndexTraceFileFilter::All
            ),
            Ok(100)
        );
        assert!(
            decode_cursor(
                &cursor,
                worktree,
                trace,
                revision,
                Some("test"),
                IndexTraceFileFilter::All
            )
            .is_err()
        );
        let mut tampered = cursor;
        tampered.replace_range(0..1, "f");
        assert!(
            decode_cursor(
                &tampered,
                worktree,
                trace,
                revision,
                Some("src"),
                IndexTraceFileFilter::All
            )
            .is_err()
        );
        Ok(())
    }

    #[test]
    fn active_parse_trace_serializes_to_the_exact_webview_contract()
    -> Result<(), Box<dyn std::error::Error>> {
        let started = IndexTraceTimestamp::new(1_000)?;
        let last_activity = IndexTraceTimestamp::new(2_000)?;
        let path = RepositoryPath::try_from_bytes(b"src/main.rs".to_vec())?;
        let revision = IndexTraceRevision::new(9)?;
        let summary = IndexTraceRunSummary::restored(
            IndexTraceId::from_bytes([0xaa; 32]),
            revision,
            IndexTraceState::Running,
            IndexTraceTrigger::FileChanges,
            started,
            None,
            last_activity,
            Some(IndexTracePhase::Parse),
            Some(path.clone()),
            None,
            false,
            true,
            IndexTraceCounts {
                discovered: 2,
                pending: 0,
                new_files: 1,
                changed: 0,
                unchanged: 1,
                deleted: 0,
                hashed: 1,
                hash_reused: 1,
                structural: 0,
                parse_reused: 0,
                generic: 0,
                failed: 0,
            },
        )?;
        let phases = vec![
            IndexTracePhaseProgress::new(
                IndexTracePhase::Discover,
                IndexTracePhaseState::Succeeded,
                Some(started),
                Some(IndexTraceTimestamp::new(1_200)?),
                Some(2),
                Some(2),
            )?,
            IndexTracePhaseProgress::new(
                IndexTracePhase::Hash,
                IndexTracePhaseState::Succeeded,
                Some(IndexTraceTimestamp::new(1_200)?),
                Some(IndexTraceTimestamp::new(1_500)?),
                Some(2),
                Some(2),
            )?,
            IndexTracePhaseProgress::new(
                IndexTracePhase::Parse,
                IndexTracePhaseState::Running,
                Some(IndexTraceTimestamp::new(1_500)?),
                None,
                Some(1),
                Some(2),
            )?,
            IndexTracePhaseProgress::pending_all().remove(3),
            IndexTracePhaseProgress::pending_all().remove(4),
            IndexTracePhaseProgress::pending_all().remove(5),
        ];
        let snapshot = IndexTraceSnapshot::new(
            summary,
            phases,
            vec![IndexTraceEvent::new(
                revision,
                last_activity,
                IndexTraceEventKind::FileObserved,
                Some(IndexTracePhase::Parse),
                Some(path.clone()),
                None,
            )],
            vec![
                IndexTraceFileRecord::new(
                    path,
                    IndexTraceFileChange::New,
                    IndexTraceHashOutcome::Hashed,
                    None,
                    Vec::new(),
                    false,
                )?,
                IndexTraceFileRecord::new(
                    RepositoryPath::try_from_bytes(b"src/lib.rs".to_vec())?,
                    IndexTraceFileChange::Unchanged,
                    IndexTraceHashOutcome::Reused,
                    None,
                    Vec::new(),
                    false,
                )?,
            ],
        )?;

        let value = serde_json::to_value(map_inspection(RetainedIndexTraces {
            current: Some(snapshot),
            previous: None,
        }))?;
        assert_eq!(value["protocolVersion"], 1);
        assert_eq!(value["result"]["status"], "available");
        assert!(
            value["result"]["serverTimeUnixMillis"]
                .as_str()
                .is_some_and(|timestamp| !timestamp.is_empty())
        );
        assert_eq!(value["result"]["stallThresholdSeconds"], 60);
        assert!(value["result"].get("server_time_unix_millis").is_none());
        assert!(value["result"].get("stall_threshold_seconds").is_none());
        assert_eq!(value["result"]["current"]["currentPhase"], "parse");
        assert_eq!(value["result"]["current"]["counts"]["discovered"], "2");
        assert_eq!(
            value["result"]["current"]["currentFile"]["display"],
            "src/main.rs"
        );
        assert_eq!(
            value["result"]["current"]["phases"]
                .as_array()
                .map(Vec::len),
            Some(6)
        );
        let _roundtrip =
            serde_json::from_value::<a3_protocol::IndexRunInspectionResponseV1>(value)?;
        Ok(())
    }
}
