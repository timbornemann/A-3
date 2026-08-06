//! Content-local Secret Candidate validation before provider request construction.

use a3_application::ContextCompileFailure;
use a3_domain::SecretCandidateClassifierV1;

pub(super) fn reject_secret_candidate(value: &str) -> Result<(), ContextCompileFailure> {
    if SecretCandidateClassifierV1::classify(value).is_some() {
        Err(ContextCompileFailure::SecretCandidate)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::reject_secret_candidate;
    use a3_application::ContextCompileFailure;

    #[test]
    fn candidates_are_specific_without_matching_normal_words() {
        assert!(reject_secret_candidate("the Kakia module is ordinary text").is_ok());
        assert!(reject_secret_candidate("AKIA-short").is_ok());
        assert_eq!(
            reject_secret_candidate("AKIAIOSFODNN7EXAMPLE"),
            Err(ContextCompileFailure::SecretCandidate)
        );
        assert_eq!(
            reject_secret_candidate("authorization: bearer fixture-secret"),
            Err(ContextCompileFailure::SecretCandidate)
        );
    }
}
