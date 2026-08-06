use a3_domain::{ProjectIdentity, TaskId, TaskLedger};
use std::error::Error;
use std::fmt;
use std::future::Future;
use std::pin::Pin;

/// Owned future returned by the object-safe Task Ledger persistence port.
pub type TaskLedgerStoreFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, TaskLedgerStoreFailure>> + Send + 'a>>;

/// Monotone compare-and-swap version of one materialized durable Task Ledger.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TaskLedgerStoreVersion(u64);

impl TaskLedgerStoreVersion {
    /// Version assigned to an atomically created ledger.
    pub const INITIAL: Self = Self(1);

    /// Reconstructs a non-zero persistence version.
    pub const fn new(value: u64) -> Result<Self, TaskLedgerStoreVersionError> {
        if value == 0 {
            return Err(TaskLedgerStoreVersionError);
        }
        Ok(Self(value))
    }

    /// Returns the persisted integer representation.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Persisted Task Ledger version was zero.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TaskLedgerStoreVersionError;

impl fmt::Display for TaskLedgerStoreVersionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Task Ledger store version must be non-zero")
    }
}

impl Error for TaskLedgerStoreVersionError {}

/// One fully revalidated ledger paired with its optimistic persistence version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredTaskLedger {
    ledger: TaskLedger,
    version: TaskLedgerStoreVersion,
}

impl StoredTaskLedger {
    /// Binds a validated materialized ledger to its non-zero store version.
    #[must_use]
    pub const fn new(ledger: TaskLedger, version: TaskLedgerStoreVersion) -> Self {
        Self { ledger, version }
    }

    /// Returns the reconstructed Task Ledger.
    #[must_use]
    pub const fn ledger(&self) -> &TaskLedger {
        &self.ledger
    }

    /// Returns the optimistic compare-and-swap version.
    #[must_use]
    pub const fn version(&self) -> TaskLedgerStoreVersion {
        self.version
    }

    /// Splits the owned ledger from its store version.
    #[must_use]
    pub fn into_parts(self) -> (TaskLedger, TaskLedgerStoreVersion) {
        (self.ledger, self.version)
    }
}

/// Persistence boundary for one relational materialized Task Ledger per durable task.
pub trait TaskLedgerStore: fmt::Debug + Send + Sync {
    /// Creates one ledger at store version one atomically.
    fn create_task_ledger<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        ledger: &'a TaskLedger,
    ) -> TaskLedgerStoreFuture<'a, TaskLedgerStoreVersion>;

    /// Replaces the materialized projection only when the expected store version is current.
    fn replace_task_ledger<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        expected_version: TaskLedgerStoreVersion,
        ledger: &'a TaskLedger,
    ) -> TaskLedgerStoreFuture<'a, TaskLedgerStoreVersion>;

    /// Loads and revalidates the exact latest materialized state and retained history.
    fn load_task_ledger<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        task_id: TaskId,
    ) -> TaskLedgerStoreFuture<'a, Option<StoredTaskLedger>>;
}

/// Stable application classification of Task Ledger persistence failures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskLedgerStoreFailure {
    /// Local worktree storage could not be reached or written.
    Unavailable,
    /// Local worktree storage failed integrity checks.
    Corrupt,
    /// Database schema is newer than this application build.
    UnsupportedSchema,
    /// Durable content violated relational or domain invariants.
    InvalidStoredData,
    /// A ledger already exists for this task.
    LedgerAlreadyExists,
    /// The task or its Goal Contract does not exist in this worktree.
    TaskNotFound,
    /// Another writer already advanced the materialized store version.
    VersionConflict,
}

impl fmt::Display for TaskLedgerStoreFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Unavailable => "Task Ledger storage is unavailable",
            Self::Corrupt => "Task Ledger storage is corrupt",
            Self::UnsupportedSchema => "Task Ledger storage uses an unsupported schema",
            Self::InvalidStoredData => "Task Ledger storage contains invalid data",
            Self::LedgerAlreadyExists => "Task Ledger already exists",
            Self::TaskNotFound => "Task Ledger task was not found",
            Self::VersionConflict => "Task Ledger store version conflicts with the current state",
        })
    }
}

impl Error for TaskLedgerStoreFailure {}

/// Inbound use case atomically creating the initial durable Task Ledger projection.
#[derive(Debug, Clone, Copy)]
pub struct CreateTaskLedger<'a> {
    store: &'a dyn TaskLedgerStore,
}

impl<'a> CreateTaskLedger<'a> {
    /// Creates the use case from its narrow persistence capability.
    #[must_use]
    pub const fn new(store: &'a dyn TaskLedgerStore) -> Self {
        Self { store }
    }

    /// Persists revision one only together with its complete initial plan graph.
    pub async fn execute(
        self,
        project: &ProjectIdentity,
        ledger: &TaskLedger,
    ) -> Result<StoredTaskLedger, TaskLedgerStoreFailure> {
        let version = self.store.create_task_ledger(project, ledger).await?;
        Ok(StoredTaskLedger::new(ledger.clone(), version))
    }
}

/// Inbound use case atomically compare-and-swapping one domain-validated ledger state.
#[derive(Debug, Clone, Copy)]
pub struct SaveTaskLedger<'a> {
    store: &'a dyn TaskLedgerStore,
}

impl<'a> SaveTaskLedger<'a> {
    /// Creates the use case from its narrow persistence capability.
    #[must_use]
    pub const fn new(store: &'a dyn TaskLedgerStore) -> Self {
        Self { store }
    }

    /// Replaces the relational projection and returns its next optimistic version.
    pub async fn execute(
        self,
        project: &ProjectIdentity,
        expected_version: TaskLedgerStoreVersion,
        ledger: &TaskLedger,
    ) -> Result<StoredTaskLedger, TaskLedgerStoreFailure> {
        let version = self
            .store
            .replace_task_ledger(project, expected_version, ledger)
            .await?;
        Ok(StoredTaskLedger::new(ledger.clone(), version))
    }
}
