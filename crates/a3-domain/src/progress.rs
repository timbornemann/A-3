use std::error::Error;
use std::fmt;
use std::num::NonZeroU64;

/// Validated progress reported by a long-running job.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Progress {
    /// The job cannot yet provide a meaningful total.
    Indeterminate,
    /// The job reports completed units against a fixed non-zero total.
    Determinate {
        /// Number of completed work units.
        completed: u64,
        /// Fixed total number of work units.
        total: NonZeroU64,
    },
}

impl Progress {
    /// Creates determinate progress after validating its bounds.
    pub fn determinate(completed: u64, total: u64) -> Result<Self, ProgressValueError> {
        let total = NonZeroU64::new(total).ok_or(ProgressValueError::ZeroTotal)?;
        if completed > total.get() {
            return Err(ProgressValueError::CompletedExceedsTotal { completed, total });
        }

        Ok(Self::Determinate { completed, total })
    }

    /// Returns the completed units when progress is determinate.
    #[must_use]
    pub const fn completed(self) -> Option<u64> {
        match self {
            Self::Indeterminate => None,
            Self::Determinate { completed, .. } => Some(completed),
        }
    }

    /// Returns the total units when progress is determinate.
    #[must_use]
    pub const fn total(self) -> Option<u64> {
        match self {
            Self::Indeterminate => None,
            Self::Determinate { total, .. } => Some(total.get()),
        }
    }

    /// Returns whether determinate progress reached its total.
    #[must_use]
    pub const fn is_complete(self) -> bool {
        match self {
            Self::Indeterminate => false,
            Self::Determinate { completed, total } => completed == total.get(),
        }
    }

    /// Validates that this observation can follow the previous one.
    pub fn validate_after(self, previous: Self) -> Result<(), ProgressTransitionError> {
        match (previous, self) {
            (Self::Indeterminate, _) => Ok(()),
            (Self::Determinate { .. }, Self::Indeterminate) => {
                Err(ProgressTransitionError::BecameIndeterminate)
            }
            (
                Self::Determinate {
                    total: previous_total,
                    ..
                },
                Self::Determinate { completed, total },
            ) if total != previous_total => Err(ProgressTransitionError::TotalChanged {
                previous: previous_total,
                next: total,
            }),
            (
                Self::Determinate {
                    completed: previous_completed,
                    ..
                },
                Self::Determinate { completed, .. },
            ) if completed < previous_completed => Err(ProgressTransitionError::Regressed {
                previous: previous_completed,
                next: completed,
            }),
            _ => Ok(()),
        }
    }
}

/// Invalid values used to construct determinate progress.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProgressValueError {
    /// A determinate total must contain at least one unit.
    ZeroTotal,
    /// Completed units cannot exceed the fixed total.
    CompletedExceedsTotal {
        /// Rejected completed-unit count.
        completed: u64,
        /// Validated non-zero total.
        total: NonZeroU64,
    },
}

impl fmt::Display for ProgressValueError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroTotal => formatter.write_str("progress total must be non-zero"),
            Self::CompletedExceedsTotal { completed, total } => write!(
                formatter,
                "completed progress {completed} exceeds total {total}"
            ),
        }
    }
}

impl Error for ProgressValueError {}

/// Invalid transition between two otherwise valid progress observations.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProgressTransitionError {
    /// A determinate job cannot return to an indeterminate state.
    BecameIndeterminate,
    /// The fixed total changed during one job execution.
    TotalChanged {
        /// Previous fixed total.
        previous: NonZeroU64,
        /// Rejected replacement total.
        next: NonZeroU64,
    },
    /// Completed units moved backwards.
    Regressed {
        /// Previously accepted completed-unit count.
        previous: u64,
        /// Rejected completed-unit count.
        next: u64,
    },
}

impl fmt::Display for ProgressTransitionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BecameIndeterminate => {
                formatter.write_str("determinate progress cannot become indeterminate")
            }
            Self::TotalChanged { previous, next } => {
                write!(
                    formatter,
                    "progress total changed from {previous} to {next}"
                )
            }
            Self::Regressed { previous, next } => {
                write!(formatter, "progress regressed from {previous} to {next}")
            }
        }
    }
}

impl Error for ProgressTransitionError {}

#[cfg(test)]
mod tests {
    use super::{Progress, ProgressTransitionError, ProgressValueError};

    #[test]
    fn determinate_progress_enforces_bounds() {
        assert_eq!(
            Progress::determinate(0, 0),
            Err(ProgressValueError::ZeroTotal)
        );
        assert!(matches!(
            Progress::determinate(4, 3),
            Err(ProgressValueError::CompletedExceedsTotal { .. })
        ));
        assert_eq!(
            Progress::determinate(3, 3).map(Progress::is_complete),
            Ok(true)
        );
    }

    #[test]
    fn progress_transitions_are_monotone_and_keep_the_total() -> Result<(), ProgressValueError> {
        let first = Progress::determinate(1, 3)?;
        let second = Progress::determinate(2, 3)?;

        assert_eq!(second.validate_after(first), Ok(()));
        assert!(matches!(
            first.validate_after(second),
            Err(ProgressTransitionError::Regressed { .. })
        ));
        assert!(matches!(
            Progress::determinate(2, 4)?.validate_after(second),
            Err(ProgressTransitionError::TotalChanged { .. })
        ));
        assert_eq!(
            Progress::Indeterminate.validate_after(second),
            Err(ProgressTransitionError::BecameIndeterminate)
        );
        Ok(())
    }
}
