use a3_domain::{GitHead, GitObjectId, GitReferenceName, ProjectIdentity};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RepositoryValidationError {
    RootUnavailable,
    InvalidRepository,
}

pub(crate) fn open_validated(
    project: &ProjectIdentity,
) -> Result<gix::Repository, RepositoryValidationError> {
    let expected_root = project.worktree().root().as_path();
    let observed_root = std::fs::canonicalize(expected_root)
        .map_err(|_| RepositoryValidationError::RootUnavailable)?;
    if observed_root != expected_root {
        return Err(RepositoryValidationError::RootUnavailable);
    }

    let repository = gix::open_opts(expected_root.to_path_buf(), gix::open::Options::isolated())
        .map_err(|_| RepositoryValidationError::InvalidRepository)?;
    let worktree = repository
        .workdir()
        .ok_or(RepositoryValidationError::InvalidRepository)?;
    let repository_root =
        std::fs::canonicalize(worktree).map_err(|_| RepositoryValidationError::RootUnavailable)?;
    let common_directory = std::fs::canonicalize(repository.common_dir())
        .map_err(|_| RepositoryValidationError::InvalidRepository)?;
    if repository_root != expected_root
        || common_directory != project.repository().common_directory().as_path()
    {
        return Err(RepositoryValidationError::InvalidRepository);
    }
    Ok(repository)
}

pub(crate) fn inspect_head(
    repository: &gix::Repository,
) -> Result<GitHead, RepositoryValidationError> {
    let head = repository
        .head()
        .map_err(|_| RepositoryValidationError::InvalidRepository)?;
    let reference = head
        .referent_name()
        .map(|name| GitReferenceName::try_from_full_name(name.to_string()))
        .transpose()
        .map_err(|_| RepositoryValidationError::InvalidRepository)?;
    if head.is_unborn() {
        return reference
            .map(|reference| GitHead::Unborn { reference })
            .ok_or(RepositoryValidationError::InvalidRepository);
    }

    let object_id = head
        .id()
        .map(|id| GitObjectId::try_from_hex(id.detach().to_string()))
        .transpose()
        .map_err(|_| RepositoryValidationError::InvalidRepository)?
        .ok_or(RepositoryValidationError::InvalidRepository)?;
    Ok(GitHead::Born {
        object_id,
        reference,
    })
}

pub(crate) fn inspect_index_checksum(
    repository: &gix::Repository,
) -> Result<Option<String>, RepositoryValidationError> {
    let index = repository
        .index_or_empty()
        .map_err(|_| RepositoryValidationError::InvalidRepository)?;
    Ok(index.checksum().map(|checksum| checksum.to_string()))
}
