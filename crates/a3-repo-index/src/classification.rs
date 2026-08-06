//! Pure path and bounded-prefix classification for discovery.

use crate::config::ProjectIgnore;
use a3_domain::{
    DiscoveredFileRole, DiscoveredFileRoles, DiscoveryExclusionReason, DiscoveryPolicy,
};
use gix::ignore::glob::pattern::Case;

pub(crate) fn classify_path(
    path: &[u8],
    is_dir: bool,
    project_ignore: &ProjectIgnore,
    case: Case,
    policy: DiscoveryPolicy,
) -> Option<DiscoveryExclusionReason> {
    if let Some(reason) = policy.classify_built_in_path(path, is_dir) {
        return Some(reason);
    }
    project_ignore
        .matches(path, is_dir, case)
        .then_some(DiscoveryExclusionReason::ProjectIgnore)
}

pub(crate) fn classify_prefix(
    prefix: &[u8],
    policy: DiscoveryPolicy,
) -> Option<DiscoveryExclusionReason> {
    policy.classify_content_prefix(prefix)
}

pub(crate) fn roles_for_path(path: &[u8]) -> DiscoveredFileRoles {
    let lower = path.iter().map(u8::to_ascii_lowercase).collect::<Vec<_>>();
    let basename = lower.rsplit(|byte| *byte == b'/').next().unwrap_or(&lower);
    let mut roles = DiscoveredFileRoles::empty();

    if is_manifest(&lower, basename) {
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

fn is_manifest(path: &[u8], basename: &[u8]) -> bool {
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
        || (basename.starts_with(b"requirements")
            && (basename.ends_with(b".txt") || basename.ends_with(b".in")))
        || (path
            .split(|byte| *byte == b'/')
            .any(|component| component == b"requirements")
            && (basename.ends_with(b".txt") || basename.ends_with(b".in")))
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
            .is_some_and(|stem| stem.starts_with(b"test_") || stem.ends_with(b"_test"))
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

fn starts_with_any(value: &[u8], prefixes: &[&[u8]]) -> bool {
    prefixes.iter().any(|prefix| value.starts_with(prefix))
}

#[cfg(test)]
mod tests {
    use super::{classify_prefix, roles_for_path};
    use a3_domain::{DiscoveredFileRole, DiscoveryExclusionReason, DiscoveryPolicy};

    #[test]
    fn roles_are_overlapping_and_path_based() {
        let roles = roles_for_path(b"tests/.github/workflows/package.json");
        assert!(roles.contains(DiscoveredFileRole::Manifest));
        assert!(roles.contains(DiscoveredFileRole::Test));
        assert!(roles_for_path(b"build.rs").contains(DiscoveredFileRole::Build));
        assert!(roles_for_path(b"test_feature.py").contains(DiscoveredFileRole::Test));
        assert!(roles_for_path(b"requirements/base.in").contains(DiscoveredFileRole::Manifest));
        assert!(
            roles_for_path(b".github/workflows/ci.yml")
                .contains(DiscoveredFileRole::ContinuousIntegration)
        );
    }

    #[test]
    fn high_confidence_private_key_prefix_is_secret() {
        assert_eq!(
            classify_prefix(
                b"x\n-----BEGIN OPENSSH PRIVATE KEY-----\ny",
                DiscoveryPolicy::v1()
            ),
            Some(DiscoveryExclusionReason::Secret)
        );
        assert_eq!(
            classify_prefix(b"text\0data", DiscoveryPolicy::v1()),
            Some(DiscoveryExclusionReason::Binary)
        );
        assert_eq!(
            classify_prefix(&[1; 128], DiscoveryPolicy::v1()),
            Some(DiscoveryExclusionReason::Binary)
        );
    }
}
