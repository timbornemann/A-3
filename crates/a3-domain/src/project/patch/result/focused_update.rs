//! Exact complete-line excerpts of one existing Update's checked byte effect.
use super::{
    MAX_PATCH_CONTENT_PREVIEW_BYTES, PatchContentPreview, PatchFileContent, PatchPreviewEntry,
    PatchPreviewError,
};
use crate::PatchUpdate;

impl PatchPreviewEntry {
    /// Shows every changed line while omitting only equal exterior bytes.
    /// The adapter supplies the same hash-checked full source used by E3 preflight.
    pub fn focused_update(
        update: &PatchUpdate,
        before: &PatchFileContent,
        remaining_bytes: usize,
    ) -> Result<Self, PatchPreviewError> {
        if before.content_hash() != update.expected().content_hash() {
            return Err(PatchPreviewError::OperationMismatch);
        }
        let after = update.content();
        let old = before.as_bytes();
        let new = after.as_bytes();
        let common_prefix = old.iter().zip(new).take_while(|(a, b)| a == b).count();
        let common_suffix = old[common_prefix..]
            .iter()
            .rev()
            .zip(new[common_prefix..].iter().rev())
            .take_while(|(a, b)| a == b)
            .count();

        // The byte comparison may stop inside a code point or between CR and LF.
        // Both excerpts start at the same complete-line boundary in the shared prefix.
        let start = (0..=common_prefix)
            .rev()
            .find(|&position| is_line_start(old, position) && is_line_start(new, position))
            .ok_or(PatchPreviewError::OperationMismatch)?;
        // Include the same suffix bytes on both sides until both lines are complete.
        // An insertion can change whether the first shared suffix bytes start a line.
        let omitted_suffix = (0..=common_suffix)
            .rev()
            .find(|&count| {
                is_line_end(old, old.len() - count) && is_line_end(new, new.len() - count)
            })
            .ok_or(PatchPreviewError::OperationMismatch)?;
        let old_end = old.len() - omitted_suffix;
        let new_end = new.len() - omitted_suffix;
        if old[..start] != new[..start] || old[old_end..] != new[new_end..] {
            return Err(PatchPreviewError::OperationMismatch);
        }
        let old_excerpt = &old[start..old_end];
        let new_excerpt = &new[start..new_end];
        if old_excerpt.len() > MAX_PATCH_CONTENT_PREVIEW_BYTES
            || new_excerpt.len() > MAX_PATCH_CONTENT_PREVIEW_BYTES
            || old_excerpt.len().saturating_add(new_excerpt.len()) > remaining_bytes
        {
            return Err(PatchPreviewError::PreviewTooLarge);
        }
        let first_line = absolute_line(old, start)?;
        let path = update.expected().path();
        Ok(Self::new(
            Some(path.clone()),
            Some(path.clone()),
            Some(PatchContentPreview::from_retained(
                before,
                old_excerpt,
                first_line,
            )?),
            Some(PatchContentPreview::from_retained(
                after,
                new_excerpt,
                first_line,
            )?),
        ))
    }
}

fn is_line_start(bytes: &[u8], position: usize) -> bool {
    if position == 0 {
        return true;
    }
    match bytes.get(position - 1) {
        Some(b'\n') => true,
        Some(b'\r') => bytes.get(position) != Some(&b'\n'),
        _ => false,
    }
}

fn is_line_end(bytes: &[u8], position: usize) -> bool {
    position == bytes.len() || is_line_start(bytes, position)
}

fn absolute_line(bytes: &[u8], position: usize) -> Result<u32, PatchPreviewError> {
    let endings = bytes[..position]
        .iter()
        .enumerate()
        .filter(|&(index, byte)| {
            *byte == b'\n' || (*byte == b'\r' && bytes.get(index + 1) != Some(&b'\n'))
        })
        .count();
    u32::try_from(endings.saturating_add(1)).map_err(|_| PatchPreviewError::PreviewTooLarge)
}
