use crate::ProtocolVersion;
use serde::{Deserialize, Serialize};

/// Strict bounded request for one page of the active project's published repository tree.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct QueryRepositoryTreeRequestV1 {
    protocol_version: ProtocolVersion,
    directory_path_hex: Option<String>,
    after_name_hex: Option<String>,
    limit: u16,
}

impl QueryRepositoryTreeRequestV1 {
    /// Creates a request whose opaque path and cursor are validated by the Rust command boundary.
    #[must_use]
    pub const fn new(
        protocol_version: ProtocolVersion,
        directory_path_hex: Option<String>,
        after_name_hex: Option<String>,
        limit: u16,
    ) -> Self {
        Self {
            protocol_version,
            directory_path_hex,
            after_name_hex,
            limit,
        }
    }

    /// Creates the first root page with the product default size.
    #[must_use]
    pub const fn root() -> Self {
        Self::new(ProtocolVersion::CURRENT, None, None, 50)
    }

    /// Returns the requested protocol version.
    #[must_use]
    pub const fn protocol_version(&self) -> ProtocolVersion {
        self.protocol_version
    }

    /// Returns a lossless repository-path token, or None for root.
    #[must_use]
    pub fn directory_path_hex(&self) -> Option<&str> {
        self.directory_path_hex.as_deref()
    }

    /// Returns an exclusive direct-child cursor token.
    #[must_use]
    pub fn after_name_hex(&self) -> Option<&str> {
        self.after_name_hex.as_deref()
    }

    /// Returns the untrusted page-size value for application validation.
    #[must_use]
    pub const fn limit(&self) -> u16 {
        self.limit
    }
}

/// Versioned progressive tree result selected from the Core-owned active project.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct RepositoryTreeResponseV1 {
    protocol_version: ProtocolVersion,
    result: RepositoryTreeResultV1,
}

impl RepositoryTreeResponseV1 {
    /// Creates the response used before a project is active.
    #[must_use]
    pub const fn no_project() -> Self {
        Self {
            protocol_version: ProtocolVersion::CURRENT,
            result: RepositoryTreeResultV1::NoProject,
        }
    }

    /// Creates the response used before the first atomic publication.
    #[must_use]
    pub const fn no_published_index() -> Self {
        Self {
            protocol_version: ProtocolVersion::CURRENT,
            result: RepositoryTreeResultV1::NoPublishedIndex,
        }
    }

    /// Creates an available page from application-validated bounded values.
    #[must_use]
    pub fn available(page: RepositoryTreePageV1) -> Self {
        Self {
            protocol_version: ProtocolVersion::CURRENT,
            result: RepositoryTreeResultV1::Available {
                page: Box::new(page),
            },
        }
    }

    /// Returns the mutually exclusive project/publication result.
    #[must_use]
    pub const fn result(&self) -> &RepositoryTreeResultV1 {
        &self.result
    }
}

/// Whether an active project and current atomic publication exist.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase", tag = "status")]
pub enum RepositoryTreeResultV1 {
    /// No project is active in this desktop process.
    NoProject,
    /// A project is active but no index has crossed the publish boundary.
    NoPublishedIndex,
    /// One bounded root or directory page is available.
    Available {
        /// Current atomic page containing at most one hundred direct children.
        page: Box<RepositoryTreePageV1>,
    },
}

/// Bounded WebView-safe page of direct published repository children.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct RepositoryTreePageV1 {
    index_run_id: String,
    snapshot_id: String,
    directory_path_hex: Option<String>,
    entries: Vec<RepositoryTreeEntryV1>,
    next_after_name_hex: Option<String>,
}

impl RepositoryTreePageV1 {
    /// Creates one strict page from already validated application values.
    #[must_use]
    pub const fn new(
        index_run_id: String,
        snapshot_id: String,
        directory_path_hex: Option<String>,
        entries: Vec<RepositoryTreeEntryV1>,
        next_after_name_hex: Option<String>,
    ) -> Self {
        Self {
            index_run_id,
            snapshot_id,
            directory_path_hex,
            entries,
            next_after_name_hex,
        }
    }

    /// Returns the exact atomic index run behind the page.
    #[must_use]
    pub fn index_run_id(&self) -> &str {
        &self.index_run_id
    }

    /// Returns the immutable snapshot behind the page.
    #[must_use]
    pub fn snapshot_id(&self) -> &str {
        &self.snapshot_id
    }

    /// Returns the lossless enumerated directory token, or None for root.
    #[must_use]
    pub fn directory_path_hex(&self) -> Option<&str> {
        self.directory_path_hex.as_deref()
    }

    /// Returns at most one hundred direct children in canonical byte order.
    #[must_use]
    pub fn entries(&self) -> &[RepositoryTreeEntryV1] {
        &self.entries
    }

    /// Returns the exclusive cursor for another page.
    #[must_use]
    pub fn next_after_name_hex(&self) -> Option<&str> {
        self.next_after_name_hex.as_deref()
    }
}

/// Structural kind of one direct repository-tree child.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RepositoryTreeEntryKindV1 {
    /// A derived directory prefix with indexed descendants.
    Directory,
    /// One exact current file revision.
    File,
}

/// One direct child with lossless navigation token and bounded display text.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct RepositoryTreeEntryV1 {
    kind: RepositoryTreeEntryKindV1,
    path_hex: String,
    name: String,
    name_truncated: bool,
    descendant_file_count: String,
    content_hash: Option<String>,
}

impl RepositoryTreeEntryV1 {
    /// Creates one application-validated directory or file projection.
    #[must_use]
    pub const fn new(
        kind: RepositoryTreeEntryKindV1,
        path_hex: String,
        name: String,
        name_truncated: bool,
        descendant_file_count: String,
        content_hash: Option<String>,
    ) -> Self {
        Self {
            kind,
            path_hex,
            name,
            name_truncated,
            descendant_file_count,
            content_hash,
        }
    }

    /// Returns whether this child is a directory or file.
    #[must_use]
    pub const fn kind(&self) -> RepositoryTreeEntryKindV1 {
        self.kind
    }

    /// Returns the lossless path token used only by this indexed read API.
    #[must_use]
    pub fn path_hex(&self) -> &str {
        &self.path_hex
    }

    /// Returns bounded sanitized display text.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns whether display text omitted characters.
    #[must_use]
    pub const fn name_truncated(&self) -> bool {
        self.name_truncated
    }

    /// Returns the exact number of indexed files below this child.
    #[must_use]
    pub fn descendant_file_count(&self) -> &str {
        &self.descendant_file_count
    }

    /// Returns exact file-revision evidence for files and None for directories.
    #[must_use]
    pub fn content_hash(&self) -> Option<&str> {
        self.content_hash.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        QueryRepositoryTreeRequestV1, RepositoryTreeEntryKindV1, RepositoryTreeEntryV1,
        RepositoryTreePageV1, RepositoryTreeResponseV1,
    };
    use crate::ProtocolVersion;

    #[test]
    fn available_page_serializes_lossless_tokens_counts_and_file_evidence()
    -> Result<(), Box<dyn std::error::Error>> {
        let response = RepositoryTreeResponseV1::available(RepositoryTreePageV1::new(
            "11".repeat(32),
            "22".repeat(32),
            None,
            vec![RepositoryTreeEntryV1::new(
                RepositoryTreeEntryKindV1::File,
                "7372632f6c69622e7273".to_owned(),
                "lib.rs".to_owned(),
                false,
                "1".to_owned(),
                Some("33".repeat(32)),
            )],
            Some("6c69622e7273".to_owned()),
        ));

        let value = serde_json::to_value(response)?;
        assert_eq!(value["protocolVersion"], 1);
        assert_eq!(value["result"]["status"], "available");
        assert_eq!(value["result"]["page"]["entries"][0]["kind"], "file");
        assert_eq!(
            value["result"]["page"]["entries"][0]["descendantFileCount"],
            "1"
        );
        assert_eq!(
            value["result"]["page"]["entries"][0]["contentHash"],
            "33".repeat(32)
        );
        Ok(())
    }

    #[test]
    fn request_rejects_unknown_fields_and_retains_untrusted_values_for_core_validation()
    -> Result<(), Box<dyn std::error::Error>> {
        let request = serde_json::from_value::<QueryRepositoryTreeRequestV1>(serde_json::json!({
            "protocolVersion": 1,
            "directoryPathHex": "ff",
            "afterNameHex": null,
            "limit": 100
        }))?;
        assert_eq!(request.protocol_version(), ProtocolVersion::V1);
        assert_eq!(request.directory_path_hex(), Some("ff"));
        assert_eq!(request.limit(), 100);

        let unknown = serde_json::json!({
            "protocolVersion": 1,
            "directoryPathHex": null,
            "afterNameHex": null,
            "limit": 50,
            "workspacePath": "C:/untrusted"
        });
        assert!(serde_json::from_value::<QueryRepositoryTreeRequestV1>(unknown).is_err());
        Ok(())
    }
}
