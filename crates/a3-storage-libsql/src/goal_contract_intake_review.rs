//! Append-only intake proposals in the existing Goal transaction and source lifecycle.
use super::{
    GoalContractRepositoryError as Error, classify_unexpected_constraint, close_read_transaction,
    close_write_transaction, load_revision_from_transaction, parse_requirement, read_i64, read_id,
    read_optional_text, read_text, read_timestamp, requirement_text, sequence_to_i64,
    timestamp_to_i64,
};
use crate::{loopback_http_codec as http, python_translation_codec as guard};
use a3_application::{GoalIntakeLoadResult, GoalIntakeStoreVersion};
use a3_domain::{
    CriterionEvidenceRequirement, GoalContract, GoalContractRevision, GoalIntakeFinding,
    GoalIntakeFindingKind, GoalIntakeOpenQuestion, GoalIntakeQuestionDraft, GoalIntakeQuestionId,
    GoalIntakeQuestionKind, GoalIntakeReview, GoalIntakeReviewBatch, GoalIntakeReviewRevision,
    GoalOriginalDigest, GoalOriginalRange, PendingGoalIntake, RepositoryPath, TaskId,
    TestCaseSelector, WorktreeId,
};
use libsql::{Connection, Transaction, TransactionBehavior, Value, params, params_from_iter};

const MAX_REVIEW_HTTP_RECIPE_BYTES: usize = 65_536;

pub(crate) async fn load(
    connection: &Connection,
    worktree: WorktreeId,
    task: TaskId,
) -> Result<Option<GoalIntakeReview>, Error> {
    let tx = connection
        .transaction_with_behavior(TransactionBehavior::Deferred)
        .await
        .map_err(Error::Begin)?;
    let result = async {
        require_schema(&tx).await?;
        if preflight(&tx, task).await? == 0 {
            return Ok(None);
        }
        // After promotion, this is a read view over the actual initial Goal original,
        // never a second Pending row, fabricated Goal or retained raw-source copy.
        let pending = match super::intake::load_in_transaction(&tx, worktree, task).await? {
            Some(GoalIntakeLoadResult::Pending(stored)) => stored.pending().clone(),
            Some(GoalIntakeLoadResult::Materialized(_)) => {
                let goal = load_revision_from_transaction(
                    &tx,
                    worktree,
                    task,
                    GoalContractRevision::INITIAL,
                )
                .await?
                .ok_or(Error::InvalidStoredData)?;
                let original = goal.original().cloned().ok_or(Error::InvalidStoredData)?;
                PendingGoalIntake::new(original, goal.created_at())
                    .map_err(|_| Error::InvalidStoredData)?
            }
            None => return Err(Error::InvalidStoredData),
        };
        load_for_capture(&tx, worktree, &pending).await
    }
    .await;
    close_read_transaction(tx, result).await
}

pub(crate) async fn append(
    connection: &Connection,
    worktree: WorktreeId,
    task: TaskId,
    expected_capture: GoalIntakeStoreVersion,
    expected_review: Option<GoalIntakeReviewRevision>,
    review: &GoalIntakeReview,
) -> Result<GoalIntakeReviewRevision, Error> {
    let tx = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .map_err(Error::Begin)?;
    let result = append_in_transaction(
        &tx,
        worktree,
        task,
        expected_capture,
        expected_review,
        review,
    )
    .await;
    close_write_transaction(tx, result).await
}

pub(super) async fn append_in_transaction(
    tx: &Transaction,
    worktree: WorktreeId,
    task: TaskId,
    expected_capture: GoalIntakeStoreVersion,
    expected_review: Option<GoalIntakeReviewRevision>,
    review: &GoalIntakeReview,
) -> Result<GoalIntakeReviewRevision, Error> {
    require_schema(tx).await?;
    let Some(GoalIntakeLoadResult::Pending(stored)) =
        super::intake::load_in_transaction(tx, worktree, task).await?
    else {
        return Err(Error::IntakeNotPending);
    };
    if stored.version() != expected_capture {
        return Err(Error::IntakeVersionConflict);
    }
    review
        .validate_capture(stored.pending())
        .map_err(|_| Error::InvalidIntakeReview)?;
    let restored = GoalIntakeReview::restore(stored.pending(), review.batches().to_vec())
        .map_err(|_| Error::InvalidIntakeReview)?;
    if restored != *review {
        return Err(Error::InvalidIntakeReview);
    }
    let previous = load_for_capture(tx, worktree, stored.pending()).await?;
    if previous.as_ref().map(GoalIntakeReview::revision) != expected_review {
        return Err(Error::IntakeReviewVersionConflict);
    }
    match previous {
        Some(previous) => review
            .validate_successor_of(&previous)
            .map_err(|_| Error::InvalidIntakeReview)?,
        None if review.revision().get() == 1 => {}
        None => return Err(Error::InvalidIntakeReview),
    }
    let batch = review.batches().last().ok_or(Error::InvalidIntakeReview)?;
    write_batch(tx, worktree, review, batch).await?;
    if guard::supported(tx).await.map_err(Error::Read)? {
        preflight_guard_bytes(tx, task).await?;
    }
    Ok(review.revision())
}

pub(super) async fn validate_promotion_if_present(
    tx: &Transaction,
    worktree: WorktreeId,
    pending: &PendingGoalIntake,
    goal: &GoalContract,
) -> Result<bool, Error> {
    if crate::migration::read_user_version(tx)
        .await
        .map_err(Error::Read)?
        < 48
    {
        return Ok(false);
    }
    let Some(review) = load_for_capture(tx, worktree, pending).await? else {
        return Ok(false);
    };
    review
        .validate_promotion(pending, goal)
        .map_err(|_| Error::InvalidIntakePromotion)?;
    Ok(true)
}
async fn require_schema(tx: &Transaction) -> Result<(), Error> {
    if crate::migration::read_user_version(tx)
        .await
        .map_err(Error::Read)?
        < 48
    {
        return Err(Error::ReviewSchemaUnsupported);
    }
    Ok(())
}

/// Count and byte aggregates are checked before loading any proposed text/path payload.
async fn preflight(tx: &Transaction, task: TaskId) -> Result<usize, Error> {
    let mut rows=tx.query("SELECT
      (SELECT count(*) FROM goal_intake_review_batches WHERE task_id=?1),
      (SELECT count(*) FROM goal_intake_review_findings WHERE task_id=?1),
      (SELECT count(*) FROM goal_intake_review_questions WHERE task_id=?1),
      (SELECT count(*) FROM goal_intake_review_diff_paths WHERE task_id=?1),
      (SELECT count(*) FROM goal_intake_review_question_ranges WHERE task_id=?1),
      (SELECT coalesce(sum(length(CAST(statement AS BLOB))),0) FROM goal_intake_review_findings WHERE task_id=?1)
        +(SELECT coalesce(sum(length(CAST(question AS BLOB))),0) FROM goal_intake_review_questions WHERE task_id=?1),
      (SELECT coalesce(sum(length(path)),0) FROM goal_intake_review_diff_paths WHERE task_id=?1),
      EXISTS(SELECT 1 FROM goal_intake_review_findings f LEFT JOIN goal_intake_review_batches b
        ON b.task_id=f.task_id AND b.review_sequence=f.review_sequence WHERE f.task_id=?1 AND b.task_id IS NULL)
      OR EXISTS(SELECT 1 FROM goal_intake_review_questions q LEFT JOIN goal_intake_review_batches b
        ON b.task_id=q.task_id AND b.review_sequence=q.review_sequence WHERE q.task_id=?1 AND b.task_id IS NULL)
      OR EXISTS(SELECT 1 FROM goal_intake_review_diff_paths p LEFT JOIN goal_intake_review_findings f
        ON f.task_id=p.task_id AND f.review_sequence=p.review_sequence AND f.finding_sequence=p.finding_sequence
        WHERE p.task_id=?1 AND f.task_id IS NULL)
      OR EXISTS(SELECT 1 FROM goal_intake_review_question_ranges r LEFT JOIN goal_intake_review_questions q
        ON q.task_id=r.task_id AND q.review_sequence=r.review_sequence AND q.question_id=r.question_id
        WHERE r.task_id=?1 AND q.task_id IS NULL)",params![task.as_bytes().to_vec()]).await.map_err(Error::Read)?;
    let row = rows
        .next()
        .await
        .map_err(Error::Read)?
        .ok_or(Error::InvalidStoredData)?;
    let mut values = [0_usize; 7];
    for (index, value) in values.iter_mut().enumerate() {
        *value = usize::try_from(read_i64(
            &row,
            i32::try_from(index).map_err(|_| Error::ResourceLimit)?,
        )?)
        .map_err(|_| Error::InvalidStoredData)?;
    }
    if values[0] > 128
        || values[1] > 256
        || values[2] > 32
        || values[3] > 4096
        || values[4] > 256
        || values[5] > 65536
        || values[6] > 65536
    {
        return Err(Error::ResourceLimit);
    }
    if read_i64(&row, 7)? != 0 || rows.next().await.map_err(Error::Read)?.is_some() {
        return Err(Error::InvalidStoredData);
    }
    if guard::supported(tx).await.map_err(Error::Read)? {
        preflight_guard_bytes(tx, task).await?;
    }
    if http::supported(tx).await.map_err(Error::Read)? {
        preflight_http_bytes(tx, task, 0).await?;
    }
    Ok(values[0])
}

async fn preflight_guard_bytes(tx: &Transaction, task: TaskId) -> Result<(), Error> {
    let mut rows = tx
        .query(
            "SELECT coalesce(sum(length(python_integer_translation)),0)
         FROM goal_intake_review_findings WHERE task_id=?1",
            params![task.as_bytes().to_vec()],
        )
        .await
        .map_err(Error::Read)?;
    let row = rows
        .next()
        .await
        .map_err(Error::Read)?
        .ok_or(Error::InvalidStoredData)?;
    if !(0..=1_048_576).contains(&read_i64(&row, 0)?) {
        return Err(Error::ResourceLimit);
    }
    Ok(())
}

async fn preflight_http_bytes(
    tx: &Transaction,
    task: TaskId,
    additional_bytes: usize,
) -> Result<(), Error> {
    let mut rows = tx
        .query(
            "SELECT coalesce(sum(length(loopback_http_recipe)),0)
         FROM goal_intake_review_findings WHERE task_id=?1",
            params![task.as_bytes().to_vec()],
        )
        .await
        .map_err(Error::Read)?;
    let row = rows
        .next()
        .await
        .map_err(Error::Read)?
        .ok_or(Error::InvalidStoredData)?;
    let stored = usize::try_from(read_i64(&row, 0)?).map_err(|_| Error::InvalidStoredData)?;
    if stored
        .checked_add(additional_bytes)
        .is_none_or(|bytes| bytes > MAX_REVIEW_HTTP_RECIPE_BYTES)
    {
        return Err(Error::ResourceLimit);
    }
    Ok(())
}

pub(super) async fn load_for_capture(
    tx: &Transaction,
    worktree: WorktreeId,
    pending: &PendingGoalIntake,
) -> Result<Option<GoalIntakeReview>, Error> {
    let count = preflight(tx, pending.task_id()).await?;
    if count == 0 {
        return Ok(None);
    }
    let mut rows=tx.query("SELECT review_sequence,CASE WHEN length(worktree_id)=32 THEN worktree_id END,
      CASE WHEN length(original_digest)=32 THEN original_digest END,capture_created_at,
      window_start,window_end,finding_count,question_count,length(worktree_id),length(original_digest)
      FROM goal_intake_review_batches WHERE task_id=?1 ORDER BY review_sequence",
      params![pending.task_id().as_bytes().to_vec()]).await.map_err(Error::Read)?;
    let guarded = guard::supported(tx).await.map_err(Error::Read)?;
    let supports_http = http::supported(tx).await.map_err(Error::Read)?;
    let mut batches = Vec::with_capacity(count);
    while let Some(row) = rows.next().await.map_err(Error::Read)? {
        if batches.len() >= count
            || read_i64(&row, 0)? != sequence_to_i64(batches.len())?
            || read_i64(&row, 8)? != 32
            || read_i64(&row, 9)? != 32
            || read_id(&row, 1)? != *worktree.as_bytes()
            || GoalOriginalDigest::from_bytes(read_id(&row, 2)?) != pending.original().digest()
            || read_timestamp(&row, 3)? != pending.created_at()
        {
            return Err(Error::InvalidStoredData);
        }
        let seq = read_i64(&row, 0)?;
        let window = range(pending, read_i64(&row, 4)?, read_i64(&row, 5)?)?;
        let finding_count = bounded_count(&row, 6, 4)?;
        let question_count = bounded_count(&row, 7, 1)?;
        let findings =
            read_findings(tx, pending, seq, finding_count, guarded, supports_http).await?;
        let questions = read_questions(tx, pending, seq, question_count).await?;
        batches.push(
            GoalIntakeReviewBatch::reconstruct(window, findings, questions)
                .map_err(|_| Error::InvalidStoredData)?,
        );
    }
    if batches.len() != count {
        return Err(Error::InvalidStoredData);
    }
    GoalIntakeReview::restore(pending, batches)
        .map(Some)
        .map_err(|_| Error::InvalidStoredData)
}
fn range(pending: &PendingGoalIntake, start: i64, end: i64) -> Result<GoalOriginalRange, Error> {
    pending
        .original()
        .range(
            usize::try_from(start).map_err(|_| Error::InvalidStoredData)?,
            usize::try_from(end).map_err(|_| Error::InvalidStoredData)?,
        )
        .map_err(|_| Error::InvalidStoredData)
}
fn bounded_count(row: &libsql::Row, index: i32, max: usize) -> Result<usize, Error> {
    let value = usize::try_from(read_i64(row, index)?).map_err(|_| Error::InvalidStoredData)?;
    if value > max {
        return Err(Error::ResourceLimit);
    }
    Ok(value)
}

async fn read_findings(
    tx: &Transaction,
    pending: &PendingGoalIntake,
    seq: i64,
    count: usize,
    guarded: bool,
    supports_http: bool,
) -> Result<Vec<GoalIntakeFinding>, Error> {
    // Columns4..12 are the existing original-criterion codec's closed projection.
    let guard_column = if guarded {
        guard::REQUIREMENT_SELECT
    } else {
        ""
    };
    let http_column = if supports_http {
        http::RECIPE_SELECT
    } else {
        ""
    };
    let mut rows=tx.query(&format!("SELECT finding_sequence,
      CASE WHEN length(CAST(kind AS BLOB))<=32 THEN kind END,start_byte,end_byte,
      CASE WHEN length(CAST(evidence_kind AS BLOB))<=32 THEN evidence_kind END,
      CASE WHEN length(command_id)=32 THEN command_id END,
      CASE WHEN length(CAST(selector_kind AS BLOB))<=8 THEN selector_kind END,
      CASE WHEN length(CAST(selector_name AS BLOB))<=1024 THEN selector_name END,minimum_cases,
      CASE WHEN length(CAST(verification_scope AS BLOB))<=16 THEN verification_scope END,
      CASE WHEN length(CAST(diff_mode AS BLOB))<=32 THEN diff_mode END,
      CASE WHEN length(CAST(kind AS BLOB))<=32
        AND length(CAST(statement AS BLOB)) BETWEEN 1 AND 2048
        AND (requiredness IS NULL OR length(CAST(requiredness AS BLOB))<=8)
        AND (evidence_kind IS NULL OR length(CAST(evidence_kind AS BLOB))<=32)
        AND (command_id IS NULL OR length(command_id)=32)
        AND (selector_kind IS NULL OR length(CAST(selector_kind AS BLOB))<=8)
        AND (selector_name IS NULL OR length(CAST(selector_name AS BLOB))<=1024)
        AND (verification_scope IS NULL OR length(CAST(verification_scope AS BLOB))<=16)
        AND (diff_mode IS NULL OR length(CAST(diff_mode AS BLOB))<=32) THEN 1 ELSE 0 END,
      diff_path_count,CASE WHEN length(CAST(statement AS BLOB)) BETWEEN 1 AND 2048 THEN statement END,
       CASE WHEN length(CAST(requiredness AS BLOB))<=8 THEN requiredness END{guard_column}{http_column}
      FROM goal_intake_review_findings WHERE task_id=?1 AND review_sequence=?2 ORDER BY finding_sequence"),
      params![pending.task_id().as_bytes().to_vec(),seq]).await.map_err(Error::Read)?;
    let mut findings = Vec::with_capacity(count);
    while let Some(row) = rows.next().await.map_err(Error::Read)? {
        if findings.len() >= count
            || read_i64(&row, 0)? != sequence_to_i64(findings.len())?
            || read_i64(&row, 11)? != 1
        {
            return Err(Error::InvalidStoredData);
        }
        let paths = read_paths(
            tx,
            pending.task_id(),
            seq,
            read_i64(&row, 0)?,
            bounded_count(&row, 12, 64)?,
        )
        .await?;
        let requiredness = read_optional_text(&row, 14)?;
        let translation = if guarded {
            guard::read_requirement(&row, 15).map_err(|_| Error::InvalidStoredData)?
        } else {
            None
        };
        let loopback = if supports_http {
            http::read_recipe(&row, 15 + i32::from(guarded))
                .map_err(|_| Error::InvalidStoredData)?
        } else {
            None
        };
        let kind = match read_text(&row, 1)?.as_str() {
            "outcome" => GoalIntakeFindingKind::Outcome {
                requirement: parse_requirement(
                    requiredness.as_deref().ok_or(Error::InvalidStoredData)?,
                )?,
                evidence_requirement: super::original::read_requirement(
                    &row,
                    paths,
                    translation,
                    loopback,
                )?,
            },
            name => {
                if requiredness.is_some()
                    || translation.is_some()
                    || loopback.is_some()
                    || !paths.is_empty()
                    || read_i64(&row, 12)? != 0
                    || (4..=10)
                        .any(|index| !matches!(row.get_value(index), Ok(libsql::Value::Null)))
                {
                    return Err(Error::InvalidStoredData);
                }
                match name {
                    "constraint" => GoalIntakeFindingKind::Constraint,
                    "non_goal" => GoalIntakeFindingKind::NonGoal,
                    "existing_decision" => GoalIntakeFindingKind::ExistingUserDecision,
                    _ => return Err(Error::InvalidStoredData),
                }
            }
        };
        let statement = read_text(&row, 13)?;
        let finding = GoalIntakeFinding::new(
            kind,
            range(pending, read_i64(&row, 2)?, read_i64(&row, 3)?)?,
            statement.clone(),
        )
        .map_err(|_| Error::InvalidStoredData)?;
        if finding.statement() != statement {
            return Err(Error::InvalidStoredData);
        }
        findings.push(finding);
    }
    if findings.len() != count {
        return Err(Error::InvalidStoredData);
    }
    Ok(findings)
}
async fn read_paths(
    tx: &Transaction,
    task: TaskId,
    seq: i64,
    finding: i64,
    count: usize,
) -> Result<Vec<RepositoryPath>, Error> {
    let mut rows=tx.query("SELECT path_sequence,CASE WHEN length(path) BETWEEN 1 AND 65536 THEN path END,length(path)
      FROM goal_intake_review_diff_paths WHERE task_id=?1 AND review_sequence=?2 AND finding_sequence=?3 ORDER BY path_sequence",
      params![task.as_bytes().to_vec(),seq,finding]).await.map_err(Error::Read)?;
    let mut paths = Vec::with_capacity(count);
    while let Some(row) = rows.next().await.map_err(Error::Read)? {
        if paths.len() >= count
            || read_i64(&row, 0)? != sequence_to_i64(paths.len())?
            || !(1..=65536).contains(&read_i64(&row, 2)?)
        {
            return Err(Error::InvalidStoredData);
        }
        let bytes: Vec<u8> = row.get(1).map_err(Error::Read)?;
        let path =
            RepositoryPath::try_from_bytes(bytes.clone()).map_err(|_| Error::InvalidStoredData)?;
        if path.as_bytes() != bytes {
            return Err(Error::InvalidStoredData);
        }
        paths.push(path);
    }
    if paths.len() != count {
        return Err(Error::InvalidStoredData);
    }
    Ok(paths)
}
async fn read_questions(
    tx: &Transaction,
    pending: &PendingGoalIntake,
    seq: i64,
    count: usize,
) -> Result<Vec<GoalIntakeOpenQuestion>, Error> {
    let mut rows = tx
        .query(
            "SELECT question_id,CASE WHEN length(CAST(kind AS BLOB))<=32 THEN kind END,
      CASE WHEN length(CAST(question AS BLOB)) BETWEEN 1 AND 512 THEN question END,range_count,
      length(CAST(kind AS BLOB)),length(CAST(question AS BLOB)) FROM goal_intake_review_questions
      WHERE task_id=?1 AND review_sequence=?2 ORDER BY question_id",
            params![pending.task_id().as_bytes().to_vec(), seq],
        )
        .await
        .map_err(Error::Read)?;
    let mut questions = Vec::with_capacity(count);
    while let Some(row) = rows.next().await.map_err(Error::Read)? {
        if questions.len() >= count
            || read_i64(&row, 4)? > 32
            || !(1..=512).contains(&read_i64(&row, 5)?)
        {
            return Err(Error::InvalidStoredData);
        }
        let id = GoalIntakeQuestionId::new(
            u16::try_from(read_i64(&row, 0)?).map_err(|_| Error::InvalidStoredData)?,
        )
        .map_err(|_| Error::InvalidStoredData)?;
        let kind = match read_text(&row, 1)?.as_str() {
            "missing_user_choice" => GoalIntakeQuestionKind::MissingUserChoice,
            "conflicting_requirements" => GoalIntakeQuestionKind::ConflictingRequirements,
            _ => return Err(Error::InvalidStoredData),
        };
        let count = bounded_count(&row, 3, 8)?;
        if count == 0 {
            return Err(Error::InvalidStoredData);
        }
        let ranges = read_question_ranges(tx, pending, seq, id, count).await?;
        let text = read_text(&row, 2)?;
        let draft = GoalIntakeQuestionDraft::new(kind, text.clone(), ranges.clone())
            .map_err(|_| Error::InvalidStoredData)?;
        if draft.text() != text || draft.ranges() != ranges {
            return Err(Error::InvalidStoredData);
        }
        questions.push(GoalIntakeOpenQuestion::reconstruct(id, draft));
    }
    if questions.len() != count {
        return Err(Error::InvalidStoredData);
    }
    Ok(questions)
}
async fn read_question_ranges(
    tx: &Transaction,
    pending: &PendingGoalIntake,
    seq: i64,
    id: GoalIntakeQuestionId,
    count: usize,
) -> Result<Vec<GoalOriginalRange>, Error> {
    let mut rows = tx
        .query(
            "SELECT range_sequence,start_byte,end_byte FROM goal_intake_review_question_ranges
      WHERE task_id=?1 AND review_sequence=?2 AND question_id=?3 ORDER BY range_sequence",
            params![
                pending.task_id().as_bytes().to_vec(),
                seq,
                i64::from(id.get())
            ],
        )
        .await
        .map_err(Error::Read)?;
    let mut ranges = Vec::with_capacity(count);
    while let Some(row) = rows.next().await.map_err(Error::Read)? {
        if ranges.len() >= count || read_i64(&row, 0)? != sequence_to_i64(ranges.len())? {
            return Err(Error::InvalidStoredData);
        }
        ranges.push(range(pending, read_i64(&row, 1)?, read_i64(&row, 2)?)?);
    }
    if ranges.len() != count {
        return Err(Error::InvalidStoredData);
    }
    Ok(ranges)
}

async fn write_batch(
    tx: &Transaction,
    worktree: WorktreeId,
    review: &GoalIntakeReview,
    batch: &GoalIntakeReviewBatch,
) -> Result<(), Error> {
    let seq = i64::from(review.revision().get());
    let task = review.goal().task_id();
    for (index, finding) in batch.findings().iter().enumerate() {
        write_finding(tx, task, seq, sequence_to_i64(index)?, finding).await?;
    }
    for question in batch.questions() {
        let draft = question.draft();
        tx.execute("INSERT INTO goal_intake_review_questions(task_id,review_sequence,question_id,kind,question,range_count)
            VALUES(?1,?2,?3,?4,?5,?6)",params![task.as_bytes().to_vec(),seq,i64::from(question.id().get()),
                match draft.kind(){GoalIntakeQuestionKind::MissingUserChoice=>"missing_user_choice",
                    GoalIntakeQuestionKind::ConflictingRequirements=>"conflicting_requirements"},
                draft.text(),i64::try_from(draft.ranges().len()).map_err(|_|Error::ResourceLimit)?])
            .await.map_err(classify_unexpected_constraint)?;
        for (index, range) in draft.ranges().iter().enumerate() {
            tx.execute("INSERT INTO goal_intake_review_question_ranges(task_id,review_sequence,question_id,range_sequence,start_byte,end_byte)
                VALUES(?1,?2,?3,?4,?5,?6)",params![task.as_bytes().to_vec(),seq,i64::from(question.id().get()),
                    sequence_to_i64(index)?,i64::from(range.start_byte()),i64::from(range.end_byte())])
                .await.map_err(classify_unexpected_constraint)?;
        }
    }
    // Children have deferred header FKs. Inserting this seal last admits all or none.
    tx.execute("INSERT INTO goal_intake_review_batches(task_id,review_sequence,worktree_id,original_digest,
        capture_created_at,window_start,window_end,finding_count,question_count) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",
        params![task.as_bytes().to_vec(),seq,worktree.as_bytes().to_vec(),review.digest().as_bytes().to_vec(),
            timestamp_to_i64(review.created_at())?,i64::from(batch.window().start_byte()),i64::from(batch.window().end_byte()),
            i64::try_from(batch.findings().len()).map_err(|_|Error::ResourceLimit)?,
            i64::try_from(batch.questions().len()).map_err(|_|Error::ResourceLimit)?])
        .await.map_err(classify_unexpected_constraint)?;
    Ok(())
}
async fn write_finding(
    tx: &Transaction,
    task: TaskId,
    seq: i64,
    index: i64,
    finding: &GoalIntakeFinding,
) -> Result<(), Error> {
    let (family, requiredness, evidence) = match finding.kind() {
        GoalIntakeFindingKind::Outcome {
            requirement,
            evidence_requirement,
        } => (
            "outcome",
            Some(requirement_text(*requirement)),
            Some(evidence_requirement),
        ),
        GoalIntakeFindingKind::Constraint => ("constraint", None, None),
        GoalIntakeFindingKind::NonGoal => ("non_goal", None, None),
        GoalIntakeFindingKind::ExistingUserDecision => ("existing_decision", None, None),
    };
    let (kind, command, selector, name, minimum, scope, diff, paths) = match evidence {
        Some(CriterionEvidenceRequirement::ExistingTest {
            command,
            selector,
            minimum_cases,
            scope,
            ..
        }) => {
            let (selector, name) = match selector {
                TestCaseSelector::All => ("all", None),
                TestCaseSelector::Exact(name) => ("exact", Some(name.as_str())),
            };
            (
                Some("existing_test"),
                Some(command.as_bytes().to_vec()),
                Some(selector),
                name,
                Some(i64::from(minimum_cases.get())),
                Some(super::original::scope_text(*scope)),
                None,
                &[][..],
            )
        }
        Some(CriterionEvidenceRequirement::ChangeOnly(invariant)) => (
            Some("change_only"),
            None,
            None,
            None,
            None,
            None,
            Some(super::original::diff_text(invariant.mode())),
            invariant.paths(),
        ),
        Some(
            CriterionEvidenceRequirement::RuntimeNeedsRecipe
            | CriterionEvidenceRequirement::LoopbackHttp(_),
        ) => (
            Some("runtime_needs_recipe"),
            None,
            None,
            None,
            None,
            None,
            None,
            &[][..],
        ),
        None => (None, None, None, None, None, None, None, &[][..]),
    };
    let mut values = Vec::from(params![
        task.as_bytes().to_vec(),
        seq,
        index,
        family,
        i64::from(finding.range().start_byte()),
        i64::from(finding.range().end_byte()),
        finding.statement(),
        requiredness,
        kind,
        command,
        selector,
        name,
        minimum,
        scope,
        diff,
        i64::try_from(paths.len()).map_err(|_| Error::ResourceLimit)?,
    ]);
    let (column, parameter) = if let Some(CriterionEvidenceRequirement::ExistingTest {
        python_integer_translation: Some(translation),
        ..
    }) = evidence
    {
        if !guard::supported(tx).await.map_err(Error::Read)? {
            return Err(Error::ReviewSchemaUnsupported);
        }
        values.push(Ok(Value::Blob(guard::write_requirement(translation))));
        (",python_integer_translation", ",?17")
    } else if let Some(CriterionEvidenceRequirement::LoopbackHttp(recipe)) = evidence {
        if !http::supported(tx).await.map_err(Error::Read)? {
            return Err(Error::ReviewSchemaUnsupported);
        }
        let payload = http::encode_recipe(recipe);
        preflight_http_bytes(tx, task, payload.len()).await?;
        values.push(Ok(Value::Blob(payload)));
        (",loopback_http_recipe", ",?17")
    } else {
        ("", "")
    };
    tx.execute(&format!("INSERT INTO goal_intake_review_findings(task_id,review_sequence,finding_sequence,kind,start_byte,end_byte,
        statement,requiredness,evidence_kind,command_id,selector_kind,selector_name,minimum_cases,verification_scope,diff_mode,diff_path_count{column})
        VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16{parameter})"),
        params_from_iter(values)).await.map_err(classify_unexpected_constraint)?;
    for (position, path) in paths.iter().enumerate() {
        tx.execute("INSERT INTO goal_intake_review_diff_paths(task_id,review_sequence,finding_sequence,path_sequence,path)
            VALUES(?1,?2,?3,?4,?5)",params![task.as_bytes().to_vec(),seq,index,sequence_to_i64(position)?,path.as_bytes().to_vec()])
            .await.map_err(classify_unexpected_constraint)?;
    }
    Ok(())
}
