//! Original-request persistence inside the existing Goal revision transaction.

use super::{
    GoalContractRepositoryError as Error, classify_unexpected_constraint, read_i64, read_id,
    read_optional_text, sequence_to_i64, validate_sequence,
};
use crate::{loopback_http_codec as http, python_translation_codec as guard};
use a3_domain::{
    AcceptanceCriterion, AcceptanceCriterionId, CriterionEvidenceRequirement, CriterionOrigin,
    DiffInvariantMode, DiffInvariantVerification, DiscoveredCommandId, GoalContract,
    GoalContractDraft, GoalContractReference, GoalOriginalDigest, GoalOriginalRequest,
    LoopbackHttpRecipe, MAX_GOAL_ORIGINAL_BYTES, MAX_GOAL_ORIGINAL_PART_BYTES,
    MAX_GOAL_ORIGINAL_PARTS, MinimumTestCaseCount, PythonIntegerTranslation, RepositoryPath,
    TestCaseSelector, TestCaseSelectorName, VerificationScope,
};
use libsql::{Transaction, Value, params, params_from_iter};
use std::collections::BTreeMap;

#[path = "goal_contract_annotations.rs"]
mod annotations;

pub(super) async fn restore(
    transaction: &Transaction,
    base: GoalContract,
    supports_original: bool,
    version: Option<i64>,
) -> Result<GoalContract, Error> {
    if !supports_original {
        if version.is_some() {
            return Err(Error::InvalidStoredData);
        }
        return Ok(base);
    }
    match version {
        None => {
            require_no_original(transaction, base.reference()).await?;
            Ok(base)
        }
        Some(1) => {
            let original = load_original(transaction, base.reference()).await?;
            let criteria = attach_criterion_origins(
                transaction,
                &original,
                base.draft().acceptance_criteria().to_vec(),
            )
            .await?;
            let draft = if schema_supports_annotations(transaction).await? {
                annotations::restore(transaction, &original, base.draft(), criteria).await?
            } else {
                GoalContractDraft::new(
                    base.draft().objective().clone(),
                    criteria,
                    base.draft().constraints().to_vec(),
                    base.draft().non_goals().to_vec(),
                    base.draft().user_decisions().to_vec(),
                    base.draft().success_verification().clone(),
                )
                .map_err(|_| Error::InvalidStoredData)?
            };
            let goal = GoalContract::reconstruct(
                base.task_id(),
                base.revision(),
                base.previous_revision(),
                base.revision_reason().cloned(),
                draft,
                base.created_at(),
            )
            .map_err(|_| Error::InvalidStoredData)?;
            goal.with_original(original)
                .map_err(|_| Error::InvalidStoredData)
        }
        Some(_) => Err(Error::InvalidStoredData),
    }
}

pub(super) async fn schema_supports_original(transaction: &Transaction) -> Result<bool, Error> {
    Ok(schema_version(transaction).await? >= 43)
}

async fn schema_supports_annotations(transaction: &Transaction) -> Result<bool, Error> {
    Ok(schema_version(transaction).await? >= 44)
}

async fn schema_version(transaction: &Transaction) -> Result<i64, Error> {
    let mut rows = transaction
        .query("PRAGMA user_version", ())
        .await
        .map_err(Error::Read)?;
    let row = rows
        .next()
        .await
        .map_err(Error::Read)?
        .ok_or(Error::InvalidStoredData)?;
    let version = read_i64(&row, 0)?;
    if rows.next().await.map_err(Error::Read)?.is_some() {
        return Err(Error::InvalidStoredData);
    }
    Ok(version)
}

pub(super) async fn require_supported_schema(
    transaction: &Transaction,
    goal: &GoalContract,
) -> Result<(), Error> {
    if goal.original().is_none() {
        return Ok(());
    }
    let version = schema_version(transaction).await?;
    if version < 43 || (annotations::has_origins(goal) && version < 44) {
        return Err(Error::OriginalSchemaUnsupported);
    }
    if version < i64::from(http::SCHEMA_VERSION)
        && goal.draft().acceptance_criteria().iter().any(|criterion| {
            criterion.origin().is_some_and(|origin| {
                matches!(
                    origin.evidence_requirement(),
                    CriterionEvidenceRequirement::LoopbackHttp(_)
                )
            })
        })
    {
        return Err(Error::OriginalSchemaUnsupported);
    }
    Ok(())
}

pub(super) async fn write(transaction: &Transaction, goal: &GoalContract) -> Result<(), Error> {
    let Some(original) = goal.original() else {
        return Ok(());
    };
    goal.validate_original_binding()
        .map_err(|_| Error::InvalidInput)?;
    // Deferred header foreign keys allow these immutable rows only before the header seal.
    if annotations::has_origins(goal) {
        if !schema_supports_annotations(transaction).await? {
            return Err(Error::OriginalSchemaUnsupported);
        }
        annotations::write(transaction, goal).await?;
    }
    transaction
        .execute(
            "INSERT INTO goal_original_headers
         (task_id, goal_revision, original_digest, byte_length, part_count)
         VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                goal.task_id().as_bytes().to_vec(),
                i64::from(goal.revision().get()),
                original.digest().as_bytes().to_vec(),
                i64::try_from(original.as_bytes().len()).map_err(|_| Error::ResourceLimit)?,
                i64::try_from(original.parts().len()).map_err(|_| Error::ResourceLimit)?
            ],
        )
        .await
        .map_err(classify_unexpected_constraint)?;
    for (index, part) in original.parts().iter().copied().enumerate() {
        let bytes = original
            .resolve(part)
            .map_err(|_| Error::InvalidInput)?
            .as_bytes()
            .to_vec();
        transaction
            .execute(
                "INSERT INTO goal_original_parts
             (task_id, goal_revision, part_sequence, start_byte, end_byte, original_bytes)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    goal.task_id().as_bytes().to_vec(),
                    i64::from(goal.revision().get()),
                    sequence_to_i64(index)?,
                    i64::from(part.start_byte()),
                    i64::from(part.end_byte()),
                    bytes
                ],
            )
            .await
            .map_err(classify_unexpected_constraint)?;
    }
    for criterion in goal.draft().acceptance_criteria() {
        let origin = criterion.origin().ok_or(Error::InvalidInput)?;
        write_criterion(transaction, goal.reference(), criterion.id(), origin).await?;
    }
    Ok(())
}

async fn write_criterion(
    transaction: &Transaction,
    goal: GoalContractReference,
    criterion: AcceptanceCriterionId,
    origin: &CriterionOrigin,
) -> Result<(), Error> {
    let (kind, command, selector, name, minimum, scope, diff, translation) =
        match origin.evidence_requirement() {
            CriterionEvidenceRequirement::ExistingTest {
                command,
                selector,
                minimum_cases,
                scope,
                python_integer_translation,
            } => {
                let (selector, name) = match selector {
                    TestCaseSelector::All => (Some("all"), None),
                    TestCaseSelector::Exact(name) => (Some("exact"), Some(name.as_str())),
                };
                (
                    "existing_test",
                    Some(command.as_bytes().to_vec()),
                    selector,
                    name,
                    Some(i64::from(minimum_cases.get())),
                    Some(scope_text(*scope)),
                    None,
                    python_integer_translation.as_ref(),
                )
            }
            CriterionEvidenceRequirement::ChangeOnly(invariant) => (
                "change_only",
                None,
                None,
                None,
                None,
                None,
                Some(diff_text(invariant.mode())),
                None,
            ),
            CriterionEvidenceRequirement::RuntimeNeedsRecipe
            | CriterionEvidenceRequirement::LoopbackHttp(_) => (
                "runtime_needs_recipe",
                None,
                None,
                None,
                None,
                None,
                None,
                None,
            ),
        };
    let range = origin.original_range();
    let diff_path_count = match origin.evidence_requirement() {
        CriterionEvidenceRequirement::ChangeOnly(invariant) => invariant.paths().len(),
        _ => 0,
    };
    let mut values = Vec::from(params![
        goal.task_id().as_bytes().to_vec(),
        i64::from(goal.revision().get()),
        criterion.as_bytes().to_vec(),
        range.digest().as_bytes().to_vec(),
        i64::from(range.start_byte()),
        i64::from(range.end_byte()),
        kind,
        command,
        selector,
        name,
        minimum,
        scope,
        diff,
        i64::try_from(diff_path_count).map_err(|_| Error::ResourceLimit)?
    ]);
    let (column, parameter) = if let Some(translation) = translation {
        if !guard::supported(transaction).await.map_err(Error::Read)? {
            return Err(Error::OriginalSchemaUnsupported);
        }
        values.push(Ok(Value::Blob(guard::write_requirement(translation))));
        (", python_integer_translation", ", ?15")
    } else if let CriterionEvidenceRequirement::LoopbackHttp(recipe) = origin.evidence_requirement()
    {
        if !http::supported(transaction).await.map_err(Error::Read)? {
            return Err(Error::OriginalSchemaUnsupported);
        }
        values.push(Ok(Value::Blob(http::encode_recipe(recipe))));
        (", loopback_http_recipe", ", ?15")
    } else {
        ("", "")
    };
    transaction
        .execute(
            &format!(
                "INSERT INTO goal_criterion_origins
         (task_id, goal_revision, criterion_id, original_digest, start_byte, end_byte,
          requirement_kind, command_id, selector_kind, selector_name, minimum_cases,
          verification_scope, diff_mode, diff_path_count{column})
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14{parameter})"
            ),
            params_from_iter(values),
        )
        .await
        .map_err(classify_unexpected_constraint)?;
    if let CriterionEvidenceRequirement::ChangeOnly(invariant) = origin.evidence_requirement() {
        for (index, path) in invariant.paths().iter().enumerate() {
            transaction
                .execute(
                    "INSERT INTO goal_criterion_diff_paths
                 (task_id, goal_revision, criterion_id, path_sequence, path)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![
                        goal.task_id().as_bytes().to_vec(),
                        i64::from(goal.revision().get()),
                        criterion.as_bytes().to_vec(),
                        sequence_to_i64(index)?,
                        path.as_bytes().to_vec()
                    ],
                )
                .await
                .map_err(classify_unexpected_constraint)?;
        }
    }
    Ok(())
}

pub(super) async fn load_original(
    transaction: &Transaction,
    goal: GoalContractReference,
) -> Result<GoalOriginalRequest, Error> {
    let mut rows = transaction
        .query(
            "SELECT CASE WHEN length(original_digest) = 32 THEN original_digest END,
              byte_length, part_count, length(original_digest) FROM goal_original_headers
         WHERE task_id = ?1 AND goal_revision = ?2",
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
    if read_i64(&row, 3)? != 32 {
        return Err(Error::InvalidStoredData);
    }
    let digest = GoalOriginalDigest::from_bytes(read_id(&row, 0)?);
    let length = read_usize(&row, 1)?;
    let count = read_usize(&row, 2)?;
    if !(1..=MAX_GOAL_ORIGINAL_BYTES).contains(&length)
        || !(1..=MAX_GOAL_ORIGINAL_PARTS).contains(&count)
        || rows.next().await.map_err(Error::Read)?.is_some()
    {
        return Err(Error::InvalidStoredData);
    }
    let mut rows = transaction
        .query(
            "SELECT part_sequence, start_byte, end_byte,
              CASE WHEN length(original_bytes) BETWEEN 1 AND 2048 THEN original_bytes END,
              length(original_bytes) FROM goal_original_parts
         WHERE task_id = ?1 AND goal_revision = ?2 ORDER BY part_sequence",
            params![
                goal.task_id().as_bytes().to_vec(),
                i64::from(goal.revision().get())
            ],
        )
        .await
        .map_err(Error::Read)?;
    let mut bytes = Vec::with_capacity(length);
    let mut ranges = Vec::with_capacity(count);
    while let Some(row) = rows.next().await.map_err(Error::Read)? {
        if ranges.len() >= count {
            return Err(Error::InvalidStoredData);
        }
        validate_sequence(&row, ranges.len())?;
        let start = read_usize(&row, 1)?;
        let end = read_usize(&row, 2)?;
        let stored_part_bytes = read_usize(&row, 4)?;
        if stored_part_bytes > MAX_GOAL_ORIGINAL_PART_BYTES {
            return Err(Error::ResourceLimit);
        }
        if stored_part_bytes == 0 {
            return Err(Error::InvalidStoredData);
        }
        let part: Vec<u8> = row.get(3).map_err(Error::Read)?;
        if start != bytes.len()
            || end <= start
            || end > length
            || part.is_empty()
            || part.len() > MAX_GOAL_ORIGINAL_PART_BYTES
            || end - start != part.len()
            || std::str::from_utf8(&part).is_err()
        {
            return Err(Error::InvalidStoredData);
        }
        bytes.extend_from_slice(&part);
        ranges.push((start, end));
    }
    if bytes.len() != length || ranges.len() != count {
        return Err(Error::InvalidStoredData);
    }
    let original = GoalOriginalRequest::reconstruct(goal, bytes, digest)
        .map_err(|_| Error::InvalidStoredData)?;
    if original.parts().len() != ranges.len()
        || original
            .parts()
            .iter()
            .zip(ranges)
            .any(|(part, (start, end))| {
                part.start_byte() as usize != start || part.end_byte() as usize != end
            })
    {
        return Err(Error::InvalidStoredData);
    }
    Ok(original)
}

pub(super) async fn require_no_original(
    transaction: &Transaction,
    goal: GoalContractReference,
) -> Result<(), Error> {
    let mut rows = transaction.query(
        "SELECT EXISTS(SELECT 1 FROM goal_original_headers WHERE task_id = ?1 AND goal_revision = ?2)
          OR EXISTS(SELECT 1 FROM goal_original_parts WHERE task_id = ?1 AND goal_revision = ?2)
          OR EXISTS(SELECT 1 FROM goal_criterion_origins WHERE task_id = ?1 AND goal_revision = ?2)
          OR EXISTS(SELECT 1 FROM goal_criterion_diff_paths WHERE task_id = ?1 AND goal_revision = ?2)",
        params![goal.task_id().as_bytes().to_vec(), i64::from(goal.revision().get())],
    ).await.map_err(Error::Read)?;
    let row = rows
        .next()
        .await
        .map_err(Error::Read)?
        .ok_or(Error::InvalidStoredData)?;
    if read_i64(&row, 0)? != 0 || rows.next().await.map_err(Error::Read)?.is_some() {
        return Err(Error::InvalidStoredData);
    }
    if schema_supports_annotations(transaction).await? {
        annotations::require_no_origins(transaction, goal).await?;
    }
    Ok(())
}

pub(super) async fn attach_criterion_origins(
    transaction: &Transaction,
    original: &GoalOriginalRequest,
    criteria: Vec<AcceptanceCriterion>,
) -> Result<Vec<AcceptanceCriterion>, Error> {
    let goal = original.goal();
    reject_orphan_diff_paths(transaction, goal).await?;
    let guarded = guard::supported(transaction).await.map_err(Error::Read)?;
    let guard_column = if guarded {
        guard::REQUIREMENT_SELECT
    } else {
        ""
    };
    let supports_http = http::supported(transaction).await.map_err(Error::Read)?;
    let http_column = if supports_http {
        http::RECIPE_SELECT
    } else {
        ""
    };
    let mut rows = transaction
        .query(
            &format!(
                "SELECT CASE WHEN length(criterion_id) = 32 THEN criterion_id END,
          CASE WHEN length(original_digest) = 32 THEN original_digest END, start_byte, end_byte,
          CASE WHEN length(CAST(requirement_kind AS BLOB)) <= 32 THEN requirement_kind END,
          CASE WHEN length(command_id) = 32 THEN command_id END,
          CASE WHEN length(CAST(selector_kind AS BLOB)) <= 8 THEN selector_kind END,
          CASE WHEN length(CAST(selector_name AS BLOB)) <= 1024 THEN selector_name END,
          minimum_cases,
          CASE WHEN length(CAST(verification_scope AS BLOB)) <= 16 THEN verification_scope END,
          CASE WHEN length(CAST(diff_mode AS BLOB)) <= 32 THEN diff_mode END,
          CASE WHEN length(criterion_id) = 32 AND length(original_digest) = 32
            AND length(CAST(requirement_kind AS BLOB)) <= 32
            AND (command_id IS NULL OR length(command_id) = 32)
            AND (selector_kind IS NULL OR length(CAST(selector_kind AS BLOB)) <= 8)
            AND (selector_name IS NULL OR length(CAST(selector_name AS BLOB)) <= 1024)
            AND (verification_scope IS NULL OR length(CAST(verification_scope AS BLOB)) <= 16)
            AND (diff_mode IS NULL OR length(CAST(diff_mode AS BLOB)) <= 32)
           THEN 1 ELSE 0 END, diff_path_count{guard_column}{http_column}
         FROM goal_criterion_origins WHERE task_id = ?1 AND goal_revision = ?2"
            ),
            params![
                goal.task_id().as_bytes().to_vec(),
                i64::from(goal.revision().get())
            ],
        )
        .await
        .map_err(Error::Read)?;
    let mut origins = BTreeMap::new();
    while let Some(row) = rows.next().await.map_err(Error::Read)? {
        if origins.len() >= criteria.len() {
            return Err(Error::InvalidStoredData);
        }
        if read_i64(&row, 11)? != 1 {
            return Err(Error::InvalidStoredData);
        }
        let criterion = AcceptanceCriterionId::from_bytes(read_id(&row, 0)?);
        let range = original
            .restore_range(
                goal,
                GoalOriginalDigest::from_bytes(read_id(&row, 1)?),
                read_usize(&row, 2)?,
                read_usize(&row, 3)?,
            )
            .map_err(|_| Error::InvalidStoredData)?;
        let paths = read_diff_paths(transaction, goal, criterion).await?;
        let translation = if guarded {
            guard::read_requirement(&row, 13).map_err(|_| Error::InvalidStoredData)?
        } else {
            None
        };
        let loopback = if supports_http {
            http::read_recipe(&row, 13 + i32::from(guarded))
                .map_err(|_| Error::InvalidStoredData)?
        } else {
            None
        };
        let requirement = read_requirement(&row, paths, translation, loopback)?;
        if origins
            .insert(criterion, CriterionOrigin::new(range, requirement))
            .is_some()
        {
            return Err(Error::InvalidStoredData);
        }
    }
    let result = criteria
        .into_iter()
        .map(|criterion| {
            let origin = origins
                .remove(&criterion.id())
                .ok_or(Error::InvalidStoredData)?;
            Ok(criterion.with_origin(origin))
        })
        .collect::<Result<Vec<_>, Error>>()?;
    if !origins.is_empty() {
        return Err(Error::InvalidStoredData);
    }
    Ok(result)
}

pub(super) fn read_requirement(
    row: &libsql::Row,
    paths: Vec<RepositoryPath>,
    translation: Option<PythonIntegerTranslation>,
    loopback: Option<LoopbackHttpRecipe>,
) -> Result<CriterionEvidenceRequirement, Error> {
    if read_usize(row, 12)? != paths.len() {
        return Err(Error::InvalidStoredData);
    }
    let kind = read_optional_text(row, 4)?.ok_or(Error::InvalidStoredData)?;
    if (kind != "existing_test" && translation.is_some())
        || (kind != "runtime_needs_recipe" && loopback.is_some())
    {
        return Err(Error::InvalidStoredData);
    }
    let command: Option<Vec<u8>> = row.get(5).map_err(Error::Read)?;
    let selector = read_optional_text(row, 6)?;
    let name = read_optional_text(row, 7)?;
    let minimum: Option<i64> = row.get(8).map_err(Error::Read)?;
    let scope = read_optional_text(row, 9)?;
    let diff = read_optional_text(row, 10)?;
    match kind.as_str() {
        "existing_test" if diff.is_none() && paths.is_empty() => {
            let command = command
                .ok_or(Error::InvalidStoredData)?
                .try_into()
                .map_err(|_| Error::InvalidStoredData)?;
            let selector = match (selector.as_deref(), name) {
                (Some("all"), None) => TestCaseSelector::All,
                (Some("exact"), Some(name)) => {
                    let selector = TestCaseSelectorName::try_from_string(name.clone())
                        .map_err(|_| Error::InvalidStoredData)?;
                    if selector.as_str() != name {
                        return Err(Error::InvalidStoredData);
                    }
                    TestCaseSelector::Exact(selector)
                }
                _ => return Err(Error::InvalidStoredData),
            };
            let minimum = u32::try_from(minimum.ok_or(Error::InvalidStoredData)?)
                .map_err(|_| Error::InvalidStoredData)?;
            Ok(CriterionEvidenceRequirement::ExistingTest {
                command: DiscoveredCommandId::from_bytes(command),
                selector,
                minimum_cases: MinimumTestCaseCount::new(minimum)
                    .map_err(|_| Error::InvalidStoredData)?,
                scope: parse_scope(scope.as_deref().ok_or(Error::InvalidStoredData)?)?,
                python_integer_translation: translation,
            })
        }
        "change_only"
            if command.is_none()
                && selector.is_none()
                && name.is_none()
                && minimum.is_none()
                && scope.is_none() =>
        {
            let mode = parse_diff(diff.as_deref().ok_or(Error::InvalidStoredData)?)?;
            let invariant = DiffInvariantVerification::new(mode, paths.clone())
                .map_err(|_| Error::InvalidStoredData)?;
            if invariant.paths() != paths {
                return Err(Error::InvalidStoredData);
            }
            Ok(CriterionEvidenceRequirement::ChangeOnly(invariant))
        }
        "runtime_needs_recipe"
            if command.is_none()
                && selector.is_none()
                && name.is_none()
                && minimum.is_none()
                && scope.is_none()
                && diff.is_none()
                && paths.is_empty() =>
        {
            Ok(loopback.map_or(
                CriterionEvidenceRequirement::RuntimeNeedsRecipe,
                CriterionEvidenceRequirement::LoopbackHttp,
            ))
        }
        _ => Err(Error::InvalidStoredData),
    }
}

async fn read_diff_paths(
    transaction: &Transaction,
    goal: GoalContractReference,
    criterion: AcceptanceCriterionId,
) -> Result<Vec<RepositoryPath>, Error> {
    let mut rows = transaction
        .query(
            "SELECT path_sequence, CASE WHEN length(path) BETWEEN 1 AND 131072 THEN path END,
              length(path) FROM goal_criterion_diff_paths
         WHERE task_id = ?1 AND goal_revision = ?2 AND criterion_id = ?3 ORDER BY path_sequence",
            params![
                goal.task_id().as_bytes().to_vec(),
                i64::from(goal.revision().get()),
                criterion.as_bytes().to_vec()
            ],
        )
        .await
        .map_err(Error::Read)?;
    let mut paths = Vec::new();
    while let Some(row) = rows.next().await.map_err(Error::Read)? {
        if paths.len() >= 64 {
            return Err(Error::ResourceLimit);
        }
        validate_sequence(&row, paths.len())?;
        let stored_path_bytes = read_usize(&row, 2)?;
        if stored_path_bytes > 131_072 {
            return Err(Error::ResourceLimit);
        }
        if stored_path_bytes == 0 {
            return Err(Error::InvalidStoredData);
        }
        let bytes: Vec<u8> = row.get(1).map_err(Error::Read)?;
        paths.push(RepositoryPath::try_from_bytes(bytes).map_err(|_| Error::InvalidStoredData)?);
    }
    Ok(paths)
}

fn read_usize(row: &libsql::Row, index: i32) -> Result<usize, Error> {
    usize::try_from(read_i64(row, index)?).map_err(|_| Error::InvalidStoredData)
}

async fn reject_orphan_diff_paths(
    transaction: &Transaction,
    goal: GoalContractReference,
) -> Result<(), Error> {
    let mut rows = transaction
        .query(
            "SELECT EXISTS(SELECT 1 FROM goal_criterion_diff_paths p
          WHERE p.task_id = ?1 AND p.goal_revision = ?2 AND NOT EXISTS (
            SELECT 1 FROM goal_criterion_origins o WHERE o.task_id = p.task_id
              AND o.goal_revision = p.goal_revision AND o.criterion_id = p.criterion_id))",
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

pub(super) fn scope_text(scope: VerificationScope) -> &'static str {
    match scope {
        VerificationScope::Targeted => "targeted",
        VerificationScope::Package => "package",
        VerificationScope::Workspace => "workspace",
    }
}

fn parse_scope(value: &str) -> Result<VerificationScope, Error> {
    match value {
        "targeted" => Ok(VerificationScope::Targeted),
        "package" => Ok(VerificationScope::Package),
        "workspace" => Ok(VerificationScope::Workspace),
        _ => Err(Error::InvalidStoredData),
    }
}

pub(super) fn diff_text(mode: DiffInvariantMode) -> &'static str {
    match mode {
        DiffInvariantMode::NoChanges => "no_changes",
        DiffInvariantMode::OnlyPaths => "only_paths",
        DiffInvariantMode::ExactPaths => "exact_paths",
        DiffInvariantMode::NonEmptyChanges => "non_empty_changes",
    }
}

fn parse_diff(value: &str) -> Result<DiffInvariantMode, Error> {
    match value {
        "no_changes" => Ok(DiffInvariantMode::NoChanges),
        "only_paths" => Ok(DiffInvariantMode::OnlyPaths),
        "exact_paths" => Ok(DiffInvariantMode::ExactPaths),
        "non_empty_changes" => Ok(DiffInvariantMode::NonEmptyChanges),
        _ => Err(Error::InvalidStoredData),
    }
}
