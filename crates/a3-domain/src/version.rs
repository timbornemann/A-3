use std::error::Error;
use std::fmt;

const MAX_APPLICATION_VERSION_LENGTH: usize = 64;

/// Validated, opaque version identifier for the running A^3 application.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ApplicationVersion(Box<str>);

impl ApplicationVersion {
    /// Returns the validated version identifier.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<&str> for ApplicationVersion {
    type Error = ApplicationVersionError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        if value.is_empty() {
            return Err(ApplicationVersionError::Empty);
        }

        if value.len() > MAX_APPLICATION_VERSION_LENGTH {
            return Err(ApplicationVersionError::TooLong {
                actual: value.len(),
                maximum: MAX_APPLICATION_VERSION_LENGTH,
            });
        }

        if let Some((index, character)) = value
            .char_indices()
            .find(|(_, character)| !is_version_character(*character))
        {
            return Err(ApplicationVersionError::InvalidCharacter { index, character });
        }

        Ok(Self(value.into()))
    }
}

impl TryFrom<String> for ApplicationVersion {
    type Error = ApplicationVersionError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::try_from(value.as_str())
    }
}

impl fmt::Display for ApplicationVersion {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

const fn is_version_character(character: char) -> bool {
    character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '+')
}

/// Validation failure for an [`ApplicationVersion`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ApplicationVersionError {
    /// The version identifier was empty.
    Empty,
    /// The version identifier exceeded its bounded representation.
    TooLong {
        /// Actual byte length of the supplied identifier.
        actual: usize,
        /// Maximum accepted byte length.
        maximum: usize,
    },
    /// The version identifier contained a character outside its safe alphabet.
    InvalidCharacter {
        /// Byte index of the invalid character.
        index: usize,
        /// Invalid character.
        character: char,
    },
}

impl fmt::Display for ApplicationVersionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("application version must not be empty"),
            Self::TooLong { actual, maximum } => write!(
                formatter,
                "application version length {actual} exceeds maximum {maximum}"
            ),
            Self::InvalidCharacter { index, character } => write!(
                formatter,
                "application version contains invalid character {character:?} at byte index {index}"
            ),
        }
    }
}

impl Error for ApplicationVersionError {}

#[cfg(test)]
mod tests {
    use super::{ApplicationVersion, ApplicationVersionError, MAX_APPLICATION_VERSION_LENGTH};

    #[test]
    fn accepts_bounded_version_identifier() -> Result<(), ApplicationVersionError> {
        let version = ApplicationVersion::try_from("0.1.0-alpha.1+local")?;

        assert_eq!(version.as_str(), "0.1.0-alpha.1+local");
        Ok(())
    }

    #[test]
    fn rejects_empty_version_identifier() {
        assert_eq!(
            ApplicationVersion::try_from(""),
            Err(ApplicationVersionError::Empty)
        );
    }

    #[test]
    fn rejects_unbounded_version_identifier() {
        let value = "1".repeat(MAX_APPLICATION_VERSION_LENGTH + 1);

        assert_eq!(
            ApplicationVersion::try_from(value),
            Err(ApplicationVersionError::TooLong {
                actual: MAX_APPLICATION_VERSION_LENGTH + 1,
                maximum: MAX_APPLICATION_VERSION_LENGTH,
            })
        );
    }

    #[test]
    fn rejects_characters_outside_safe_alphabet() {
        assert_eq!(
            ApplicationVersion::try_from("1.0 release"),
            Err(ApplicationVersionError::InvalidCharacter {
                index: 3,
                character: ' ',
            })
        );
    }
}
