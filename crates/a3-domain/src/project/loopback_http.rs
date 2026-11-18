//! Immutable local HTTP verification obligations and bounded terminal observations.

use super::{
    AgentRunId, ContentHash, FileRevision, MachineProcessEffect, PolicyDecisionId,
    PolicyResourceId, ProcessArgument, ProcessEnvironmentVariable, ProcessExecutable,
    ProcessExecutionMode, ProcessNetworkScope, ProcessOutputLimit, ProcessPlanBinding, ProcessSpec,
    ProcessSpecSchemaVersion, ProcessTimeout, RepositoryPath, SnapshotId, TaskEvidenceId,
    TaskStepId, ToolRunId, VerificationRunId, VerificationSpecId, WorkspaceDirectory, WorktreeId,
};
use std::{error::Error, fmt, num::NonZeroU16};

mod mismatch;
mod startup;
pub use mismatch::{LoopbackHttpMismatch, LoopbackHttpMismatchDiagnostic};
pub use startup::{LoopbackFailureExitCode, LoopbackStartupEvidence, LoopbackStartupPhase};

/// Maximum requests in one owned local service invocation.
pub const MAX_LOOPBACK_PROBES: usize = 8;
/// Maximum complete response body admitted by the recipe and transport.
pub const MAX_LOOPBACK_BODY_BYTES: u32 = 8 * 1024;
/// Exact default port supported by the Python invocation without flags.
pub const LOOPBACK_DEFAULT_PORT: u16 = 8765;
/// Milliseconds reserved for the owned stop and final listener observation.
pub const LOOPBACK_CLEANUP_RESERVE_MILLIS: u64 = 2_000;
/// Maximum total runtime for one approved local service recipe.
pub const MAX_LOOPBACK_PROCESS_MILLIS: u64 = 30_000;
const LOOPBACK_HOST: &str = "127.0.0.1";

/// Closed HTTP methods admitted by the local verification recipe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum LoopbackMethod {
    /// GET request.
    Get,
    /// POST request.
    Post,
    /// PUT request.
    Put,
    /// DELETE request.
    Delete,
    /// PATCH request.
    Patch,
}

impl LoopbackMethod {
    /// Returns the canonical HTTP method token.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Post => "POST",
            Self::Put => "PUT",
            Self::Delete => "DELETE",
            Self::Patch => "PATCH",
        }
    }
}

/// One exact HTTP request without authority, query, header or body overrides.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct LoopbackProbe {
    method: LoopbackMethod,
    path: String,
}

impl LoopbackProbe {
    /// Accepts a bounded canonical ASCII origin-form path.
    pub fn new(method: LoopbackMethod, path: String) -> Result<Self, LoopbackHttpError> {
        if path.len() > 256
            || !path
                .strip_prefix('/')
                .is_some_and(|suffix| suffix.is_empty() || valid_relative_path(suffix))
        {
            return Err(LoopbackHttpError);
        }
        Ok(Self { method, path })
    }

    /// Returns the exact request method.
    #[must_use]
    pub const fn method(&self) -> LoopbackMethod {
        self.method
    }

    /// Returns the canonical path sent on the approved local socket.
    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }
}

/// Exact closed argv form used to start the Python service.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum LoopbackStart {
    /// The script is invoked without host and port arguments.
    Default,
    /// The script receives the exact literal loopback host and recipe port.
    Explicit,
}

/// Required response body semantics, without retaining raw response data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum LoopbackExpectedBody {
    /// A complete bounded response is sufficient for this case.
    AnyBounded,
    /// The byte length and full BLAKE3 hash must match exactly.
    Exact {
        /// Expected BLAKE3 hash of the complete body.
        hash: ContentHash,
        /// Expected body length in bytes.
        bytes: u32,
    },
}

/// Positive header and encoding conditions required by a specific response.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct LoopbackHeaderChecks {
    /// Require exact `text/plain; charset=utf-8` and a valid UTF-8 body.
    pub plain_utf8: bool,
    /// Require an exact `Allow: GET` header.
    pub allow_get: bool,
}

/// One ordered HTTP request and the exact properties its result must establish.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct LoopbackHttpCheck {
    probe: LoopbackProbe,
    status: u16,
    body: LoopbackExpectedBody,
    headers: LoopbackHeaderChecks,
}

impl LoopbackHttpCheck {
    /// Admits only terminal HTTP statuses and response bodies within the transport limit.
    pub fn new(
        probe: LoopbackProbe,
        status: u16,
        body: LoopbackExpectedBody,
        headers: LoopbackHeaderChecks,
    ) -> Result<Self, LoopbackHttpError> {
        if !valid_status(status)
            || matches!(body, LoopbackExpectedBody::Exact { bytes, .. } if bytes > MAX_LOOPBACK_BODY_BYTES)
        {
            return Err(LoopbackHttpError);
        }
        Ok(Self {
            probe,
            status,
            body,
            headers,
        })
    }

    /// Returns the exact request to execute for this case.
    #[must_use]
    pub const fn probe(&self) -> &LoopbackProbe {
        &self.probe
    }

    /// Returns the required terminal HTTP status.
    #[must_use]
    pub const fn status(&self) -> u16 {
        self.status
    }

    /// Returns the expected body condition.
    #[must_use]
    pub const fn body(&self) -> LoopbackExpectedBody {
        self.body
    }

    /// Returns the additional required header and encoding conditions.
    #[must_use]
    pub const fn headers(&self) -> LoopbackHeaderChecks {
        self.headers
    }

    /// Checks a complete bounded observation against this required response.
    #[must_use]
    pub fn matches(&self, actual: &LoopbackHttpObservation) -> bool {
        self.status == actual.status
            && (!self.headers.plain_utf8 || actual.headers.plain_utf8)
            && (!self.headers.allow_get || actual.headers.allow_get)
            && match self.body {
                LoopbackExpectedBody::AnyBounded => true,
                LoopbackExpectedBody::Exact { hash, bytes } => {
                    hash == actual.body_hash && bytes == actual.body_bytes
                }
            }
    }
}

/// Immutable original-bound operational recipe for a bounded local service check.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct LoopbackHttpRecipe {
    script: RepositoryPath,
    start: LoopbackStart,
    port: NonZeroU16,
    checks: Vec<LoopbackHttpCheck>,
    default_start_check_index: Option<u8>,
}

impl LoopbackHttpRecipe {
    /// Rejects noncanonical script paths, inconsistent default ports, and duplicate probes.
    pub fn new(
        script: RepositoryPath,
        start: LoopbackStart,
        port: NonZeroU16,
        checks: Vec<LoopbackHttpCheck>,
    ) -> Result<Self, LoopbackHttpError> {
        if loopback_script_path(&script).is_none()
            || (start == LoopbackStart::Default && port.get() != LOOPBACK_DEFAULT_PORT)
            || checks.is_empty()
            || checks.len() > MAX_LOOPBACK_PROBES
            || checks.iter().enumerate().any(|(index, check)| {
                checks[..index]
                    .iter()
                    .any(|previous| previous.probe == check.probe)
            })
        {
            return Err(LoopbackHttpError);
        }
        Ok(Self {
            script,
            start,
            port,
            checks,
            default_start_check_index: None,
        })
    }

    /// Extends an explicit start with a default start and two required rejected starts.
    ///
    /// The default start on port 8765 must repeat the indicated successful GET. The
    /// negative starts must observe an error for the invalid port argument and for the
    /// explicit argv while the Core holds the explicit endpoint.
    pub fn with_startup_coverage(
        mut self,
        default_check_index: u8,
    ) -> Result<Self, LoopbackHttpError> {
        let check = self
            .checks
            .get(usize::from(default_check_index))
            .ok_or(LoopbackHttpError)?;
        if self.default_start_check_index.is_some()
            || self.start != LoopbackStart::Explicit
            || self.port.get() == LOOPBACK_DEFAULT_PORT
            || self.checks.len() >= MAX_LOOPBACK_PROBES
            || check.probe().method() != LoopbackMethod::Get
            || check.status() != 200
            || !matches!(check.body(), LoopbackExpectedBody::Exact { .. })
        {
            return Err(LoopbackHttpError);
        }
        self.default_start_check_index = Some(default_check_index);
        Ok(self)
    }

    /// Returns the primary GET also required on the default start when startup coverage is active.
    #[must_use]
    pub const fn default_start_check_index(&self) -> Option<u8> {
        self.default_start_check_index
    }

    /// Returns the worktree-relative Python script required by the original.
    #[must_use]
    pub const fn script(&self) -> &RepositoryPath {
        &self.script
    }

    /// Returns the exact start argv form.
    #[must_use]
    pub const fn start(&self) -> LoopbackStart {
        self.start
    }

    /// Returns the selected literal IPv4 loopback port.
    #[must_use]
    pub const fn port(&self) -> NonZeroU16 {
        self.port
    }

    /// Returns the ordered HTTP cases for the primary start.
    #[must_use]
    pub fn checks(&self) -> &[LoopbackHttpCheck] {
        &self.checks
    }

    /// Derives the declared listener resource for literal IPv4 loopback and the exact port.
    #[must_use]
    pub fn network_target(port: NonZeroU16) -> PolicyResourceId {
        let mut hash = blake3::Hasher::new_derive_key("a3.loopback-verification.listener.v1");
        hash.update(LOOPBACK_HOST.as_bytes());
        hash.update(&port.get().to_le_bytes());
        PolicyResourceId::from_bytes(*hash.finalize().as_bytes())
    }

    /// Compiles the only admissible direct Python start for this recipe and exact bounds.
    pub fn process_spec(
        &self,
        run: AgentRunId,
        worktree: WorktreeId,
        step: TaskStepId,
        settings: LoopbackProcessSettings,
    ) -> Result<ProcessSpec, LoopbackHttpError> {
        if settings.timeout.as_millis() <= LOOPBACK_CLEANUP_RESERVE_MILLIS
            || settings.timeout.as_millis() > MAX_LOOPBACK_PROCESS_MILLIS
        {
            return Err(LoopbackHttpError);
        }
        let path = loopback_script_path(&self.script).ok_or(LoopbackHttpError)?;
        let mut arguments = vec!["-B".to_owned(), path.to_owned()];
        if self.start == LoopbackStart::Explicit {
            arguments.extend([
                "--host".to_owned(),
                LOOPBACK_HOST.to_owned(),
                "--port".to_owned(),
                self.port.get().to_string(),
            ]);
        }
        let arguments = arguments
            .into_iter()
            .map(ProcessArgument::try_from_string)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| LoopbackHttpError)?;
        let environment = if settings.system_root {
            vec![
                ProcessEnvironmentVariable::try_from_string("SYSTEMROOT".to_owned())
                    .map_err(|_| LoopbackHttpError)?,
            ]
        } else {
            Vec::new()
        };
        ProcessSpec::new(
            ProcessSpecSchemaVersion::V2,
            run,
            worktree,
            ProcessExecutable::try_from_string("python".to_owned())
                .map_err(|_| LoopbackHttpError)?,
            arguments,
            WorkspaceDirectory::Root,
            environment,
            settings.timeout,
            settings.stdout_limit,
            settings.stderr_limit,
            ProcessExecutionMode::Open,
            ProcessPlanBinding::Validated(step),
            ProcessNetworkScope::Requested(Self::network_target(self.port)),
        )
        .map_err(|_| LoopbackHttpError)
    }

    /// Returns the content-free identity of the exact start coverage, requests and expectations.
    #[must_use]
    pub fn id(&self) -> PolicyResourceId {
        let key = if self.default_start_check_index.is_some() {
            "a3.loopback-http-recipe.v2"
        } else {
            "a3.loopback-http-recipe.v1"
        };
        let mut hash = blake3::Hasher::new_derive_key(key);
        hash_field(&mut hash, self.script.as_bytes());
        hash.update(&[match self.start {
            LoopbackStart::Default => 0,
            LoopbackStart::Explicit => 1,
        }]);
        hash.update(&self.port.get().to_le_bytes());
        hash.update(&(self.checks.len() as u64).to_le_bytes());
        for check in &self.checks {
            hash_field(&mut hash, check.probe.method().as_str().as_bytes());
            hash_field(&mut hash, check.probe.path().as_bytes());
            hash.update(&check.status.to_le_bytes());
            hash_headers(&mut hash, check.headers);
            match check.body {
                LoopbackExpectedBody::AnyBounded => {
                    hash.update(&[0]);
                }
                LoopbackExpectedBody::Exact {
                    hash: expected,
                    bytes,
                } => {
                    hash.update(&[1]);
                    hash.update(expected.as_bytes());
                    hash.update(&bytes.to_le_bytes());
                }
            }
        }
        if let Some(index) = self.default_start_check_index {
            hash.update(&[index]);
        }
        PolicyResourceId::from_bytes(*hash.finalize().as_bytes())
    }
}

/// Returns the exact portable script token allowed by the direct Python recipe.
#[must_use]
pub fn loopback_script_path(path: &RepositoryPath) -> Option<&str> {
    let path = std::str::from_utf8(path.as_bytes()).ok()?;
    (!path.starts_with('-') && path.ends_with(".py") && valid_relative_path(path)).then_some(path)
}

fn valid_relative_path(path: &str) -> bool {
    path.split('/').all(|segment| {
        !matches!(segment, "" | "." | "..")
            && segment
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    })
}

const fn valid_status(status: u16) -> bool {
    status >= 200 && status <= 599
}

/// Exact process bounds and the sole optional OS-owned environment entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoopbackProcessSettings {
    /// Total approved process deadline.
    pub timeout: ProcessTimeout,
    /// Exact approved stdout limit.
    pub stdout_limit: ProcessOutputLimit,
    /// Exact approved stderr limit.
    pub stderr_limit: ProcessOutputLimit,
    /// Whether the Windows adapter must supply the canonical `SYSTEMROOT`.
    pub system_root: bool,
}

/// Durable coordinates needed to reconstruct the exact approved start from the recipe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoopbackHttpProcess {
    specification: PolicyResourceId,
    settings: LoopbackProcessSettings,
}

impl LoopbackHttpProcess {
    /// Retains a process identity and all settings needed for exact reconstruction on reopen.
    #[must_use]
    pub const fn new(specification: PolicyResourceId, settings: LoopbackProcessSettings) -> Self {
        Self {
            specification,
            settings,
        }
    }

    /// Extracts the closed settings; the caller must compare against the compiled recipe start.
    pub fn from_spec(spec: &ProcessSpec) -> Result<Self, LoopbackHttpError> {
        if spec.version() != ProcessSpecSchemaVersion::V2
            || spec.machine_effect() != Some(MachineProcessEffect::Unknown)
        {
            return Err(LoopbackHttpError);
        }
        let system_root = match spec.environment_allowlist() {
            [] => false,
            [entry] if entry.as_str() == "SYSTEMROOT" => true,
            _ => return Err(LoopbackHttpError),
        };
        Ok(Self::new(
            spec.specification_id(),
            LoopbackProcessSettings {
                timeout: spec.timeout(),
                stdout_limit: spec.stdout_limit(),
                stderr_limit: spec.stderr_limit(),
                system_root,
            },
        ))
    }

    /// Returns the exact approved process specification identity.
    #[must_use]
    pub const fn specification(self) -> PolicyResourceId {
        self.specification
    }

    /// Returns the immutable settings used to reconstruct the original command.
    #[must_use]
    pub const fn settings(self) -> LoopbackProcessSettings {
        self.settings
    }
}

/// One complete bounded response with its exact policy decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoopbackHttpObservation {
    decision: PolicyDecisionId,
    status: u16,
    body_hash: ContentHash,
    body_bytes: u32,
    headers: LoopbackHeaderChecks,
}

impl LoopbackHttpObservation {
    /// Validates the closed content-free observation before it can be retained as evidence.
    pub fn new(
        decision: PolicyDecisionId,
        status: u16,
        body_hash: ContentHash,
        body_bytes: u32,
        headers: LoopbackHeaderChecks,
    ) -> Result<Self, LoopbackHttpError> {
        if !valid_status(status) || body_bytes > MAX_LOOPBACK_BODY_BYTES {
            return Err(LoopbackHttpError);
        }
        Ok(Self {
            decision,
            status,
            body_hash,
            body_bytes,
            headers,
        })
    }

    /// Returns the policy decision authorizing this exact probe.
    #[must_use]
    pub const fn decision(self) -> PolicyDecisionId {
        self.decision
    }

    /// Returns the observed terminal HTTP status.
    #[must_use]
    pub const fn status(self) -> u16 {
        self.status
    }

    /// Returns the digest of the complete response body.
    #[must_use]
    pub const fn body_hash(self) -> ContentHash {
        self.body_hash
    }

    /// Returns the byte length of the complete response body.
    #[must_use]
    pub const fn body_bytes(self) -> u32 {
        self.body_bytes
    }

    /// Returns the observed header and encoding facts.
    #[must_use]
    pub const fn headers(self) -> LoopbackHeaderChecks {
        self.headers
    }
}

/// Immutable IDs binding one settled HTTP attempt to its active verification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoopbackHttpEvidenceContext {
    /// Exact verification-run identity.
    pub verification_run_id: VerificationRunId,
    /// Exact immutable verification specification.
    pub spec_id: VerificationSpecId,
    /// Controlled run that owned the lifecycle.
    pub run_id: AgentRunId,
    /// Durable attempt opened before the first effect.
    pub tool_run_id: ToolRunId,
    /// Exact active plan step.
    pub step_id: TaskStepId,
    /// Current snapshot verified for this lifecycle.
    pub snapshot_id: SnapshotId,
}

/// Structured evidence for one settled, owned HTTP verification lifecycle.
///
/// The caller verifies actual cleanup and its durable attempt before constructing this artifact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoopbackHttpEvidence {
    id: TaskEvidenceId,
    context: LoopbackHttpEvidenceContext,
    recipe_id: PolicyResourceId,
    script: FileRevision,
    process: LoopbackHttpProcess,
    start_decision: PolicyDecisionId,
    observations: Vec<LoopbackHttpObservation>,
    startup: Option<LoopbackStartupEvidence>,
}

impl LoopbackHttpEvidence {
    /// Admits a bounded ordered report with distinct exact policy decisions.
    pub fn new(
        context: LoopbackHttpEvidenceContext,
        recipe_id: PolicyResourceId,
        script: FileRevision,
        process: LoopbackHttpProcess,
        start_decision: PolicyDecisionId,
        observations: Vec<LoopbackHttpObservation>,
    ) -> Result<Self, LoopbackHttpError> {
        if loopback_script_path(script.path()).is_none()
            || observations.is_empty()
            || observations.len() > MAX_LOOPBACK_PROBES
            || observations.iter().enumerate().any(|(index, observation)| {
                observation.decision == start_decision
                    || observations[..index]
                        .iter()
                        .any(|previous| previous.decision == observation.decision)
            })
        {
            return Err(LoopbackHttpError);
        }
        let mut hash = blake3::Hasher::new_derive_key("a3.loopback-http-evidence.v1");
        hash.update(context.verification_run_id.as_bytes());
        hash.update(context.spec_id.as_bytes());
        hash.update(context.run_id.as_bytes());
        hash.update(context.tool_run_id.as_bytes());
        hash.update(context.step_id.as_bytes());
        hash.update(context.snapshot_id.as_bytes());
        hash.update(recipe_id.as_bytes());
        hash_field(&mut hash, script.path().as_bytes());
        hash.update(script.content_hash().as_bytes());
        hash.update(process.specification().as_bytes());
        let settings = process.settings();
        hash.update(&settings.timeout.as_millis().to_le_bytes());
        hash.update(&settings.stdout_limit.get().to_le_bytes());
        hash.update(&settings.stderr_limit.get().to_le_bytes());
        hash.update(&[u8::from(settings.system_root)]);
        hash.update(start_decision.as_bytes());
        hash.update(&(observations.len() as u64).to_le_bytes());
        for observation in &observations {
            hash.update(observation.decision.as_bytes());
            hash.update(&observation.status.to_le_bytes());
            hash.update(observation.body_hash.as_bytes());
            hash.update(&observation.body_bytes.to_le_bytes());
            hash_headers(&mut hash, observation.headers);
        }
        Ok(Self {
            id: TaskEvidenceId::from_bytes(*hash.finalize().as_bytes()),
            context,
            recipe_id,
            script,
            process,
            start_decision,
            observations,
            startup: None,
        })
    }

    /// Admits the complete observed startup coverage under one settled attempt.
    #[allow(clippy::too_many_arguments)]
    pub fn new_startup(
        context: LoopbackHttpEvidenceContext,
        recipe_id: PolicyResourceId,
        script: FileRevision,
        process: LoopbackHttpProcess,
        free_explicit_decision: PolicyDecisionId,
        explicit_observations: Vec<LoopbackHttpObservation>,
        startup: LoopbackStartupEvidence,
    ) -> Result<Self, LoopbackHttpError> {
        let mut evidence = Self::new(
            context,
            recipe_id,
            script,
            process,
            free_explicit_decision,
            explicit_observations,
        )?;
        let mut ids = std::collections::BTreeSet::from([free_explicit_decision]);
        for id in evidence
            .observations
            .iter()
            .map(|probe| probe.decision())
            .chain(startup.decisions())
        {
            if !ids.insert(id) {
                return Err(LoopbackHttpError);
            }
        }
        let mut hash = blake3::Hasher::new_derive_key("a3.loopback-http-evidence.v2");
        hash.update(evidence.id.as_bytes());
        startup.hash_into(&mut hash);
        evidence.id = TaskEvidenceId::from_bytes(*hash.finalize().as_bytes());
        evidence.startup = Some(startup);
        Ok(evidence)
    }

    /// Returns the additional startup observations when the full coverage was observed.
    #[must_use]
    pub const fn startup(&self) -> Option<&LoopbackStartupEvidence> {
        self.startup.as_ref()
    }

    /// Returns the derived evidence identity, including every observed fact.
    #[must_use]
    pub const fn id(&self) -> TaskEvidenceId {
        self.id
    }

    /// Returns the immutable attempt and snapshot coordinates.
    #[must_use]
    pub const fn context(&self) -> LoopbackHttpEvidenceContext {
        self.context
    }

    /// Returns the exact recipe bound before dispatch.
    #[must_use]
    pub const fn recipe_id(&self) -> PolicyResourceId {
        self.recipe_id
    }

    /// Returns the bound script revision.
    #[must_use]
    pub const fn script(&self) -> &FileRevision {
        &self.script
    }

    /// Returns the approved exact process specification identity.
    #[must_use]
    pub const fn process_specification(&self) -> PolicyResourceId {
        self.process.specification()
    }

    /// Returns the exact process identity and reconstruction settings.
    #[must_use]
    pub const fn process(&self) -> LoopbackHttpProcess {
        self.process
    }

    /// Returns the exact start policy decision.
    #[must_use]
    pub const fn start_decision(&self) -> PolicyDecisionId {
        self.start_decision
    }

    /// Returns the ordered complete responses.
    #[must_use]
    pub fn observations(&self) -> &[LoopbackHttpObservation] {
        &self.observations
    }

    /// Evaluates the exact immutable recipe and all required responses.
    #[must_use]
    pub fn matches(&self, recipe: &LoopbackHttpRecipe) -> bool {
        (match (recipe.default_start_check_index(), self.startup.as_ref()) {
            (None, None) => true,
            (Some(index), Some(startup)) => recipe
                .checks
                .get(usize::from(index))
                .is_some_and(|check| check.matches(&startup.default_observation())),
            _ => false,
        }) && self.recipe_id == recipe.id()
            && self.script.path() == recipe.script()
            && self.observations.len() == recipe.checks.len()
            && recipe
                .checks
                .iter()
                .zip(&self.observations)
                .all(|(check, actual)| check.matches(actual))
    }
}

fn hash_field(hash: &mut blake3::Hasher, bytes: &[u8]) {
    hash.update(&(bytes.len() as u64).to_le_bytes());
    hash.update(bytes);
}

fn hash_headers(hash: &mut blake3::Hasher, checks: LoopbackHeaderChecks) {
    hash.update(&[u8::from(checks.plain_utf8), u8::from(checks.allow_get)]);
}

/// A recipe or observation exceeded its closed structural contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoopbackHttpError;

impl fmt::Display for LoopbackHttpError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("invalid closed loopback HTTP contract")
    }
}

impl Error for LoopbackHttpError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn startup_coverage_binds_the_default_get_and_cannot_use_a_single_start_receipt()
    -> Result<(), Box<dyn Error>> {
        let path = RepositoryPath::try_from_bytes(b"server.py".to_vec())?;
        let body_hash = ContentHash::from_bytes([9; 32]);
        let checks = vec![
            LoopbackHttpCheck::new(
                LoopbackProbe::new(LoopbackMethod::Get, "/".to_owned())?,
                200,
                LoopbackExpectedBody::Exact {
                    hash: body_hash,
                    bytes: 5,
                },
                LoopbackHeaderChecks::default(),
            )?,
            LoopbackHttpCheck::new(
                LoopbackProbe::new(LoopbackMethod::Get, "/health".to_owned())?,
                200,
                LoopbackExpectedBody::Exact {
                    hash: body_hash,
                    bytes: 5,
                },
                LoopbackHeaderChecks::default(),
            )?,
            LoopbackHttpCheck::new(
                LoopbackProbe::new(LoopbackMethod::Post, "/".to_owned())?,
                405,
                LoopbackExpectedBody::AnyBounded,
                LoopbackHeaderChecks::default(),
            )?,
        ];
        let single = LoopbackHttpRecipe::new(
            path.clone(),
            LoopbackStart::Explicit,
            NonZeroU16::try_from(18_765)?,
            checks.clone(),
        )?;
        let aggregate = single.clone().with_startup_coverage(0)?;
        assert_ne!(aggregate.id(), single.id());
        assert_ne!(
            aggregate.id(),
            single.clone().with_startup_coverage(1)?.id()
        );
        assert_eq!(aggregate.default_start_check_index(), Some(0));
        assert!(aggregate.clone().with_startup_coverage(1).is_err());
        assert!(single.clone().with_startup_coverage(2).is_err());
        assert!(single.clone().with_startup_coverage(3).is_err());
        assert!(
            LoopbackHttpRecipe::new(
                path.clone(),
                LoopbackStart::Explicit,
                NonZeroU16::try_from(LOOPBACK_DEFAULT_PORT)?,
                checks,
            )?
            .with_startup_coverage(0)
            .is_err()
        );

        let context = LoopbackHttpEvidenceContext {
            verification_run_id: VerificationRunId::from_bytes([1; 32]),
            spec_id: VerificationSpecId::from_bytes([2; 32]),
            run_id: AgentRunId::from_bytes([3; 32]),
            tool_run_id: ToolRunId::from_bytes([4; 32]),
            step_id: TaskStepId::from_bytes([5; 32]),
            snapshot_id: SnapshotId::from_bytes([6; 32]),
        };
        let output_limit = ProcessOutputLimit::new(1024)?;
        let process = LoopbackHttpProcess::from_spec(&single.process_spec(
            context.run_id,
            WorktreeId::from_bytes([7; 32]),
            context.step_id,
            LoopbackProcessSettings {
                timeout: ProcessTimeout::from_millis(4000)?,
                stdout_limit: output_limit,
                stderr_limit: output_limit,
                system_root: false,
            },
        )?)?;
        let receipt = |recipe: &LoopbackHttpRecipe| {
            let observations = [(11, 200), (12, 200), (13, 405)]
                .into_iter()
                .map(|(id, status)| {
                    LoopbackHttpObservation::new(
                        PolicyDecisionId::from_bytes([id; 32]),
                        status,
                        body_hash,
                        5,
                        LoopbackHeaderChecks::default(),
                    )
                })
                .collect::<Result<_, _>>()?;
            LoopbackHttpEvidence::new(
                context,
                recipe.id(),
                FileRevision::new(path.clone(), body_hash),
                process,
                PolicyDecisionId::from_bytes([10; 32]),
                observations,
            )
        };
        assert!(receipt(&single)?.matches(&single));
        assert!(!receipt(&aggregate)?.matches(&aggregate));
        Ok(())
    }
}
