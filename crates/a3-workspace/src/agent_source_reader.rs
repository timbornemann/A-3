use crate::path_policy::open_regular_no_follow;
use crate::platform_path;
use crate::{PathEntryKind, PathPolicy};
use a3_application::{
    AgentSourcePage, AgentSourceReadControl, AgentSourceReadFailure, AgentSourceReader,
    AgentSourceReaderFuture,
};
use a3_domain::{
    AgentFileInspection, AgentFileStartLine, ContentHash, DiscoveryExclusionReason,
    DiscoveryPolicy, FileRevision, ProjectIdentity, SecretCandidateClassifierV1, SourcePosition,
    SourceRange,
};
use std::fs::File;
use std::io::Read;
use std::path::Path;

const MAX_AGENT_SOURCE_PAGE_BYTES: usize = 12 * 1_024;

/// Safe local adapter for content-addressed, complete-line source pages.
#[derive(Debug, Clone, Copy, Default)]
pub struct WorkspaceAgentSourceReader;

impl AgentSourceReader for WorkspaceAgentSourceReader {
    fn read_page<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        expected_revision: &'a FileRevision,
        request: &'a AgentFileInspection,
        control: &'a dyn AgentSourceReadControl,
    ) -> AgentSourceReaderFuture<'a> {
        Box::pin(async move {
            read_page_from_root(
                project.worktree().root().as_path(),
                expected_revision,
                request,
                control,
            )
        })
    }
}

fn read_page_from_root(
    root: &Path,
    expected_revision: &FileRevision,
    request: &AgentFileInspection,
    control: &dyn AgentSourceReadControl,
) -> Result<AgentSourcePage, AgentSourceReadFailure> {
    if control.is_cancelled() {
        return Err(AgentSourceReadFailure::Cancelled);
    }
    if request.path() != expected_revision.path() {
        return Err(AgentSourceReadFailure::Denied);
    }
    let observation_policy = DiscoveryPolicy::v1();
    if let Some(reason) =
        observation_policy.classify_built_in_path(expected_revision.path().as_bytes(), false)
    {
        return Err(map_exclusion(reason));
    }
    let relative = platform_path::repository_path(expected_revision.path())
        .map_err(|_| AgentSourceReadFailure::InvalidEncoding)?;
    let policy =
        PathPolicy::from_selected_root(root).map_err(|_| AgentSourceReadFailure::Denied)?;
    let canonical = policy
        .resolve_existing(&relative)
        .map_err(|_| AgentSourceReadFailure::Denied)?;
    if canonical.kind() != PathEntryKind::File {
        return Err(AgentSourceReadFailure::Unavailable);
    }
    let mut file = open_regular_no_follow(canonical.as_path())
        .map_err(|_| AgentSourceReadFailure::Unavailable)?;
    let metadata = file
        .metadata()
        .map_err(|_| AgentSourceReadFailure::Unavailable)?;
    if metadata.len() > DiscoveryPolicy::v1().max_file_bytes() {
        return Err(AgentSourceReadFailure::FileTooLarge);
    }
    let bytes = read_bounded(&mut file, observation_policy.max_file_bytes(), control)?;
    if control.is_cancelled() {
        return Err(AgentSourceReadFailure::Cancelled);
    }
    let metadata_after = file
        .metadata()
        .map_err(|_| AgentSourceReadFailure::Unavailable)?;
    if metadata_after.len() != metadata.len() {
        return Err(AgentSourceReadFailure::Stale);
    }
    let canonical_after = policy
        .resolve_existing(&relative)
        .map_err(|_| AgentSourceReadFailure::Denied)?;
    if canonical_after.kind() != PathEntryKind::File {
        return Err(AgentSourceReadFailure::Unavailable);
    }
    if canonical_after.as_path() != canonical.as_path() {
        return Err(AgentSourceReadFailure::Stale);
    }
    let prefix_length = bytes
        .len()
        .min(observation_policy.inspection_prefix_bytes());
    if let Some(reason) = observation_policy.classify_content_prefix(&bytes[..prefix_length]) {
        return Err(map_exclusion(reason));
    }
    let actual_hash = ContentHash::from_bytes(*blake3::hash(&bytes).as_bytes());
    if actual_hash != expected_revision.content_hash() {
        return Err(AgentSourceReadFailure::Stale);
    }
    let text = std::str::from_utf8(&bytes).map_err(|_| AgentSourceReadFailure::InvalidEncoding)?;
    if SecretCandidateClassifierV1::classify(text).is_some() {
        return Err(AgentSourceReadFailure::SecretCandidate);
    }
    build_page(expected_revision.clone(), request, text, control)
}

fn read_bounded(
    file: &mut File,
    maximum_bytes: u64,
    control: &dyn AgentSourceReadControl,
) -> Result<Vec<u8>, AgentSourceReadFailure> {
    let capacity = usize::try_from(maximum_bytes)
        .map_err(|_| AgentSourceReadFailure::FileTooLarge)?
        .min(64 * 1_024);
    let mut bytes = Vec::with_capacity(capacity);
    let mut buffer = [0u8; 64 * 1_024];
    loop {
        if control.is_cancelled() {
            return Err(AgentSourceReadFailure::Cancelled);
        }
        let read = file
            .read(&mut buffer)
            .map_err(|_| AgentSourceReadFailure::Unavailable)?;
        if read == 0 {
            break;
        }
        let next_length = bytes
            .len()
            .checked_add(read)
            .ok_or(AgentSourceReadFailure::FileTooLarge)?;
        if u64::try_from(next_length).map_err(|_| AgentSourceReadFailure::FileTooLarge)?
            > maximum_bytes
        {
            return Err(AgentSourceReadFailure::FileTooLarge);
        }
        bytes.extend_from_slice(&buffer[..read]);
    }
    Ok(bytes)
}

fn map_exclusion(reason: DiscoveryExclusionReason) -> AgentSourceReadFailure {
    match reason {
        DiscoveryExclusionReason::Secret => AgentSourceReadFailure::SecretCandidate,
        DiscoveryExclusionReason::Binary => AgentSourceReadFailure::BinaryContent,
        DiscoveryExclusionReason::TooLarge => AgentSourceReadFailure::FileTooLarge,
        DiscoveryExclusionReason::ProjectIgnore
        | DiscoveryExclusionReason::Vendor
        | DiscoveryExclusionReason::Generated
        | DiscoveryExclusionReason::SymbolicLink
        | DiscoveryExclusionReason::SpecialFile => AgentSourceReadFailure::Denied,
    }
}

fn build_page(
    revision: FileRevision,
    request: &AgentFileInspection,
    text: &str,
    control: &dyn AgentSourceReadControl,
) -> Result<AgentSourcePage, AgentSourceReadFailure> {
    let lines = line_ranges(text);
    let start_index = usize::try_from(request.start_line().get() - 1)
        .map_err(|_| AgentSourceReadFailure::InvalidPage)?;
    if start_index >= lines.len() {
        let point = source_position_at(text, text.len())?;
        let range = SourceRange::new(text.len(), text.len(), point, point)
            .map_err(|_| AgentSourceReadFailure::InvalidPage)?;
        return AgentSourcePage::new(
            revision,
            range,
            request.start_line(),
            String::new(),
            None,
            false,
        )
        .map_err(|_| AgentSourceReadFailure::InvalidPage);
    }

    let maximum_lines = usize::from(request.line_count().get());
    let mut end_index = start_index;
    let mut retained_bytes = 0usize;
    while end_index < lines.len() && end_index - start_index < maximum_lines {
        if control.is_cancelled() {
            return Err(AgentSourceReadFailure::Cancelled);
        }
        let (line_start, line_end) = lines[end_index];
        let line_bytes = line_end - line_start;
        if retained_bytes.saturating_add(line_bytes) > MAX_AGENT_SOURCE_PAGE_BYTES {
            if end_index == start_index {
                return Err(AgentSourceReadFailure::LineTooLong);
            }
            break;
        }
        retained_bytes += line_bytes;
        end_index += 1;
    }
    let start_byte = lines[start_index].0;
    let end_byte = lines[end_index - 1].1;
    let range = SourceRange::new(
        start_byte,
        end_byte,
        source_position_at(text, start_byte)?,
        source_position_at(text, end_byte)?,
    )
    .map_err(|_| AgentSourceReadFailure::InvalidPage)?;
    let truncated = end_index < lines.len();
    let next_start_line = if truncated {
        let one_based = u32::try_from(end_index)
            .ok()
            .and_then(|value| value.checked_add(1))
            .ok_or(AgentSourceReadFailure::InvalidPage)?;
        Some(AgentFileStartLine::new(one_based).map_err(|_| AgentSourceReadFailure::InvalidPage)?)
    } else {
        None
    };
    AgentSourcePage::new(
        revision,
        range,
        request.start_line(),
        text[start_byte..end_byte].to_owned(),
        next_start_line,
        truncated,
    )
    .map_err(|_| AgentSourceReadFailure::InvalidPage)
}

fn line_ranges(text: &str) -> Vec<(usize, usize)> {
    let mut lines = Vec::new();
    let mut start = 0usize;
    for (index, byte) in text.bytes().enumerate() {
        if byte == b'\n' {
            lines.push((start, index + 1));
            start = index + 1;
        }
    }
    if start < text.len() {
        lines.push((start, text.len()));
    }
    lines
}

fn source_position_at(
    text: &str,
    byte_offset: usize,
) -> Result<SourcePosition, AgentSourceReadFailure> {
    let prefix = text
        .get(..byte_offset)
        .ok_or(AgentSourceReadFailure::InvalidPage)?;
    let row = u32::try_from(prefix.bytes().filter(|byte| *byte == b'\n').count())
        .map_err(|_| AgentSourceReadFailure::InvalidPage)?;
    let column_start = prefix.rfind('\n').map_or(0, |index| index + 1);
    let column = u32::try_from(byte_offset - column_start)
        .map_err(|_| AgentSourceReadFailure::InvalidPage)?;
    Ok(SourcePosition::new(row, column))
}

#[cfg(test)]
mod tests {
    use super::*;
    use a3_domain::{AgentFileLineCount, RepositoryPath};

    #[derive(Debug)]
    struct Active;

    impl AgentSourceReadControl for Active {
        fn is_cancelled(&self) -> bool {
            false
        }
    }

    #[test]
    fn complete_line_pages_have_forward_cursor_and_exact_ranges()
    -> Result<(), Box<dyn std::error::Error>> {
        let path = RepositoryPath::try_from_bytes(b"src/lib.rs".to_vec())?;
        let text = "alpha\nbeta\ngamma";
        let revision = FileRevision::new(
            path.clone(),
            ContentHash::from_bytes(*blake3::hash(text.as_bytes()).as_bytes()),
        );
        let request = AgentFileInspection::new(
            path,
            AgentFileStartLine::new(2)?,
            AgentFileLineCount::new(1)?,
        );
        let page = build_page(revision, &request, text, &Active)?;

        assert_eq!(page.text(), "beta\n");
        assert_eq!(page.range().start_byte(), 6);
        assert_eq!(page.range().end_byte(), 11);
        assert_eq!(page.next_start_line(), Some(AgentFileStartLine::new(3)?));
        assert!(page.truncated());
        Ok(())
    }

    #[test]
    fn oversized_first_line_is_rejected_instead_of_split() -> Result<(), Box<dyn std::error::Error>>
    {
        let path = RepositoryPath::try_from_bytes(b"large.rs".to_vec())?;
        let text = "x".repeat(MAX_AGENT_SOURCE_PAGE_BYTES + 1);
        let revision = FileRevision::new(
            path.clone(),
            ContentHash::from_bytes(*blake3::hash(text.as_bytes()).as_bytes()),
        );
        let request = AgentFileInspection::new(
            path,
            AgentFileStartLine::new(1)?,
            AgentFileLineCount::new(1)?,
        );

        assert_eq!(
            build_page(revision, &request, &text, &Active),
            Err(AgentSourceReadFailure::LineTooLong)
        );
        Ok(())
    }
}
