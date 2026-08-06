use std::fmt;

/// Version-one classifier for content that must not enter tool previews or model context.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SecretCandidateClassifierV1;

impl SecretCandidateClassifierV1 {
    /// Classifies a bounded UTF-8 value without retaining the matched credential material.
    #[must_use]
    pub fn classify(value: &str) -> Option<SecretCandidateKind> {
        let lower = value.to_ascii_lowercase();
        if [
            "-----begin private key-----",
            "-----begin rsa private key-----",
            "-----begin ec private key-----",
            "-----begin openssh private key-----",
        ]
        .iter()
        .any(|marker| lower.contains(marker))
        {
            return Some(SecretCandidateKind::PrivateKey);
        }
        if lower.contains("authorization: bearer ") {
            return Some(SecretCandidateKind::BearerCredential);
        }
        if lower.contains("github_pat_") || lower.contains("ghp_") {
            return Some(SecretCandidateKind::GitHubCredential);
        }
        if contains_aws_access_key(value.as_bytes()) {
            return Some(SecretCandidateKind::AwsAccessKey);
        }
        lower
            .lines()
            .any(line_has_secret_assignment)
            .then_some(SecretCandidateKind::SecretAssignment)
    }
}

/// Content-free reason a value was classified as a possible secret.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecretCandidateKind {
    /// PEM or OpenSSH private-key material.
    PrivateKey,
    /// HTTP bearer credential.
    BearerCredential,
    /// GitHub token prefix.
    GitHubCredential,
    /// AWS access-key identifier.
    AwsAccessKey,
    /// A known credential field with a non-trivial assigned value.
    SecretAssignment,
}

impl fmt::Display for SecretCandidateKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("possible secret content")
    }
}

fn contains_aws_access_key(value: &[u8]) -> bool {
    value.windows(20).any(|candidate| {
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
    use super::{SecretCandidateClassifierV1, SecretCandidateKind};

    #[test]
    fn classifier_is_content_free_and_does_not_match_normal_words() {
        assert_eq!(
            SecretCandidateClassifierV1::classify("the Kakia module is ordinary text"),
            None
        );
        assert_eq!(
            SecretCandidateClassifierV1::classify("AKIAIOSFODNN7EXAMPLE"),
            Some(SecretCandidateKind::AwsAccessKey)
        );
        assert_eq!(
            SecretCandidateClassifierV1::classify("authorization: bearer fixture-secret-value"),
            Some(SecretCandidateKind::BearerCredential)
        );
        assert_eq!(
            SecretCandidateClassifierV1::classify("password=fixture-secret-value"),
            Some(SecretCandidateKind::SecretAssignment)
        );
        assert_eq!(
            SecretCandidateKind::PrivateKey.to_string(),
            "possible secret content"
        );
    }
}
