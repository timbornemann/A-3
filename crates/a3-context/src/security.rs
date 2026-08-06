//! Content-local Secret Candidate validation before provider request construction.

use a3_application::ContextCompileFailure;

pub(super) fn reject_secret_candidate(value: &str) -> Result<(), ContextCompileFailure> {
    let lower = value.to_ascii_lowercase();
    let markers = [
        "-----begin private key-----",
        "-----begin rsa private key-----",
        "authorization: bearer ",
        "github_pat_",
        "ghp_",
    ];
    if markers.iter().any(|marker| lower.contains(marker))
        || contains_aws_access_key(value)
        || lower.lines().any(line_has_secret_assignment)
    {
        Err(ContextCompileFailure::SecretCandidate)
    } else {
        Ok(())
    }
}

fn contains_aws_access_key(value: &str) -> bool {
    value.as_bytes().windows(20).any(|candidate| {
        candidate.starts_with(b"AKIA")
            && candidate[4..]
                .iter()
                .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit())
    })
}

fn line_has_secret_assignment(line: &str) -> bool {
    let trimmed = line.trim();
    ["password=", "password:", "api_key=", "secret=", "token="]
        .iter()
        .any(|prefix| {
            trimmed.starts_with(prefix)
                && trimmed[prefix.len()..]
                    .trim_matches(|character: char| {
                        character.is_whitespace() || matches!(character, '"' | '\'')
                    })
                    .len()
                    >= 8
        })
}

#[cfg(test)]
mod tests {
    use super::{contains_aws_access_key, reject_secret_candidate};
    use a3_application::ContextCompileFailure;

    #[test]
    fn candidates_are_specific_without_matching_normal_words() {
        assert!(reject_secret_candidate("the Kakia module is ordinary text").is_ok());
        assert!(!contains_aws_access_key("AKIA-short"));
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
