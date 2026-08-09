use a3_domain::ProcessEnvironmentVariable;
use std::collections::BTreeMap;
use std::error::Error;
use std::ffi::{OsStr, OsString};
use std::fmt;

const MAX_HOST_ENVIRONMENT_VARIABLES: usize = 64;

/// Explicit adapter-owned host values from which a `ProcessSpec` may request a subset.
#[derive(Clone, Default)]
pub struct ProcessHostEnvironment {
    values: BTreeMap<ProcessEnvironmentVariable, OsString>,
}

impl ProcessHostEnvironment {
    /// Creates a bounded environment snapshot without reading the ambient process environment.
    pub fn new(
        values: Vec<(ProcessEnvironmentVariable, OsString)>,
    ) -> Result<Self, ProcessHostEnvironmentError> {
        if values.len() > MAX_HOST_ENVIRONMENT_VARIABLES {
            return Err(ProcessHostEnvironmentError::TooManyVariables {
                actual: values.len(),
            });
        }
        let mut canonical = BTreeMap::new();
        for (name, value) in values {
            if canonical.insert(name, value).is_some() {
                return Err(ProcessHostEnvironmentError::DuplicateVariable);
            }
        }
        Ok(Self { values: canonical })
    }

    /// Captures only the explicitly named variables; missing values stay unavailable.
    pub fn capture(
        variables: Vec<ProcessEnvironmentVariable>,
    ) -> Result<Self, ProcessHostEnvironmentError> {
        if variables.len() > MAX_HOST_ENVIRONMENT_VARIABLES {
            return Err(ProcessHostEnvironmentError::TooManyVariables {
                actual: variables.len(),
            });
        }
        let values = variables
            .into_iter()
            .filter_map(|name| std::env::var_os(name.as_str()).map(|value| (name, value)))
            .collect();
        Self::new(values)
    }

    pub(crate) fn value(&self, name: &ProcessEnvironmentVariable) -> Option<&OsStr> {
        self.values.get(name).map(OsString::as_os_str)
    }

    pub(crate) fn value_by_name(&self, name: &str) -> Option<&OsStr> {
        self.values.iter().find_map(|(candidate, value)| {
            (candidate.as_str() == name).then_some(value.as_os_str())
        })
    }
}

impl fmt::Debug for ProcessHostEnvironment {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProcessHostEnvironment")
            .field("variable_count", &self.values.len())
            .finish_non_exhaustive()
    }
}

/// Explicit host environment exceeded its fixed size or repeated a canonical name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessHostEnvironmentError {
    /// More than 64 values were configured.
    TooManyVariables {
        /// Observed value count.
        actual: usize,
    },
    /// One canonical name appeared more than once.
    DuplicateVariable,
}

impl fmt::Display for ProcessHostEnvironmentError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::TooManyVariables { .. } => "process host environment exceeds 64 variables",
            Self::DuplicateVariable => "process host environment contains a duplicate variable",
        })
    }
}

impl Error for ProcessHostEnvironmentError {}
