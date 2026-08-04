use a3_application::{
    LanguageParseControl, LanguageParseFailure, LanguageParseInput, LanguageParsePolicy,
};
use a3_domain::ContentHash;

const HASH_CHUNK_BYTES: usize = 64 * 1024;

/// Revalidates bounded source bytes against the discovery-time file revision.
///
/// Every adapter calls this before parsing so stale or substituted bytes cannot
/// acquire structural evidence for another content hash.
pub fn verify_language_parse_input(
    input: LanguageParseInput<'_>,
    policy: LanguageParsePolicy,
    control: &dyn LanguageParseControl,
) -> Result<(), LanguageParseFailure> {
    if control.is_cancelled() {
        return Err(LanguageParseFailure::Cancelled);
    }
    if input.source().len() > policy.max_source_bytes() {
        return Err(LanguageParseFailure::InputTooLarge);
    }

    let mut hasher = blake3::Hasher::new();
    for chunk in input.source().chunks(HASH_CHUNK_BYTES) {
        if control.is_cancelled() {
            return Err(LanguageParseFailure::Cancelled);
        }
        hasher.update(chunk);
    }
    let observed = ContentHash::from_bytes(*hasher.finalize().as_bytes());
    if observed != input.revision().content_hash() {
        return Err(LanguageParseFailure::RevisionMismatch);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::verify_language_parse_input;
    use a3_application::{
        LanguageParseControl, LanguageParseControlError, LanguageParseFailure, LanguageParseInput,
        LanguageParsePolicy,
    };
    use a3_domain::{ContentHash, DiscoveredFileRoles, FileRevision, Progress, RepositoryPath};
    use std::error::Error;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Debug, Default)]
    struct CancelDuringHash {
        polls: AtomicUsize,
    }

    impl LanguageParseControl for CancelDuringHash {
        fn is_cancelled(&self) -> bool {
            self.polls.fetch_add(1, Ordering::AcqRel) >= 2
        }

        fn report_progress(&self, _progress: Progress) -> Result<(), LanguageParseControlError> {
            Ok(())
        }
    }

    #[test]
    fn input_revalidation_observes_cancellation_between_hash_chunks() -> Result<(), Box<dyn Error>>
    {
        let source = vec![b'a'; 3 * 64 * 1024];
        let revision = FileRevision::new(
            RepositoryPath::try_from_bytes(b"src/large.rs".to_vec())?,
            ContentHash::from_bytes(*blake3::hash(&source).as_bytes()),
        );
        assert_eq!(
            verify_language_parse_input(
                LanguageParseInput::new(&revision, &source, DiscoveredFileRoles::empty(),),
                LanguageParsePolicy::v1(),
                &CancelDuringHash::default(),
            ),
            Err(LanguageParseFailure::Cancelled)
        );
        Ok(())
    }
}
