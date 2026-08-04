use std::fmt;

const ID_LENGTH: usize = 32;

macro_rules! stable_id {
    ($(#[$metadata:meta])* $name:ident) => {
        $(#[$metadata])*
        #[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name([u8; ID_LENGTH]);

        impl $name {
            /// Constructs an ID from a versioned 256-bit derivation.
            #[must_use]
            pub const fn from_bytes(bytes: [u8; ID_LENGTH]) -> Self {
                Self(bytes)
            }

            /// Returns the canonical binary representation used by derivation and persistence.
            #[must_use]
            pub const fn as_bytes(&self) -> &[u8; ID_LENGTH] {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                write_hex(&self.0, formatter)
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(concat!(stringify!($name), "("))?;
                write_hex(&self.0, formatter)?;
                formatter.write_str(")")
            }
        }
    };
}

stable_id!(
    /// Stable identity of one catalog project across confirmed location changes.
    ProjectId
);
stable_id!(
    /// Stable local identity of a logical Git repository.
    RepositoryId
);
stable_id!(
    /// Stable identity of one concrete worktree location within a repository.
    WorktreeId
);
stable_id!(
    /// Stable digest of Git's repository-local metadata anchor for one worktree.
    WorktreeAnchorId
);
stable_id!(
    /// Credential-free fingerprint of a repository's normalized primary remote.
    RemoteIdentity
);
stable_id!(
    /// Stable identity of one immutable observed worktree snapshot.
    SnapshotId
);
stable_id!(
    /// Stable identity of one deterministic index attempt.
    IndexRunId
);

fn write_hex(bytes: &[u8; ID_LENGTH], formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    for byte in bytes {
        write!(formatter, "{byte:02x}")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        IndexRunId, ProjectId, RemoteIdentity, RepositoryId, SnapshotId, WorktreeAnchorId,
        WorktreeId,
    };

    #[test]
    fn stable_ids_have_fixed_lowercase_hex_representation() {
        let bytes = [0xabu8; 32];

        assert_eq!(
            RepositoryId::from_bytes(bytes).to_string(),
            "abababababababababababababababababababababababababababababababab"
        );
        assert_eq!(WorktreeId::from_bytes(bytes).as_bytes(), &bytes);
        assert_eq!(WorktreeAnchorId::from_bytes(bytes).as_bytes(), &bytes);
        assert_eq!(ProjectId::from_bytes(bytes).as_bytes(), &bytes);
        assert_eq!(RemoteIdentity::from_bytes(bytes).as_bytes(), &bytes);
        assert_eq!(SnapshotId::from_bytes(bytes).as_bytes(), &bytes);
        assert_eq!(IndexRunId::from_bytes(bytes).as_bytes(), &bytes);
    }
}
