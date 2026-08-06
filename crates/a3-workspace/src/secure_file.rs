use crate::path_policy::open_regular_no_follow;
use crate::platform_path;
use crate::{PathEntryKind, PathPolicy};
use a3_domain::{
    ContentHash, DiscoveryExclusionReason, DiscoveryPolicy, FileRevision,
    SecretCandidateClassifierV1,
};
use std::fs::File;
use std::io::Read;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SecureFileReadError {
    Denied,
    Unavailable,
    Stale,
    TooLarge,
    InvalidEncoding,
    Binary,
    SecretCandidate,
    Cancelled,
}

pub(crate) fn read_verified_text(
    root: &Path,
    expected: &FileRevision,
    is_cancelled: impl Fn() -> bool,
) -> Result<Vec<u8>, SecureFileReadError> {
    if is_cancelled() {
        return Err(SecureFileReadError::Cancelled);
    }
    let observation_policy = DiscoveryPolicy::v1();
    if let Some(reason) =
        observation_policy.classify_built_in_path(expected.path().as_bytes(), false)
    {
        return Err(map_exclusion(reason));
    }
    let relative = platform_path::repository_path(expected.path())
        .map_err(|_| SecureFileReadError::InvalidEncoding)?;
    let policy = PathPolicy::from_selected_root(root).map_err(|_| SecureFileReadError::Denied)?;
    let canonical = policy
        .resolve_existing(&relative)
        .map_err(|_| SecureFileReadError::Denied)?;
    if canonical.kind() != PathEntryKind::File {
        return Err(SecureFileReadError::Unavailable);
    }
    let mut file = open_regular_no_follow(canonical.as_path())
        .map_err(|_| SecureFileReadError::Unavailable)?;
    let metadata = file
        .metadata()
        .map_err(|_| SecureFileReadError::Unavailable)?;
    if metadata.len() > observation_policy.max_file_bytes() {
        return Err(SecureFileReadError::TooLarge);
    }
    let bytes = read_bounded(
        &mut file,
        observation_policy.max_file_bytes(),
        &is_cancelled,
    )?;
    if is_cancelled() {
        return Err(SecureFileReadError::Cancelled);
    }
    let metadata_after = file
        .metadata()
        .map_err(|_| SecureFileReadError::Unavailable)?;
    if metadata_after.len() != metadata.len() {
        return Err(SecureFileReadError::Stale);
    }
    let canonical_after = policy
        .resolve_existing(&relative)
        .map_err(|_| SecureFileReadError::Denied)?;
    if canonical_after.kind() != PathEntryKind::File {
        return Err(SecureFileReadError::Unavailable);
    }
    if canonical_after.as_path() != canonical.as_path() {
        return Err(SecureFileReadError::Stale);
    }
    let prefix_length = bytes
        .len()
        .min(observation_policy.inspection_prefix_bytes());
    if let Some(reason) = observation_policy.classify_content_prefix(&bytes[..prefix_length]) {
        return Err(map_exclusion(reason));
    }
    let actual_hash = ContentHash::from_bytes(*blake3::hash(&bytes).as_bytes());
    if actual_hash != expected.content_hash() {
        return Err(SecureFileReadError::Stale);
    }
    let text = std::str::from_utf8(&bytes).map_err(|_| SecureFileReadError::InvalidEncoding)?;
    if SecretCandidateClassifierV1::classify(text).is_some() {
        return Err(SecureFileReadError::SecretCandidate);
    }
    Ok(bytes)
}

fn read_bounded(
    file: &mut File,
    maximum_bytes: u64,
    is_cancelled: &impl Fn() -> bool,
) -> Result<Vec<u8>, SecureFileReadError> {
    let capacity = usize::try_from(maximum_bytes)
        .map_err(|_| SecureFileReadError::TooLarge)?
        .min(64 * 1_024);
    let mut bytes = Vec::with_capacity(capacity);
    let mut buffer = [0u8; 64 * 1_024];
    loop {
        if is_cancelled() {
            return Err(SecureFileReadError::Cancelled);
        }
        let read = file
            .read(&mut buffer)
            .map_err(|_| SecureFileReadError::Unavailable)?;
        if read == 0 {
            break;
        }
        let next_length = bytes
            .len()
            .checked_add(read)
            .ok_or(SecureFileReadError::TooLarge)?;
        if u64::try_from(next_length).map_err(|_| SecureFileReadError::TooLarge)? > maximum_bytes {
            return Err(SecureFileReadError::TooLarge);
        }
        bytes.extend_from_slice(&buffer[..read]);
    }
    Ok(bytes)
}

fn map_exclusion(reason: DiscoveryExclusionReason) -> SecureFileReadError {
    match reason {
        DiscoveryExclusionReason::Secret => SecureFileReadError::SecretCandidate,
        DiscoveryExclusionReason::Binary => SecureFileReadError::Binary,
        DiscoveryExclusionReason::TooLarge => SecureFileReadError::TooLarge,
        DiscoveryExclusionReason::ProjectIgnore
        | DiscoveryExclusionReason::Vendor
        | DiscoveryExclusionReason::Generated
        | DiscoveryExclusionReason::SymbolicLink
        | DiscoveryExclusionReason::SpecialFile => SecureFileReadError::Denied,
    }
}
