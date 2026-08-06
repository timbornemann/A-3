use super::{RepositoryPath, WorktreeId};
use std::error::Error;
use std::fmt;

const MANIFEST_ROLE: u8 = 1 << 0;
const BUILD_ROLE: u8 = 1 << 1;
const TEST_ROLE: u8 = 1 << 2;
const CONTINUOUS_INTEGRATION_ROLE: u8 = 1 << 3;
const ALL_ROLES: u8 = MANIFEST_ROLE | BUILD_ROLE | TEST_ROLE | CONTINUOUS_INTEGRATION_ROLE;
const VENDOR_DIRECTORIES: &[&[u8]] = &[
    b"node_modules",
    b"vendor",
    b"vendors",
    b"third_party",
    b"third-party",
    b"bower_components",
    b"site-packages",
    b".venv",
    b"venv",
];
const GENERATED_DIRECTORIES: &[&[u8]] = &[
    b"target",
    b"dist",
    b"build",
    b"out",
    b".next",
    b".nuxt",
    b".svelte-kit",
    b"coverage",
    b"__pycache__",
    b".pytest_cache",
    b".mypy_cache",
    b".ruff_cache",
    b".cache",
    b"generated",
];
const BINARY_EXTENSIONS: &[&[u8]] = &[
    b"7z", b"a", b"avi", b"bin", b"bmp", b"class", b"db", b"dll", b"dylib", b"eot", b"exe",
    b"flac", b"gif", b"gz", b"ico", b"jar", b"jpeg", b"jpg", b"lib", b"lockb", b"mov", b"mp3",
    b"mp4", b"o", b"obj", b"ogg", b"otf", b"parquet", b"pdb", b"pdf", b"png", b"pyc", b"rlib",
    b"rmeta", b"so", b"sqlite", b"sqlite3", b"tar", b"tiff", b"ttf", b"wasm", b"wav", b"webm",
    b"webp", b"woff", b"woff2", b"xz", b"zip",
];
const SECRET_EXTENSIONS: &[&[u8]] = &[b"jks", b"kdbx", b"key", b"keystore", b"p12", b"pem", b"pfx"];

/// Version of the deterministic rules that produced a discovery result.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct DiscoveryPolicyVersion(u32);

impl DiscoveryPolicyVersion {
    /// Creates a non-zero discovery policy version.
    pub fn new(value: u32) -> Result<Self, DiscoveryPolicyVersionError> {
        if value == 0 {
            return Err(DiscoveryPolicyVersionError);
        }
        Ok(Self(value))
    }

    /// Returns the stable primitive representation.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// Discovery policy version zero is invalid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiscoveryPolicyVersionError;

impl fmt::Display for DiscoveryPolicyVersionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("discovery policy version must be positive")
    }
}

impl Error for DiscoveryPolicyVersionError {}

/// Bounded V1 policy shared by every local repository discovery adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiscoveryPolicy {
    version: DiscoveryPolicyVersion,
    max_candidates: usize,
    max_total_path_bytes: usize,
    max_file_bytes: u64,
    inspection_prefix_bytes: usize,
    max_config_bytes: usize,
    max_ignore_patterns: usize,
    max_ignore_pattern_bytes: usize,
}

impl DiscoveryPolicy {
    /// Returns the immutable V1 resource and compatibility limits.
    #[must_use]
    pub const fn v1() -> Self {
        Self {
            version: DiscoveryPolicyVersion(1),
            max_candidates: 250_000,
            max_total_path_bytes: 64 * 1024 * 1024,
            max_file_bytes: 4 * 1024 * 1024,
            inspection_prefix_bytes: 16 * 1024,
            max_config_bytes: 64 * 1024,
            max_ignore_patterns: 256,
            max_ignore_pattern_bytes: 1024,
        }
    }

    /// Returns the policy revision captured by the result.
    #[must_use]
    pub const fn version(self) -> DiscoveryPolicyVersion {
        self.version
    }

    /// Returns the maximum unique tracked and untracked candidates.
    #[must_use]
    pub const fn max_candidates(self) -> usize {
        self.max_candidates
    }

    /// Returns the aggregate memory boundary for unique repository path bytes.
    #[must_use]
    pub const fn max_total_path_bytes(self) -> usize {
        self.max_total_path_bytes
    }

    /// Returns the largest file eligible for prefix inspection and indexing.
    #[must_use]
    pub const fn max_file_bytes(self) -> u64 {
        self.max_file_bytes
    }

    /// Returns the maximum prefix read from one eligible file.
    #[must_use]
    pub const fn inspection_prefix_bytes(self) -> usize {
        self.inspection_prefix_bytes
    }

    /// Returns the maximum accepted byte length of `.a3/project.toml`.
    #[must_use]
    pub const fn max_config_bytes(self) -> usize {
        self.max_config_bytes
    }

    /// Returns the maximum number of project exclusion patterns.
    #[must_use]
    pub const fn max_ignore_patterns(self) -> usize {
        self.max_ignore_patterns
    }

    /// Returns the maximum UTF-8 byte length of one project exclusion pattern.
    #[must_use]
    pub const fn max_ignore_pattern_bytes(self) -> usize {
        self.max_ignore_pattern_bytes
    }

    /// Applies the non-overridable V1 path exclusions before any repository-owned ignore rule.
    #[must_use]
    pub fn classify_built_in_path(
        self,
        path: &[u8],
        is_directory: bool,
    ) -> Option<DiscoveryExclusionReason> {
        let components = path.split(|byte| *byte == b'/').collect::<Vec<_>>();
        if components.iter().any(|component| {
            VENDOR_DIRECTORIES
                .iter()
                .any(|known| component.eq_ignore_ascii_case(known))
        }) {
            return Some(DiscoveryExclusionReason::Vendor);
        }
        if components.iter().any(|component| {
            GENERATED_DIRECTORIES
                .iter()
                .any(|known| component.eq_ignore_ascii_case(known))
        }) || (!is_directory && is_generated_file(path))
        {
            return Some(DiscoveryExclusionReason::Generated);
        }
        if is_secret_path(&components) {
            return Some(DiscoveryExclusionReason::Secret);
        }
        if !is_directory
            && extension(path).is_some_and(|extension| {
                BINARY_EXTENSIONS
                    .iter()
                    .any(|known| extension.eq_ignore_ascii_case(known))
            })
        {
            return Some(DiscoveryExclusionReason::Binary);
        }
        None
    }

    /// Classifies at most the bounded prefix supplied by a filesystem adapter.
    #[must_use]
    pub fn classify_content_prefix(self, prefix: &[u8]) -> Option<DiscoveryExclusionReason> {
        if contains_private_key_banner(prefix) || contains_credential_token(prefix) {
            return Some(DiscoveryExclusionReason::Secret);
        }
        looks_binary(prefix).then_some(DiscoveryExclusionReason::Binary)
    }
}

impl Default for DiscoveryPolicy {
    fn default() -> Self {
        Self::v1()
    }
}

fn is_secret_basename(basename: &[u8]) -> bool {
    let basename_is = |value: &[u8]| basename.eq_ignore_ascii_case(value);
    basename_is(b".env")
        || starts_with_ignore_ascii_case(basename, b".env.")
        || [
            b".npmrc".as_slice(),
            b".pypirc",
            b".netrc",
            b"_netrc",
            b"auth.json",
            b"credentials",
            b"credentials.json",
            b"secrets.json",
            b"service-account.json",
            b"service_account.json",
            b"id_rsa",
            b"id_ed25519",
            b"id_ecdsa",
        ]
        .iter()
        .any(|known| basename_is(known))
        || extension(basename).is_some_and(|extension| {
            SECRET_EXTENSIONS
                .iter()
                .any(|known| extension.eq_ignore_ascii_case(known))
        })
        || ((starts_with_ignore_ascii_case(basename, b"service-account-")
            || starts_with_ignore_ascii_case(basename, b"service_account_"))
            && ends_with_ignore_ascii_case(basename, b".json"))
}

fn is_secret_path(components: &[&[u8]]) -> bool {
    let Some(basename) = components.last() else {
        return false;
    };
    if is_secret_basename(basename) {
        return true;
    }
    components.windows(2).any(|pair| {
        (pair[0].eq_ignore_ascii_case(b".aws") && pair[1].eq_ignore_ascii_case(b"credentials"))
            || (pair[0].eq_ignore_ascii_case(b".docker")
                && pair[1].eq_ignore_ascii_case(b"config.json"))
            || (pair[0].eq_ignore_ascii_case(b".kube") && pair[1].eq_ignore_ascii_case(b"config"))
            || pair[0].eq_ignore_ascii_case(b".ssh")
    })
}

fn is_generated_file(path: &[u8]) -> bool {
    ends_with_ignore_ascii_case(path, b".min.js")
        || ends_with_ignore_ascii_case(path, b".min.css")
        || ends_with_ignore_ascii_case(path, b".map")
        || path
            .windows(b".generated.".len())
            .any(|window| window.eq_ignore_ascii_case(b".generated."))
        || ends_with_ignore_ascii_case(path, b".g.dart")
        || ends_with_ignore_ascii_case(path, b".designer.cs")
}

fn contains_private_key_banner(bytes: &[u8]) -> bool {
    [
        b"-----BEGIN PRIVATE KEY-----".as_slice(),
        b"-----BEGIN RSA PRIVATE KEY-----",
        b"-----BEGIN EC PRIVATE KEY-----",
        b"-----BEGIN OPENSSH PRIVATE KEY-----",
    ]
    .iter()
    .any(|needle| contains(bytes, needle))
}

fn contains_credential_token(bytes: &[u8]) -> bool {
    contains_token_with_tail(bytes, b"ghp_", 36, |byte| byte.is_ascii_alphanumeric())
        || contains_token_with_tail(bytes, b"github_pat_", 22, |byte| {
            byte.is_ascii_alphanumeric() || byte == b'_'
        })
        || contains_aws_access_key(bytes)
}

fn looks_binary(bytes: &[u8]) -> bool {
    if bytes.contains(&0) {
        return true;
    }
    let control_bytes = bytes
        .iter()
        .filter(|byte| {
            byte.is_ascii_control() && !matches!(**byte, b'\n' | b'\r' | b'\t' | 0x08 | 0x0c | 0x1b)
        })
        .count();
    !bytes.is_empty() && control_bytes.saturating_mul(100) > bytes.len().saturating_mul(30)
}

fn contains_aws_access_key(bytes: &[u8]) -> bool {
    bytes.windows(20).any(|window| {
        window.starts_with(b"AKIA")
            && window[4..]
                .iter()
                .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit())
    })
}

fn contains_token_with_tail(
    bytes: &[u8],
    prefix: &[u8],
    tail_length: usize,
    valid: impl Fn(u8) -> bool,
) -> bool {
    let token_length = prefix.len().saturating_add(tail_length);
    bytes.windows(token_length).any(|window| {
        window.starts_with(prefix) && window[prefix.len()..].iter().copied().all(&valid)
    })
}

fn extension(path: &[u8]) -> Option<&[u8]> {
    let basename = path.rsplit(|byte| *byte == b'/').next()?;
    let position = basename.iter().rposition(|byte| *byte == b'.')?;
    basename.get(position.saturating_add(1)..)
}

fn starts_with_ignore_ascii_case(value: &[u8], prefix: &[u8]) -> bool {
    value
        .get(..prefix.len())
        .is_some_and(|start| start.eq_ignore_ascii_case(prefix))
}

fn ends_with_ignore_ascii_case(value: &[u8], suffix: &[u8]) -> bool {
    value
        .get(value.len().saturating_sub(suffix.len())..)
        .is_some_and(|end| end.eq_ignore_ascii_case(suffix))
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

/// Whether Git already tracks a discovered file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DiscoveryOrigin {
    /// The path is present in the repository index.
    Tracked,
    /// The path is present in the worktree but not in the repository index.
    Untracked,
}

/// Semantic file roles detected without parsing file contents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DiscoveredFileRole {
    /// Dependency, workspace, or package manifest.
    Manifest,
    /// Build-system entry point or build configuration.
    Build,
    /// Test source, test fixture, or test configuration.
    Test,
    /// Continuous-integration workflow or pipeline configuration.
    ContinuousIntegration,
}

/// Compact validated set of overlapping discovery roles.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct DiscoveredFileRoles(u8);

impl DiscoveredFileRoles {
    /// Creates an empty role set for an ordinary relevant text file.
    #[must_use]
    pub const fn empty() -> Self {
        Self(0)
    }

    /// Adds a semantic role.
    #[must_use]
    pub const fn with(mut self, role: DiscoveredFileRole) -> Self {
        self.0 |= match role {
            DiscoveredFileRole::Manifest => MANIFEST_ROLE,
            DiscoveredFileRole::Build => BUILD_ROLE,
            DiscoveredFileRole::Test => TEST_ROLE,
            DiscoveredFileRole::ContinuousIntegration => CONTINUOUS_INTEGRATION_ROLE,
        };
        self
    }

    /// Returns whether the role is present.
    #[must_use]
    pub const fn contains(self, role: DiscoveredFileRole) -> bool {
        let mask = match role {
            DiscoveredFileRole::Manifest => MANIFEST_ROLE,
            DiscoveredFileRole::Build => BUILD_ROLE,
            DiscoveredFileRole::Test => TEST_ROLE,
            DiscoveredFileRole::ContinuousIntegration => CONTINUOUS_INTEGRATION_ROLE,
        };
        self.0 & mask != 0
    }

    /// Reconstructs roles from a storage or protocol boundary.
    pub fn try_from_bits(bits: u8) -> Result<Self, DiscoveredFileRolesError> {
        if bits & !ALL_ROLES != 0 {
            return Err(DiscoveredFileRolesError);
        }
        Ok(Self(bits))
    }

    /// Returns the stable bit representation.
    #[must_use]
    pub const fn bits(self) -> u8 {
        self.0
    }
}

/// A stored role bitset contained an unknown role.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiscoveredFileRolesError;

impl fmt::Display for DiscoveredFileRolesError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("discovered file roles contain unknown bits")
    }
}

impl Error for DiscoveredFileRolesError {}

/// Reason a candidate was rejected before hashing or parsing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DiscoveryExclusionReason {
    /// A repository-owned A^3 exclusion pattern matched.
    ProjectIgnore,
    /// A non-overridable built-in vendor path matched.
    Vendor,
    /// A non-overridable built-in generated path matched.
    Generated,
    /// A known secret path or high-confidence credential signature matched.
    Secret,
    /// File metadata exceeds the fixed indexing size limit.
    TooLarge,
    /// Prefix inspection classified the file as binary.
    Binary,
    /// The path is a symbolic link and is never followed by discovery.
    SymbolicLink,
    /// The path is not a regular file.
    SpecialFile,
}

/// Deterministic counts of security and relevance exclusions.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DiscoveryExclusionCounts {
    project_ignore: u64,
    vendor: u64,
    generated: u64,
    secret: u64,
    too_large: u64,
    binary: u64,
    symbolic_link: u64,
    special_file: u64,
}

impl DiscoveryExclusionCounts {
    /// Records one exclusion using saturating arithmetic at the telemetry boundary.
    pub fn record(&mut self, reason: DiscoveryExclusionReason) {
        let counter = match reason {
            DiscoveryExclusionReason::ProjectIgnore => &mut self.project_ignore,
            DiscoveryExclusionReason::Vendor => &mut self.vendor,
            DiscoveryExclusionReason::Generated => &mut self.generated,
            DiscoveryExclusionReason::Secret => &mut self.secret,
            DiscoveryExclusionReason::TooLarge => &mut self.too_large,
            DiscoveryExclusionReason::Binary => &mut self.binary,
            DiscoveryExclusionReason::SymbolicLink => &mut self.symbolic_link,
            DiscoveryExclusionReason::SpecialFile => &mut self.special_file,
        };
        *counter = counter.saturating_add(1);
    }

    /// Returns the count for one exclusion reason.
    #[must_use]
    pub const fn get(self, reason: DiscoveryExclusionReason) -> u64 {
        match reason {
            DiscoveryExclusionReason::ProjectIgnore => self.project_ignore,
            DiscoveryExclusionReason::Vendor => self.vendor,
            DiscoveryExclusionReason::Generated => self.generated,
            DiscoveryExclusionReason::Secret => self.secret,
            DiscoveryExclusionReason::TooLarge => self.too_large,
            DiscoveryExclusionReason::Binary => self.binary,
            DiscoveryExclusionReason::SymbolicLink => self.symbolic_link,
            DiscoveryExclusionReason::SpecialFile => self.special_file,
        }
    }

    /// Returns the total number of classified exclusions.
    #[must_use]
    pub const fn total(self) -> u64 {
        self.project_ignore
            .saturating_add(self.vendor)
            .saturating_add(self.generated)
            .saturating_add(self.secret)
            .saturating_add(self.too_large)
            .saturating_add(self.binary)
            .saturating_add(self.symbolic_link)
            .saturating_add(self.special_file)
    }
}

/// One bounded relevant file discovered in a worktree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredFile {
    path: RepositoryPath,
    origin: DiscoveryOrigin,
    size_bytes: u64,
    roles: DiscoveredFileRoles,
}

impl DiscoveredFile {
    /// Creates one candidate after the adapter has applied safety classification.
    #[must_use]
    pub const fn new(
        path: RepositoryPath,
        origin: DiscoveryOrigin,
        size_bytes: u64,
        roles: DiscoveredFileRoles,
    ) -> Self {
        Self {
            path,
            origin,
            size_bytes,
            roles,
        }
    }

    /// Returns the lossless Git-style repository-relative path.
    #[must_use]
    pub const fn path(&self) -> &RepositoryPath {
        &self.path
    }

    /// Returns whether Git tracks the path.
    #[must_use]
    pub const fn origin(&self) -> DiscoveryOrigin {
        self.origin
    }

    /// Returns the metadata size observed before bounded prefix inspection.
    #[must_use]
    pub const fn size_bytes(&self) -> u64 {
        self.size_bytes
    }

    /// Returns overlapping semantic roles detected from the path.
    #[must_use]
    pub const fn roles(&self) -> DiscoveredFileRoles {
        self.roles
    }
}

/// Complete deterministic output of one successful discovery run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveryResult {
    worktree_id: WorktreeId,
    policy_version: DiscoveryPolicyVersion,
    files: Vec<DiscoveredFile>,
    exclusions: DiscoveryExclusionCounts,
}

impl DiscoveryResult {
    /// Sorts by lossless path bytes and rejects duplicate repository paths.
    pub fn new(
        worktree_id: WorktreeId,
        policy_version: DiscoveryPolicyVersion,
        mut files: Vec<DiscoveredFile>,
        exclusions: DiscoveryExclusionCounts,
    ) -> Result<Self, DiscoveryResultError> {
        files.sort_by(|left, right| left.path.cmp(&right.path));
        if files.windows(2).any(|pair| pair[0].path == pair[1].path) {
            return Err(DiscoveryResultError::DuplicatePath);
        }
        Ok(Self {
            worktree_id,
            policy_version,
            files,
            exclusions,
        })
    }

    /// Returns the worktree whose privileged adapter produced this result.
    #[must_use]
    pub const fn worktree_id(&self) -> WorktreeId {
        self.worktree_id
    }

    /// Returns the exact rules revision used by discovery.
    #[must_use]
    pub const fn policy_version(&self) -> DiscoveryPolicyVersion {
        self.policy_version
    }

    /// Returns included files in canonical repository-path byte order.
    #[must_use]
    pub fn files(&self) -> &[DiscoveredFile] {
        &self.files
    }

    /// Returns aggregate rejection counts without exposing content or credentials.
    #[must_use]
    pub const fn exclusions(&self) -> DiscoveryExclusionCounts {
        self.exclusions
    }
}

/// Invalid result assembled at the local adapter boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiscoveryResultError {
    /// The same normalized path was included more than once.
    DuplicatePath,
}

impl fmt::Display for DiscoveryResultError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicatePath => formatter.write_str("discovery result contains duplicate paths"),
        }
    }
}

impl Error for DiscoveryResultError {}

#[cfg(test)]
mod tests {
    use super::{
        DiscoveredFile, DiscoveredFileRole, DiscoveredFileRoles, DiscoveryExclusionCounts,
        DiscoveryOrigin, DiscoveryPolicy, DiscoveryResult, DiscoveryResultError,
    };
    use crate::{RepositoryPath, WorktreeId};

    #[test]
    fn discovery_result_sorts_lossless_paths_and_rejects_duplicates()
    -> Result<(), Box<dyn std::error::Error>> {
        let first = RepositoryPath::try_from_bytes(b"a/non-utf8-\xff".to_vec())?;
        let second = RepositoryPath::try_from_bytes(b"z.rs".to_vec())?;
        let file =
            |path, origin| DiscoveredFile::new(path, origin, 7, DiscoveredFileRoles::empty());
        let result = DiscoveryResult::new(
            WorktreeId::from_bytes([9; 32]),
            DiscoveryPolicy::v1().version(),
            vec![
                file(second, DiscoveryOrigin::Tracked),
                file(first.clone(), DiscoveryOrigin::Untracked),
            ],
            DiscoveryExclusionCounts::default(),
        )?;

        assert_eq!(result.files()[0].path(), &first);
        assert_eq!(
            DiscoveryResult::new(
                WorktreeId::from_bytes([9; 32]),
                DiscoveryPolicy::v1().version(),
                vec![
                    file(first.clone(), DiscoveryOrigin::Tracked),
                    file(first, DiscoveryOrigin::Untracked),
                ],
                DiscoveryExclusionCounts::default(),
            ),
            Err(DiscoveryResultError::DuplicatePath)
        );
        Ok(())
    }

    #[test]
    fn roles_can_overlap_without_unknown_bits() {
        let roles = DiscoveredFileRoles::empty()
            .with(DiscoveredFileRole::Manifest)
            .with(DiscoveredFileRole::Build);

        assert!(roles.contains(DiscoveredFileRole::Manifest));
        assert!(roles.contains(DiscoveredFileRole::Build));
        assert!(!roles.contains(DiscoveredFileRole::Test));
        assert_eq!(DiscoveredFileRoles::try_from_bits(roles.bits()), Ok(roles));
        assert!(DiscoveredFileRoles::try_from_bits(0x80).is_err());
    }
}
