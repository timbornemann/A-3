use crate::platform_path;
use a3_domain::{CanonicalDirectory, RemoteIdentity, RepositoryId, WorktreeId};
use blake3::Hasher;
use gix::url::Scheme;

const REPOSITORY_ID_VERSION: &[u8] = b"a3.repository-id.v1";
const WORKTREE_ID_VERSION: &[u8] = b"a3.worktree-id.v1";
const REMOTE_ID_VERSION: &[u8] = b"a3.remote-id.v1";

pub(crate) fn repository_id(common_directory: &CanonicalDirectory) -> RepositoryId {
    let mut hasher = Hasher::new();
    update_field(&mut hasher, REPOSITORY_ID_VERSION);
    update_field(
        &mut hasher,
        &platform_path::bytes(common_directory.as_path()),
    );
    RepositoryId::from_bytes(*hasher.finalize().as_bytes())
}

pub(crate) fn worktree_id(repository_id: RepositoryId, root: &CanonicalDirectory) -> WorktreeId {
    let mut hasher = Hasher::new();
    update_field(&mut hasher, WORKTREE_ID_VERSION);
    update_field(&mut hasher, repository_id.as_bytes());
    update_field(&mut hasher, &platform_path::bytes(root.as_path()));
    WorktreeId::from_bytes(*hasher.finalize().as_bytes())
}

pub(crate) fn remote_identity(url: &gix::Url) -> RemoteIdentity {
    let mut hasher = Hasher::new();
    update_field(&mut hasher, REMOTE_ID_VERSION);
    update_field(&mut hasher, url.scheme.as_str().as_bytes());
    update_field(
        &mut hasher,
        url.host.as_deref().map(str::as_bytes).unwrap_or_default(),
    );
    update_field(
        &mut hasher,
        &url.port
            .or_else(|| url.scheme.default_port())
            .unwrap_or_default()
            .to_le_bytes(),
    );
    update_field(&mut hasher, &normalized_remote_path(url));
    RemoteIdentity::from_bytes(*hasher.finalize().as_bytes())
}

fn normalized_remote_path(url: &gix::Url) -> Vec<u8> {
    let mut path: Vec<u8> = url.path.iter().copied().collect();
    if matches!(url.scheme, Scheme::Http | Scheme::Https)
        && let Some(position) = path.iter().position(|byte| matches!(byte, b'?' | b'#'))
    {
        path.truncate(position);
    }
    while path.ends_with(b"/") {
        path.pop();
    }
    if path.ends_with(b".git") {
        path.truncate(path.len() - 4);
    }
    path
}

fn update_field(hasher: &mut Hasher, value: &[u8]) {
    hasher.update(&(value.len() as u128).to_le_bytes());
    hasher.update(value);
}

#[cfg(test)]
mod tests {
    use super::remote_identity;

    #[test]
    fn remote_identity_ignores_credentials_suffixes_and_default_port()
    -> Result<(), Box<dyn std::error::Error>> {
        let with_secret =
            gix::Url::try_from("https://user:secret@example.com:443/Owner/Repo.git/?token=secret")?;
        let normalized = gix::Url::try_from("https://example.com/Owner/Repo")?;

        assert_eq!(remote_identity(&with_secret), remote_identity(&normalized));
        Ok(())
    }
}
