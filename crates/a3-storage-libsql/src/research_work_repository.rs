//! Adapter-local bounded checkpoint encoding. Original source bytes are never retained.
use super::*;
use a3_domain::{
    ResearchAccessAttempt, ResearchAccessKind, ResearchAccessOutcome, ResearchQuestionCheckpoint,
    ResearchQuestionDraft, ResearchQuestionId, ResearchQuestionKind, ResearchQuestionPriority,
    ResearchQuestionStatus, ResearchResult, ResearchResultKind, ResearchResultSource,
    ResearchWorkState,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Checkpoint {
    version: u8,
    objective: String,
    revision: u32,
    questions: Vec<Question>,
    #[serde(default)]
    accesses: Vec<Access>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Access {
    question: u16,
    scope: [u8; 32],
    key: [u8; 32],
    kind: u8,
    starts: u16,
    outcome: Option<u8>,
}

fn encode_access(a: &ResearchAccessAttempt) -> Access {
    Access {
        question: a.question.get(),
        scope: *a.scope.as_bytes(),
        key: *a.key.as_bytes(),
        starts: a.starts,
        kind: match a.kind {
            ResearchAccessKind::Inspect => 1,
            ResearchAccessKind::LiteralSearch => 2,
            ResearchAccessKind::IndexSearch => 3,
            ResearchAccessKind::Directory => 4,
            ResearchAccessKind::Relations => 5,
            ResearchAccessKind::Flow => 6,
            ResearchAccessKind::Changes => 7,
            ResearchAccessKind::Diagnostics => 8,
            ResearchAccessKind::Dependencies => 9,
            ResearchAccessKind::Tests => 10,
            ResearchAccessKind::SecurityCandidates => 11,
        },
        outcome: a.outcome.map(|r| match r {
            ResearchAccessOutcome::Completed => 1,
            ResearchAccessOutcome::NoMatch => 2,
            ResearchAccessOutcome::Unresolved => 3,
            ResearchAccessOutcome::Limited => 4,
            ResearchAccessOutcome::Unavailable => 5,
        }),
    }
}

fn decode_access(a: Access) -> Result<ResearchAccessAttempt, AskResearchRepositoryError> {
    let invalid = || AskResearchRepositoryError::InvalidStoredData;
    Ok(ResearchAccessAttempt {
        question: ResearchQuestionId::new(a.question).map_err(|_| invalid())?,
        scope: ContentHash::from_bytes(a.scope),
        key: ContentHash::from_bytes(a.key),
        starts: a.starts,
        kind: match a.kind {
            1 => ResearchAccessKind::Inspect,
            2 => ResearchAccessKind::LiteralSearch,
            3 => ResearchAccessKind::IndexSearch,
            4 => ResearchAccessKind::Directory,
            5 => ResearchAccessKind::Relations,
            6 => ResearchAccessKind::Flow,
            7 => ResearchAccessKind::Changes,
            8 => ResearchAccessKind::Diagnostics,
            9 => ResearchAccessKind::Dependencies,
            10 => ResearchAccessKind::Tests,
            11 => ResearchAccessKind::SecurityCandidates,
            _ => return Err(invalid()),
        },
        outcome: a
            .outcome
            .map(|r| match r {
                1 => Ok(ResearchAccessOutcome::Completed),
                2 => Ok(ResearchAccessOutcome::NoMatch),
                3 => Ok(ResearchAccessOutcome::Unresolved),
                4 => Ok(ResearchAccessOutcome::Limited),
                5 => Ok(ResearchAccessOutcome::Unavailable),
                _ => Err(invalid()),
            })
            .transpose()?,
    })
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Question {
    request_fragment: String,
    outcome: String,
    priority: u8,
    kind: u8,
    dependencies: Vec<u16>,
    status: u8,
    result: Option<ResultRow>,
    attempts: Vec<[u8; 32]>,
    exclusions: Vec<[u8; 32]>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ResultRow {
    kind: u8,
    text: String,
    sources: Vec<Source>,
    boundary: Option<[u8; 32]>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Source {
    id: [u8; 32],
    path: Vec<u8>,
    hash: [u8; 32],
    range: [u32; 6],
}

pub(crate) fn encode(state: &ResearchWorkState) -> Result<String, AskResearchRepositoryError> {
    let questions = state
        .questions()
        .iter()
        .map(|q| Question {
            request_fragment: q.definition().request_fragment.clone(),
            outcome: q.definition().outcome.clone(),
            priority: match q.definition().priority {
                ResearchQuestionPriority::Required => 1,
                ResearchQuestionPriority::Supporting => 2,
                ResearchQuestionPriority::Optional => 3,
            },
            kind: match q.definition().kind {
                ResearchQuestionKind::Repository => 1,
                ResearchQuestionKind::Design => 2,
            },
            dependencies: q
                .definition()
                .dependencies
                .iter()
                .map(|id| id.get())
                .collect(),
            status: match q.status() {
                ResearchQuestionStatus::Open => 1,
                ResearchQuestionStatus::Active => 2,
                ResearchQuestionStatus::Answered => 3,
                ResearchQuestionStatus::Limited => 4,
                ResearchQuestionStatus::Blocked => 5,
                ResearchQuestionStatus::Stale => 6,
            },
            result: q.result().map(|r| ResultRow {
                kind: match r.kind() {
                    ResearchResultKind::Interpretation => 1,
                    ResearchResultKind::DesignDecision => 2,
                    ResearchResultKind::BoundedUnknown => 3,
                },
                text: r.text().to_owned(),
                boundary: r.boundary().map(|b| *b.as_bytes()),
                sources: r
                    .sources()
                    .iter()
                    .map(|s| Source {
                        id: *s.source_id.as_bytes(),
                        path: s.revision.path().as_bytes().to_vec(),
                        hash: *s.revision.content_hash().as_bytes(),
                        range: [
                            s.range.start_byte(),
                            s.range.end_byte(),
                            s.range.start_position().row(),
                            s.range.start_position().column(),
                            s.range.end_position().row(),
                            s.range.end_position().column(),
                        ],
                    })
                    .collect(),
            }),
            attempts: q.attempts().iter().map(|h| *h.as_bytes()).collect(),
            exclusions: q.exclusions().iter().map(|h| *h.as_bytes()).collect(),
        })
        .collect();
    let payload = serde_json::to_string(&Checkpoint {
        version: 1,
        objective: state.objective().to_owned(),
        revision: state.revision(),
        questions,
        accesses: state.accesses().iter().map(encode_access).collect(),
    })
    .map_err(|_| AskResearchRepositoryError::InvalidInput)?;
    if payload.len() > 524288 {
        return Err(AskResearchRepositoryError::InvalidInput);
    }
    Ok(payload)
}

pub(crate) fn decode(payload: &str) -> Result<ResearchWorkState, AskResearchRepositoryError> {
    let invalid = || AskResearchRepositoryError::InvalidStoredData;
    if payload.is_empty() || payload.len() > 524288 {
        return Err(invalid());
    }
    let row: Checkpoint = serde_json::from_str(payload).map_err(|_| invalid())?;
    if row.version != 1 || row.questions.len() > 32 || row.accesses.len() > 256 {
        return Err(invalid());
    }
    let mut questions = Vec::with_capacity(row.questions.len());
    for q in row.questions {
        if q.dependencies.len() > 31 || q.attempts.len() > 24 || q.exclusions.len() > 48 {
            return Err(invalid());
        }
        let result = if let Some(r) = q.result {
            if r.sources.len() > 32 {
                return Err(invalid());
            }
            let mut sources = Vec::with_capacity(r.sources.len());
            for s in r.sources {
                sources.push(ResearchResultSource {
                    source_id: AskResearchSourceId::from_bytes(s.id),
                    revision: FileRevision::new(
                        RepositoryPath::try_from_bytes(s.path).map_err(|_| invalid())?,
                        ContentHash::from_bytes(s.hash),
                    ),
                    range: SourceRange::new(
                        usize::try_from(s.range[0]).map_err(|_| invalid())?,
                        usize::try_from(s.range[1]).map_err(|_| invalid())?,
                        SourcePosition::new(s.range[2], s.range[3]),
                        SourcePosition::new(s.range[4], s.range[5]),
                    )
                    .map_err(|_| invalid())?,
                });
            }
            Some(
                ResearchResult::new(
                    match r.kind {
                        1 => ResearchResultKind::Interpretation,
                        2 => ResearchResultKind::DesignDecision,
                        3 => ResearchResultKind::BoundedUnknown,
                        _ => return Err(invalid()),
                    },
                    r.text,
                    sources,
                    r.boundary.map(ContentHash::from_bytes),
                )
                .map_err(|_| invalid())?,
            )
        } else {
            None
        };
        let attempts = q
            .attempts
            .iter()
            .copied()
            .map(ContentHash::from_bytes)
            .collect::<BTreeSet<_>>();
        let exclusions = q
            .exclusions
            .iter()
            .copied()
            .map(ContentHash::from_bytes)
            .collect::<BTreeSet<_>>();
        if attempts.len() != q.attempts.len() || exclusions.len() != q.exclusions.len() {
            return Err(invalid());
        }
        questions.push(ResearchQuestionCheckpoint {
            definition: ResearchQuestionDraft {
                request_fragment: q.request_fragment,
                outcome: q.outcome,
                priority: match q.priority {
                    1 => ResearchQuestionPriority::Required,
                    2 => ResearchQuestionPriority::Supporting,
                    3 => ResearchQuestionPriority::Optional,
                    _ => return Err(invalid()),
                },
                kind: match q.kind {
                    1 => ResearchQuestionKind::Repository,
                    2 => ResearchQuestionKind::Design,
                    _ => return Err(invalid()),
                },
                dependencies: q
                    .dependencies
                    .into_iter()
                    .map(|id| ResearchQuestionId::new(id).map_err(|_| invalid()))
                    .collect::<Result<_, _>>()?,
            },
            status: match q.status {
                1 => ResearchQuestionStatus::Open,
                2 => ResearchQuestionStatus::Active,
                3 => ResearchQuestionStatus::Answered,
                4 => ResearchQuestionStatus::Limited,
                5 => ResearchQuestionStatus::Blocked,
                6 => ResearchQuestionStatus::Stale,
                _ => return Err(invalid()),
            },
            result,
            attempts,
            exclusions,
        });
    }
    ResearchWorkState::restore(row.objective, row.revision, questions)
        .and_then(|state| {
            state.with_restored_accesses(
                row.accesses
                    .into_iter()
                    .map(decode_access)
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|_| a3_domain::ResearchWorkError::InvalidTransition)?,
            )
        })
        .map_err(|_| invalid())
}

pub(super) async fn insert(
    transaction: &Transaction,
    worktree: WorktreeId,
    event: &AskResearchEvent,
    state: &ResearchWorkState,
) -> Result<(), AskResearchRepositoryError> {
    // A caller cannot bind a different contract to an existing audit trail.
    if let Some(previous) = load(
        transaction,
        worktree,
        event.session_id(),
        event.user_sequence(),
    )
    .await?
        && (previous.objective() != state.objective()
            || !previous
                .questions()
                .iter()
                .map(|q| q.definition())
                .eq(state.questions().iter().map(|q| q.definition()))
            || state.revision() < previous.revision()
            || (state.revision() == previous.revision() && state != &previous))
    {
        return Err(AskResearchRepositoryError::Conflict);
    }
    for source in state
        .questions()
        .iter()
        .filter_map(|q| q.result())
        .flat_map(|r| r.sources())
    {
        // The source's navigation range can be narrower than a later safe inspectSource page.
        // Exact result ranges were admitted against actual original delivery, not this caption.
        let mut rows = transaction.query("SELECT path, content_hash FROM agent_work_trace_sources WHERE worktree_id=?1 AND session_id=?2 AND source_id=?3", params![worktree.as_bytes().to_vec(), event.session_id().as_bytes().to_vec(), source.source_id.as_bytes().to_vec()]).await.map_err(AskResearchRepositoryError::Read)?;
        let row = rows
            .next()
            .await
            .map_err(AskResearchRepositoryError::Read)?
            .ok_or(AskResearchRepositoryError::InvalidInput)?;
        if row
            .get::<Vec<u8>>(0)
            .map_err(AskResearchRepositoryError::Read)?
            != source.revision.path().as_bytes()
            || read_id(&row, 1)? != *source.revision.content_hash().as_bytes()
        {
            return Err(AskResearchRepositoryError::InvalidInput);
        }
        if rows
            .next()
            .await
            .map_err(AskResearchRepositoryError::Read)?
            .is_some()
        {
            return Err(AskResearchRepositoryError::InvalidStoredData);
        }
    }
    transaction.execute("INSERT INTO agent_research_work_checkpoints (worktree_id,session_id,user_sequence,event_sequence,work_revision,schema_version,payload) VALUES (?1,?2,?3,?4,?5,1,?6)", params![worktree.as_bytes().to_vec(), event.session_id().as_bytes().to_vec(), u64_i64(event.user_sequence().get())?, i64::from(event.sequence()), i64::from(state.revision()), encode(state)?]).await.map_err(AskResearchRepositoryError::Write)?;
    Ok(())
}

pub(super) async fn load(
    connection: &Connection,
    worktree: WorktreeId,
    session: AgentSessionId,
    sequence: AgentSessionSequence,
) -> Result<Option<ResearchWorkState>, AskResearchRepositoryError> {
    // Migration tests can inspect historical adapter snapshots before V36 is applied.
    if crate::migration::read_user_version(connection)
        .await
        .map_err(AskResearchRepositoryError::Read)?
        < 36
    {
        return Ok(None);
    }
    let mut rows = connection.query("SELECT payload,work_revision FROM agent_research_work_checkpoints WHERE worktree_id=?1 AND session_id=?2 AND user_sequence=?3 ORDER BY event_sequence DESC LIMIT 1", params![worktree.as_bytes().to_vec(), session.as_bytes().to_vec(), u64_i64(sequence.get())?]).await.map_err(AskResearchRepositoryError::Read)?;
    let Some(row) = rows
        .next()
        .await
        .map_err(AskResearchRepositoryError::Read)?
    else {
        return Ok(None);
    };
    let state = decode(&read_string(&row, 0)?)?;
    if state.revision() != read_u32(&row, 1)? {
        return Err(AskResearchRepositoryError::InvalidStoredData);
    }
    Ok(Some(state))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn failure(error: impl std::fmt::Debug) -> std::io::Error {
        std::io::Error::other(format!("{error:?}"))
    }
    #[test]
    fn checkpoint_and_event_commit_atomically_and_survive_the_event_window_and_reopen()
    -> Result<(), Box<dyn std::error::Error>> {
        crate::run_native_libsql_test(async {
            use a3_domain::*;
            let path = std::env::temp_dir().join(format!(
                "a3-research-work-reopen-{}-{}.db",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)?
                    .as_nanos()
            ));
            std::fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&path)?;
            let db = libsql::Builder::new_local(&path)
                .build()
                .await
                .map_err(failure)?;
            let connection = db.connect()?;
            connection
                .execute("PRAGMA foreign_keys=ON", ())
                .await
                .map_err(failure)?;
            let worktree = WorktreeId::from_bytes([2; 32]);
            crate::migration::migrate_knowledge(&connection, &[1; 32], worktree.as_bytes())
                .await
                .map_err(failure)?;
            let session_id = AgentSessionId::from_bytes([3; 32]);
            let sequence = AgentSessionSequence::FIRST;
            let time = AgentSessionTimestamp::from_unix_millis(1)?;
            let session = AgentSession::from_parts(
                session_id,
                AgentSessionRevision::new(1)?,
                AgentSessionTitle::try_from_string("Research checkpoint".to_owned())?,
                AgentSessionMode::Plan,
                AgentSessionState::Running,
                time,
                time,
                Some(sequence),
                None,
                None,
                false,
            );
            let entry = AgentSessionEntry::try_new(
                session_id,
                sequence,
                AgentSessionEntryKind::UserMessage,
                AgentSessionText::try_from_string("design import".to_owned())?,
                time,
                None,
                None,
                None,
            )?;
            crate::agent_session_repository::create(
                &connection,
                worktree,
                &session,
                Some(&entry),
                None,
            )
            .await
            .map_err(failure)?;
            let turn = AskResearchTurn::new_for_mode(
                session_id,
                sequence,
                IndexRunId::from_bytes([4; 32]),
                SnapshotId::from_bytes([5; 32]),
                time,
                AgentSessionMode::Plan,
                AgentResearchDepth::Standard,
            );
            let event = |ordinal| {
                AskResearchEvent::new(
                    session_id,
                    sequence,
                    ordinal,
                    AskResearchPhase::Evaluating,
                    AskResearchState::Running,
                    "Core checkpoint".to_owned(),
                    None,
                    AskResearchCompleteness::NotApplicable,
                    time,
                )
            };
            let mut work = ResearchWorkState::new(
                "design import".to_owned(),
                vec![ResearchQuestionDraft {
                    request_fragment: "design import".to_owned(),
                    outcome: "CLI interface".to_owned(),
                    priority: ResearchQuestionPriority::Required,
                    kind: ResearchQuestionKind::Design,
                    dependencies: vec![],
                }],
            )?;
            let access_scope = ContentHash::from_bytes([11; 32]);
            let access_key = ContentHash::from_bytes([12; 32]);
            work.begin_access(
                ResearchQuestionId::FIRST,
                access_scope,
                access_key,
                ResearchAccessKind::Directory,
            )?;
            super::super::begin(
                &connection,
                worktree,
                &turn,
                &event(1)?.with_work_state(work.clone()),
            )
            .await
            .map_err(failure)?;
            for ordinal in 2..=71 {
                super::super::append_event(&connection, worktree, &event(ordinal)?)
                    .await
                    .map_err(failure)?;
            }
            let detail = super::super::load_detail(&connection, worktree, session_id, sequence)
                .await
                .map_err(failure)?
                .ok_or("detail")?;
            assert_eq!(detail.events().len(), 64);
            assert_eq!(detail.work_state(), Some(&work));
            assert_eq!(
                detail.work_state().ok_or("work")?.accesses()[0].outcome,
                None
            );
            work.finish_access(
                ResearchQuestionId::FIRST,
                access_scope,
                access_key,
                ResearchAccessOutcome::NoMatch,
            )?;
            let mut invalid = work.clone();
            // Keep the question/result kind valid so the adapter must reject the
            // unowned original reference, not merely the earlier Domain type check.
            invalid.resolve(
                ResearchQuestionId::new(1)?,
                ResearchResult::new(
                    ResearchResultKind::DesignDecision,
                    "unsupported".to_owned(),
                    vec![ResearchResultSource {
                        source_id: AskResearchSourceId::from_bytes([9; 32]),
                        revision: FileRevision::new(
                            RepositoryPath::try_from_bytes(b"missing.py".to_vec())?,
                            ContentHash::from_bytes([8; 32]),
                        ),
                        range: SourceRange::new(
                            0,
                            1,
                            SourcePosition::new(0, 0),
                            SourcePosition::new(0, 1),
                        )?,
                    }],
                    None,
                )?,
            )?;
            assert!(
                super::super::append_event(
                    &connection,
                    worktree,
                    &event(72)?.with_work_state(invalid)
                )
                .await
                .is_err()
            );
            assert_eq!(
                super::super::load_detail(&connection, worktree, session_id, sequence)
                    .await
                    .map_err(failure)?
                    .ok_or("detail")?,
                detail
            );
            work.resolve(
                ResearchQuestionId::new(1)?,
                ResearchResult::new(
                    ResearchResultKind::DesignDecision,
                    "New CSV header: title".to_owned(),
                    vec![],
                    None,
                )?,
            )?;
            super::super::append_event(
                &connection,
                worktree,
                &event(72)?.with_work_state(work.clone()),
            )
            .await
            .map_err(failure)?;
            let reopened = db.connect()?;
            assert_eq!(
                load(&reopened, worktree, session_id, sequence)
                    .await
                    .map_err(failure)?,
                Some(work)
            );
            assert!(
                connection
                    .execute(
                        "UPDATE agent_research_work_checkpoints SET work_revision=99",
                        ()
                    )
                    .await
                    .is_err()
            );
            assert!(
                connection
                    .execute("DELETE FROM agent_research_work_checkpoints", ())
                    .await
                    .is_err()
            );
            let tombstone = AgentSession::from_parts(
                session_id,
                AgentSessionRevision::new(2)?,
                session.title().clone(),
                AgentSessionMode::Plan,
                AgentSessionState::Archived,
                time,
                time,
                None,
                None,
                None,
                true,
            );
            // User-authorized presentation deletion must cascade without disabling FK policy.
            crate::agent_session_repository::delete_presentation(
                &connection,
                worktree,
                session_id,
                session.revision(),
                &tombstone,
            )
            .await
            .map_err(failure)?;
            assert!(
                load(&connection, worktree, session_id, sequence)
                    .await
                    .map_err(failure)?
                    .is_none()
            );
            assert!(
                super::super::load_detail(&connection, worktree, session_id, sequence)
                    .await
                    .map_err(failure)?
                    .is_none()
            );
            drop(reopened);
            drop(connection);
            drop(db);
            std::fs::remove_file(path)?;
            Ok(())
        })
    }
    #[test]
    fn checkpoint_is_strict_and_preserves_questions_without_source_blobs()
    -> Result<(), Box<dyn std::error::Error>> {
        let state = ResearchWorkState::new(
            "design import".to_owned(),
            vec![ResearchQuestionDraft {
                request_fragment: "design import".to_owned(),
                outcome: "CLI interface".to_owned(),
                priority: ResearchQuestionPriority::Required,
                kind: ResearchQuestionKind::Design,
                dependencies: vec![],
            }],
        )?;
        let json = encode(&state).map_err(failure)?;
        assert_eq!(decode(&json).map_err(failure)?, state);
        let mut value: serde_json::Value = serde_json::from_str(&json)?;
        value.as_object_mut().ok_or("object")?.remove("accesses");
        assert_eq!(decode(&value.to_string()).map_err(failure)?, state);
        value["raw_reply"] = serde_json::json!("not permitted");
        assert!(decode(&value.to_string()).is_err());
        value.as_object_mut().ok_or("object")?.remove("raw_reply");
        value["questions"][0]["status"] = serde_json::json!(3);
        assert!(decode(&value.to_string()).is_err());
        Ok(())
    }
}
