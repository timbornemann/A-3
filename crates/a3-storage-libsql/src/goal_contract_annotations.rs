//! Optional original ranges on the three existing ordered Goal text collections.

use super::{Error, read_i64, read_id, read_usize, schema_supports_annotations};
use a3_domain::{
    AcceptanceCriterion, GoalConstraint, GoalContract, GoalContractDraft, GoalContractReference,
    GoalOriginalDigest, GoalOriginalRange, GoalOriginalRequest, NonGoal, UserDecision,
};
use libsql::{Transaction, params};

const MAX_ENTRIES: usize = 64;
const CONSTRAINTS: &str = "goal_constraint_origins";
const NON_GOALS: &str = "goal_non_goal_origins";
const USER_DECISIONS: &str = "goal_user_decision_origins";

pub(super) fn has_origins(goal: &GoalContract) -> bool {
    goal.draft()
        .constraints()
        .iter()
        .any(|item| item.origin().is_some())
        || goal
            .draft()
            .non_goals()
            .iter()
            .any(|item| item.origin().is_some())
        || goal
            .draft()
            .user_decisions()
            .iter()
            .any(|item| item.origin().is_some())
}

pub(super) async fn write(transaction: &Transaction, goal: &GoalContract) -> Result<(), Error> {
    if !schema_supports_annotations(transaction).await? {
        return Err(Error::OriginalSchemaUnsupported);
    }
    write_ranges(
        transaction,
        goal.reference(),
        CONSTRAINTS,
        goal.draft()
            .constraints()
            .iter()
            .map(GoalConstraint::origin),
    )
    .await?;
    write_ranges(
        transaction,
        goal.reference(),
        NON_GOALS,
        goal.draft().non_goals().iter().map(NonGoal::origin),
    )
    .await?;
    write_ranges(
        transaction,
        goal.reference(),
        USER_DECISIONS,
        goal.draft()
            .user_decisions()
            .iter()
            .map(UserDecision::origin),
    )
    .await
}

async fn write_ranges(
    transaction: &Transaction,
    goal: GoalContractReference,
    table: &str,
    ranges: impl Iterator<Item = Option<GoalOriginalRange>>,
) -> Result<(), Error> {
    // Table names come exclusively from this module's three fixed constants.
    let sql = format!(
        "INSERT INTO {table}
        (task_id, goal_revision, item_sequence, original_digest, start_byte, end_byte)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6)"
    );
    for (index, range) in ranges.enumerate() {
        if index >= MAX_ENTRIES {
            return Err(Error::ResourceLimit);
        }
        if let Some(range) = range {
            transaction
                .execute(
                    &sql,
                    params![
                        goal.task_id().as_bytes().to_vec(),
                        i64::from(goal.revision().get()),
                        i64::try_from(index + 1).map_err(|_| Error::ResourceLimit)?,
                        range.digest().as_bytes().to_vec(),
                        i64::from(range.start_byte()),
                        i64::from(range.end_byte())
                    ],
                )
                .await
                .map_err(super::classify_unexpected_constraint)?;
        }
    }
    Ok(())
}

pub(super) async fn restore(
    transaction: &Transaction,
    original: &GoalOriginalRequest,
    draft: &GoalContractDraft,
    criteria: Vec<AcceptanceCriterion>,
) -> Result<GoalContractDraft, Error> {
    let constraint_ranges = read_ranges(
        transaction,
        original,
        CONSTRAINTS,
        draft.constraints().len(),
    )
    .await?;
    let non_goal_ranges =
        read_ranges(transaction, original, NON_GOALS, draft.non_goals().len()).await?;
    let decision_ranges = read_ranges(
        transaction,
        original,
        USER_DECISIONS,
        draft.user_decisions().len(),
    )
    .await?;
    let constraints = draft
        .constraints()
        .iter()
        .cloned()
        .zip(constraint_ranges)
        .map(|(item, origin)| match origin {
            Some(range) => item.with_origin(range),
            None => item,
        })
        .collect();
    let non_goals = draft
        .non_goals()
        .iter()
        .cloned()
        .zip(non_goal_ranges)
        .map(|(item, origin)| match origin {
            Some(range) => item.with_origin(range),
            None => item,
        })
        .collect();
    let decisions = draft
        .user_decisions()
        .iter()
        .cloned()
        .zip(decision_ranges)
        .map(|(item, origin)| match origin {
            Some(range) => item.with_origin(range),
            None => item,
        })
        .collect();
    GoalContractDraft::new(
        draft.objective().clone(),
        criteria,
        constraints,
        non_goals,
        decisions,
        draft.success_verification().clone(),
    )
    .map_err(|_| Error::InvalidStoredData)
}

async fn read_ranges(
    transaction: &Transaction,
    original: &GoalOriginalRequest,
    table: &str,
    entry_count: usize,
) -> Result<Vec<Option<GoalOriginalRange>>, Error> {
    if entry_count > MAX_ENTRIES {
        return Err(Error::ResourceLimit);
    }
    let sql = format!(
        "SELECT item_sequence,
        CASE WHEN length(original_digest) = 32 THEN original_digest END,
        start_byte, end_byte, length(original_digest)
        FROM {table} WHERE task_id = ?1 AND goal_revision = ?2 ORDER BY item_sequence"
    );
    let goal = original.goal();
    let mut rows = transaction
        .query(
            &sql,
            params![
                goal.task_id().as_bytes().to_vec(),
                i64::from(goal.revision().get())
            ],
        )
        .await
        .map_err(Error::Read)?;
    let mut ranges = vec![None; entry_count];
    let mut seen = 0;
    while let Some(row) = rows.next().await.map_err(Error::Read)? {
        if seen >= entry_count || read_i64(&row, 4)? != 32 {
            return Err(Error::InvalidStoredData);
        }
        let sequence = read_usize(&row, 0)?;
        let index = sequence
            .checked_sub(1)
            .filter(|index| *index < entry_count)
            .ok_or(Error::InvalidStoredData)?;
        let range = original
            .restore_range(
                goal,
                GoalOriginalDigest::from_bytes(read_id(&row, 1)?),
                read_usize(&row, 2)?,
                read_usize(&row, 3)?,
            )
            .map_err(|_| Error::InvalidStoredData)?;
        if ranges[index].replace(range).is_some() {
            return Err(Error::InvalidStoredData);
        }
        seen += 1;
    }
    Ok(ranges)
}

pub(super) async fn require_no_origins(
    transaction: &Transaction,
    goal: GoalContractReference,
) -> Result<(), Error> {
    let mut rows = transaction
        .query(
            "SELECT EXISTS(SELECT 1 FROM goal_constraint_origins
            WHERE task_id = ?1 AND goal_revision = ?2)
        OR EXISTS(SELECT 1 FROM goal_non_goal_origins
            WHERE task_id = ?1 AND goal_revision = ?2)
        OR EXISTS(SELECT 1 FROM goal_user_decision_origins
            WHERE task_id = ?1 AND goal_revision = ?2)",
            params![
                goal.task_id().as_bytes().to_vec(),
                i64::from(goal.revision().get())
            ],
        )
        .await
        .map_err(Error::Read)?;
    let row = rows
        .next()
        .await
        .map_err(Error::Read)?
        .ok_or(Error::InvalidStoredData)?;
    if read_i64(&row, 0)? != 0 || rows.next().await.map_err(Error::Read)?.is_some() {
        return Err(Error::InvalidStoredData);
    }
    Ok(())
}
