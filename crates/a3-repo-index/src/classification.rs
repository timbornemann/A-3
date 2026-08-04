//! Pure path and bounded-prefix classification for discovery.

use crate::config::ProjectIgnore;
use a3_domain::{DiscoveredFileRole, DiscoveredFileRoles, DiscoveryExclusionReason};
use gix::ignore::glob::pattern::Case;

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

pub(crate) fn classify_path(
    path: &[u8],
    is_dir: bool,
    project_ignore: &ProjectIgnore,
    case: Case,
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
    }) || (!is_dir && is_generated_file(path))
    {
        return Some(DiscoveryExclusionReason::Generated);
    }
    if is_secret_path(&components) {
        return Some(DiscoveryExclusionReason::Secret);
    }
    if !is_dir
        && extension(path).is_some_and(|extension| {
            BINARY_EXTENSIONS
                .iter()
                .any(|known| extension.eq_ignore_ascii_case(known))
        })
    {
        return Some(DiscoveryExclusionReason::Binary);
    }
    project_ignore
        .matches(path, is_dir, case)
        .then_some(DiscoveryExclusionReason::ProjectIgnore)
}

pub(crate) fn classify_prefix(prefix: &[u8]) -> Option<DiscoveryExclusionReason> {
    if contains_private_key_banner(prefix) || contains_credential_token(prefix) {
        return Some(DiscoveryExclusionReason::Secret);
    }
    looks_binary(prefix).then_some(DiscoveryExclusionReason::Binary)
}

pub(crate) fn roles_for_path(path: &[u8]) -> DiscoveredFileRoles {
    let lower = path.iter().map(u8::to_ascii_lowercase).collect::<Vec<_>>();
    let basename = lower.rsplit(|byte| *byte == b'/').next().unwrap_or(&lower);
    let mut roles = DiscoveredFileRoles::empty();

    if is_manifest(basename) {
        roles = roles.with(DiscoveredFileRole::Manifest);
    }
    if is_build_file(&lower, basename) {
        roles = roles.with(DiscoveredFileRole::Build);
    }
    if is_test_file(&lower, basename) {
        roles = roles.with(DiscoveredFileRole::Test);
    }
    if is_ci_file(&lower, basename) {
        roles = roles.with(DiscoveredFileRole::ContinuousIntegration);
    }
    roles
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

fn is_manifest(basename: &[u8]) -> bool {
    [
        b"cargo.toml".as_slice(),
        b"cargo.lock",
        b"package.json",
        b"package-lock.json",
        b"pnpm-lock.yaml",
        b"pnpm-workspace.yaml",
        b"yarn.lock",
        b"deno.json",
        b"deno.jsonc",
        b"pyproject.toml",
        b"poetry.lock",
        b"pipfile",
        b"pipfile.lock",
        b"setup.py",
        b"setup.cfg",
        b"go.mod",
        b"go.sum",
        b"pom.xml",
        b"build.gradle",
        b"build.gradle.kts",
        b"gemfile",
        b"composer.json",
    ]
    .contains(&basename)
        || basename.starts_with(b"dockerfile.")
        || (basename.starts_with(b"requirements") && basename.ends_with(b".txt"))
}

fn is_build_file(path: &[u8], basename: &[u8]) -> bool {
    [
        b"build.rs".as_slice(),
        b"makefile",
        b"gnumakefile",
        b"cmakelists.txt",
        b"dockerfile",
        b"justfile",
    ]
    .contains(&basename)
        || starts_with_any(
            basename,
            &[
                b"tsconfig".as_slice(),
                b"vite.config",
                b"webpack.config",
                b"rollup.config",
                b"esbuild.config",
            ],
        )
        || path.starts_with(b".cargo/")
}

fn is_test_file(path: &[u8], basename: &[u8]) -> bool {
    path.split(|byte| *byte == b'/').any(|component| {
        matches!(
            component,
            b"test" | b"tests" | b"spec" | b"specs" | b"__tests__"
        )
    }) || basename.windows(6).any(|window| window == b".test.")
        || basename.windows(6).any(|window| window == b".spec.")
        || starts_with_any(
            basename,
            &[
                b"jest.config".as_slice(),
                b"vitest.config",
                b"playwright.config",
            ],
        )
        || [b"pytest.ini".as_slice(), b"tox.ini", b"conftest.py"].contains(&basename)
        || basename
            .split(|byte| *byte == b'.')
            .next()
            .is_some_and(|stem| stem.ends_with(b"_test"))
}

fn is_ci_file(path: &[u8], basename: &[u8]) -> bool {
    path.starts_with(b".github/workflows/")
        || path.starts_with(b".circleci/")
        || path.starts_with(b".buildkite/")
        || [
            b".gitlab-ci.yml".as_slice(),
            b".gitlab-ci.yaml",
            b"azure-pipelines.yml",
            b"azure-pipelines.yaml",
            b"jenkinsfile",
        ]
        .contains(&basename)
}

fn extension(path: &[u8]) -> Option<&[u8]> {
    let basename = path.rsplit(|byte| *byte == b'/').next()?;
    let position = basename.iter().rposition(|byte| *byte == b'.')?;
    basename.get(position.saturating_add(1)..)
}

fn starts_with_any(value: &[u8], prefixes: &[&[u8]]) -> bool {
    prefixes.iter().any(|prefix| value.starts_with(prefix))
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

#[cfg(test)]
mod tests {
    use super::{classify_prefix, roles_for_path};
    use a3_domain::{DiscoveredFileRole, DiscoveryExclusionReason};

    #[test]
    fn roles_are_overlapping_and_path_based() {
        let roles = roles_for_path(b"tests/.github/workflows/package.json");
        assert!(roles.contains(DiscoveredFileRole::Manifest));
        assert!(roles.contains(DiscoveredFileRole::Test));
        assert!(roles_for_path(b"build.rs").contains(DiscoveredFileRole::Build));
        assert!(
            roles_for_path(b".github/workflows/ci.yml")
                .contains(DiscoveredFileRole::ContinuousIntegration)
        );
    }

    #[test]
    fn high_confidence_private_key_prefix_is_secret() {
        assert_eq!(
            classify_prefix(b"x\n-----BEGIN OPENSSH PRIVATE KEY-----\ny"),
            Some(DiscoveryExclusionReason::Secret)
        );
        assert_eq!(
            classify_prefix(b"text\0data"),
            Some(DiscoveryExclusionReason::Binary)
        );
        assert_eq!(
            classify_prefix(&[1; 128]),
            Some(DiscoveryExclusionReason::Binary)
        );
    }
}
