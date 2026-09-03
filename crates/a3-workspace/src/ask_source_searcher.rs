use crate::secure_file::{SecureFileReadError, read_verified_text};
use a3_application::{
    AskSourceSearchControl, AskSourceSearchFailure, AskSourceSearcher, AskSourceSearcherFuture,
    AskSourceTextHit, AskSourceTextSearch, AskSourceTextSearchResult,
};
use a3_domain::{
    AskResearchCompleteness, FileRevision, ProjectIdentity, PublishedIndex, SourcePosition,
    SourceRange,
};
use std::time::{Duration, Instant};

const MAX_HITS: usize = 100;
const MAX_FILES: usize = 2_000;
const MAX_BYTES: u64 = 32 * 1_024 * 1_024;
const MAX_DURATION: Duration = Duration::from_secs(30);

/// Safe adapter for bounded literal searches over current indexed local source.
#[derive(Debug, Clone, Copy, Default)]
pub struct WorkspaceAskSourceSearcher;

impl AskSourceSearcher for WorkspaceAskSourceSearcher {
    fn search<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        published: &'a PublishedIndex,
        query: &'a AskSourceTextSearch,
        control: &'a dyn AskSourceSearchControl,
    ) -> AskSourceSearcherFuture<'a> {
        Box::pin(async move { search(project, published, query, control) })
    }
}

fn search(
    project: &ProjectIdentity,
    published: &PublishedIndex,
    query: &AskSourceTextSearch,
    control: &dyn AskSourceSearchControl,
) -> Result<AskSourceTextSearchResult, AskSourceSearchFailure> {
    let started = Instant::now();
    let files = published.publication().graph().files();
    let mut hits = Vec::new();
    let mut examined = 0usize;
    let mut examined_bytes = 0u64;
    let mut limited = files.len() > MAX_FILES;
    for revision in files.iter().take(MAX_FILES) {
        if control.is_cancelled() {
            return Err(AskSourceSearchFailure::Cancelled);
        }
        if started.elapsed() >= MAX_DURATION {
            limited = true;
            break;
        }
        let bytes = match read_verified_text(project.worktree().root().as_path(), revision, || {
            control.is_cancelled()
        }) {
            Ok(bytes) => bytes,
            Err(SecureFileReadError::Cancelled) => return Err(AskSourceSearchFailure::Cancelled),
            Err(SecureFileReadError::Stale | SecureFileReadError::Unavailable) => {
                limited = true;
                continue;
            }
            Err(
                SecureFileReadError::Denied
                | SecureFileReadError::TooLarge
                | SecureFileReadError::InvalidEncoding
                | SecureFileReadError::Binary
                | SecureFileReadError::SecretCandidate,
            ) => continue,
        };
        let byte_count =
            u64::try_from(bytes.len()).map_err(|_| AskSourceSearchFailure::InvalidResult)?;
        if examined_bytes.saturating_add(byte_count) > MAX_BYTES {
            limited = true;
            break;
        }
        examined_bytes = examined_bytes.saturating_add(byte_count);
        examined = examined.saturating_add(1);
        let text =
            std::str::from_utf8(&bytes).map_err(|_| AskSourceSearchFailure::InvalidResult)?;
        collect_file_hits(revision, text, query, &mut hits)?;
        if hits.len() >= MAX_HITS {
            limited = true;
            hits.truncate(MAX_HITS);
            break;
        }
    }
    AskSourceTextSearchResult::new(
        hits,
        u16::try_from(examined).map_err(|_| AskSourceSearchFailure::InvalidResult)?,
        examined_bytes,
        if limited {
            AskResearchCompleteness::Limited
        } else {
            AskResearchCompleteness::Complete
        },
    )
    .map_err(|_| AskSourceSearchFailure::InvalidResult)
}

fn collect_file_hits(
    revision: &FileRevision,
    text: &str,
    query: &AskSourceTextSearch,
    hits: &mut Vec<AskSourceTextHit>,
) -> Result<(), AskSourceSearchFailure> {
    let mut file_offset = 0usize;
    for (row, line) in text.split_inclusive('\n').enumerate() {
        let searchable = line
            .strip_suffix('\n')
            .unwrap_or(line)
            .strip_suffix('\r')
            .unwrap_or(line.strip_suffix('\n').unwrap_or(line));
        let (folded, positions) = fold_with_positions(searchable);
        for literal in query.literals() {
            let needle = literal.to_lowercase();
            let mut offset = 0usize;
            while offset <= folded.len() {
                let Some(relative) = folded[offset..].find(&needle) else {
                    break;
                };
                let lower_start = offset + relative;
                let lower_end = lower_start.saturating_add(needle.len());
                let Some(&original_start) = positions.get(lower_start) else {
                    break;
                };
                let original_end = positions
                    .get(lower_end)
                    .copied()
                    .unwrap_or(searchable.len());
                let absolute_start = file_offset.saturating_add(original_start);
                let absolute_end = file_offset.saturating_add(original_end);
                let row = u32::try_from(row).map_err(|_| AskSourceSearchFailure::InvalidResult)?;
                let range = SourceRange::new(
                    absolute_start,
                    absolute_end,
                    SourcePosition::new(
                        row,
                        u32::try_from(original_start)
                            .map_err(|_| AskSourceSearchFailure::InvalidResult)?,
                    ),
                    SourcePosition::new(
                        row,
                        u32::try_from(original_end)
                            .map_err(|_| AskSourceSearchFailure::InvalidResult)?,
                    ),
                )
                .map_err(|_| AskSourceSearchFailure::InvalidResult)?;
                hits.push(AskSourceTextHit::new(
                    revision.clone(),
                    range,
                    literal.clone(),
                ));
                if hits.len() >= MAX_HITS {
                    return Ok(());
                }
                offset = lower_end.max(lower_start.saturating_add(1));
            }
        }
        file_offset = file_offset.saturating_add(line.len());
    }
    Ok(())
}

fn fold_with_positions(value: &str) -> (String, Vec<usize>) {
    let mut folded = String::new();
    let mut positions = Vec::new();
    for (index, character) in value.char_indices() {
        let lowered = character.to_lowercase().collect::<String>();
        positions.extend(std::iter::repeat_n(index, lowered.len()));
        folded.push_str(&lowered);
    }
    positions.push(value.len());
    (folded, positions)
}

#[cfg(test)]
mod tests {
    use super::{collect_file_hits, fold_with_positions};
    use a3_application::AskSourceTextSearch;
    use a3_domain::{ContentHash, FileRevision, RepositoryPath};

    #[test]
    fn folded_positions_retain_original_byte_offsets() {
        let (folded, positions) = fold_with_positions("A TODO");
        assert_eq!(folded, "a todo");
        assert_eq!(positions[2], 2);
        assert_eq!(positions[6], 6);
    }

    #[test]
    fn literal_search_finds_case_insensitive_matches_beyond_a_file_prefix()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut text = "kein Treffer\n".repeat(200);
        text.push_str("// todo: später beheben\n");
        let revision = FileRevision::new(
            RepositoryPath::try_from_bytes(b"src/late.rs".to_vec())?,
            ContentHash::from_bytes([1; 32]),
        );
        let query = AskSourceTextSearch::new(vec!["TODO".to_owned()])?;
        let mut hits = Vec::new();

        collect_file_hits(&revision, &text, &query, &mut hits)?;

        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].range().start_position().row(), 200);
        assert_eq!(hits[0].literal(), "TODO");
        Ok(())
    }

    #[test]
    fn literal_search_stops_at_the_hundred_hit_boundary() -> Result<(), Box<dyn std::error::Error>>
    {
        let text = "TODO ".repeat(120);
        let revision = FileRevision::new(
            RepositoryPath::try_from_bytes(b"src/many.rs".to_vec())?,
            ContentHash::from_bytes([2; 32]),
        );
        let query = AskSourceTextSearch::new(vec!["todo".to_owned()])?;
        let mut hits = Vec::new();

        collect_file_hits(&revision, &text, &query, &mut hits)?;

        assert_eq!(hits.len(), 100);
        Ok(())
    }
}
