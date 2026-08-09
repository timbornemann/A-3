use crate::process_environment::ProcessHostEnvironment;
use crate::{PathEntryKind, PathPolicy, platform_path};
use a3_application::ProcessRunFailure;
use a3_domain::{ProcessSpec, ProjectIdentity, WorkspaceDirectory};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};

pub(crate) fn prepare_command(
    project: &ProjectIdentity,
    specification: &ProcessSpec,
    environment: &ProcessHostEnvironment,
) -> Result<Command, ProcessRunFailure> {
    if project.worktree().id() != specification.worktree_id() {
        return Err(ProcessRunFailure::Denied);
    }
    let path_policy = PathPolicy::from_selected_root(project.worktree().root().as_path())
        .map_err(|_| ProcessRunFailure::Denied)?;
    let working_directory = resolve_working_directory(&path_policy, specification)?;
    let executable = resolve_executable(specification.executable().as_str(), environment)?;

    let mut command = Command::new(executable);
    command
        .args(
            specification
                .arguments()
                .iter()
                .map(a3_domain::ProcessArgument::as_str),
        )
        .current_dir(working_directory)
        .env_clear()
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for variable in specification.environment_allowlist() {
        let value = environment
            .value(variable)
            .ok_or(ProcessRunFailure::Denied)?;
        command.env(variable.as_str(), value);
    }
    Ok(command)
}

fn resolve_working_directory(
    policy: &PathPolicy,
    specification: &ProcessSpec,
) -> Result<PathBuf, ProcessRunFailure> {
    let candidate = match specification.working_directory() {
        WorkspaceDirectory::Root => policy.root().as_path().to_path_buf(),
        WorkspaceDirectory::Subtree(path) => {
            let relative =
                platform_path::repository_path(path).map_err(|_| ProcessRunFailure::Denied)?;
            let resolved = policy
                .resolve_existing(relative)
                .map_err(|_| ProcessRunFailure::Denied)?;
            if resolved.kind() != PathEntryKind::Directory {
                return Err(ProcessRunFailure::Denied);
            }
            resolved.as_path().to_path_buf()
        }
    };
    if !candidate.starts_with(policy.root().as_path()) {
        return Err(ProcessRunFailure::Denied);
    }
    Ok(candidate)
}

fn resolve_executable(
    value: &str,
    environment: &ProcessHostEnvironment,
) -> Result<PathBuf, ProcessRunFailure> {
    let requested = Path::new(value);
    if requested.is_absolute() {
        return validate_executable(requested);
    }
    if requested.components().count() != 1
        || !matches!(requested.components().next(), Some(Component::Normal(_)))
    {
        return Err(ProcessRunFailure::Denied);
    }
    let search_path = environment
        .value_by_name("PATH")
        .ok_or(ProcessRunFailure::Denied)?;
    for directory in std::env::split_paths(search_path) {
        if !directory.is_absolute() {
            continue;
        }
        for candidate in executable_candidates(&directory, value)? {
            if let Ok(executable) = validate_executable(&candidate) {
                return Ok(executable);
            }
        }
    }
    Err(ProcessRunFailure::Denied)
}

#[cfg(windows)]
fn executable_candidates(directory: &Path, value: &str) -> Result<Vec<PathBuf>, ProcessRunFailure> {
    let requested = Path::new(value);
    match requested.extension().and_then(std::ffi::OsStr::to_str) {
        Some(extension) if extension.eq_ignore_ascii_case("exe") => {
            Ok(vec![directory.join(requested)])
        }
        None => Ok(vec![directory.join(format!("{value}.exe"))]),
        Some(_) => Err(ProcessRunFailure::Denied),
    }
}

#[cfg(not(windows))]
fn executable_candidates(directory: &Path, value: &str) -> Result<Vec<PathBuf>, ProcessRunFailure> {
    Ok(vec![directory.join(value)])
}

fn validate_executable(candidate: &Path) -> Result<PathBuf, ProcessRunFailure> {
    #[cfg(windows)]
    {
        if !candidate
            .extension()
            .and_then(std::ffi::OsStr::to_str)
            .is_some_and(|extension| extension.eq_ignore_ascii_case("exe"))
        {
            return Err(ProcessRunFailure::Denied);
        }
    }

    let canonical = fs::canonicalize(candidate).map_err(|_| ProcessRunFailure::Denied)?;
    let metadata = fs::metadata(&canonical).map_err(|_| ProcessRunFailure::Denied)?;
    if !metadata.is_file() {
        return Err(ProcessRunFailure::Denied);
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        if metadata.permissions().mode() & 0o111 == 0 {
            return Err(ProcessRunFailure::Denied);
        }
    }
    Ok(canonical)
}
