use std::error::Error;
use std::fmt;
use std::net::IpAddr;

/// Whether an Ollama-compatible endpoint stays on the host loopback boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OllamaEndpointScope {
    /// Literal IPv4 or IPv6 loopback after `localhost` normalization.
    LocalLoopback,
    /// Any DNS name or IP address outside loopback.
    Remote,
}

/// Validated credential-free origin for an Ollama-compatible API.
#[derive(Clone, PartialEq, Eq)]
pub struct OllamaEndpoint {
    url: reqwest::Url,
    scope: OllamaEndpointScope,
}

impl OllamaEndpoint {
    /// Parses one origin, normalizes `localhost` to IPv4 loopback, and rejects path/query data.
    pub fn parse(value: &str) -> Result<Self, OllamaEndpointError> {
        let mut url = reqwest::Url::parse(value).map_err(|_| OllamaEndpointError::InvalidUrl)?;
        if !matches!(url.scheme(), "http" | "https") {
            return Err(OllamaEndpointError::UnsupportedScheme);
        }
        if !url.username().is_empty() || url.password().is_some() {
            return Err(OllamaEndpointError::CredentialsForbidden);
        }
        if url.query().is_some() || url.fragment().is_some() || !matches!(url.path(), "" | "/") {
            return Err(OllamaEndpointError::OriginRequired);
        }
        let host = url.host_str().ok_or(OllamaEndpointError::MissingHost)?;
        if host.eq_ignore_ascii_case("localhost") {
            url.set_host(Some("127.0.0.1"))
                .map_err(|_| OllamaEndpointError::InvalidUrl)?;
        }
        let scope = url
            .host_str()
            .and_then(|host| host.parse::<IpAddr>().ok())
            .map_or(OllamaEndpointScope::Remote, endpoint_scope);
        if scope == OllamaEndpointScope::Remote && url.scheme() != "https" {
            return Err(OllamaEndpointError::InsecureRemote);
        }
        Ok(Self { url, scope })
    }

    /// Returns whether the validated origin is loopback or remote.
    #[must_use]
    pub const fn scope(&self) -> OllamaEndpointScope {
        self.scope
    }

    pub(crate) fn chat_url(&self) -> reqwest::Url {
        let mut url = self.url.clone();
        url.set_path("/api/chat");
        url
    }

    pub(crate) fn show_url(&self) -> reqwest::Url {
        let mut url = self.url.clone();
        url.set_path("/api/show");
        url
    }
}

impl fmt::Debug for OllamaEndpoint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OllamaEndpoint")
            .field("scheme", &self.url.scheme())
            .field("scope", &self.scope)
            .field("port", &self.url.port_or_known_default())
            .finish()
    }
}

fn endpoint_scope(address: IpAddr) -> OllamaEndpointScope {
    if address.is_loopback() {
        OllamaEndpointScope::LocalLoopback
    } else {
        OllamaEndpointScope::Remote
    }
}

/// Invalid or unsafe Ollama-compatible endpoint configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OllamaEndpointError {
    /// Input was not an absolute URL.
    InvalidUrl,
    /// Only HTTP and HTTPS are supported.
    UnsupportedScheme,
    /// URL did not contain a host.
    MissingHost,
    /// Userinfo could expose credentials through configuration or logs.
    CredentialsForbidden,
    /// Configuration must be an origin without API path, query, or fragment.
    OriginRequired,
    /// Non-loopback endpoints require HTTPS in addition to explicit policy approval.
    InsecureRemote,
}

impl fmt::Display for OllamaEndpointError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidUrl => "model endpoint is not a valid absolute URL",
            Self::UnsupportedScheme => "model endpoint scheme is unsupported",
            Self::MissingHost => "model endpoint has no host",
            Self::CredentialsForbidden => "model endpoint must not contain credentials",
            Self::OriginRequired => "model endpoint must be an origin without path or query",
            Self::InsecureRemote => "remote model endpoint must use HTTPS",
        })
    }
}

impl Error for OllamaEndpointError {}

/// Dynamic authorization checked before every model request.
pub trait OllamaEndpointPolicy: fmt::Debug + Send + Sync {
    /// Authorizes the exact current endpoint or returns a content-free denial.
    fn authorize(&self, endpoint: &OllamaEndpoint) -> Result<(), OllamaEndpointPolicyError>;
}

/// Default policy allowing only normalized loopback endpoints.
#[derive(Debug, Clone, Copy, Default)]
pub struct LocalOnlyOllamaEndpointPolicy;

impl OllamaEndpointPolicy for LocalOnlyOllamaEndpointPolicy {
    fn authorize(&self, endpoint: &OllamaEndpoint) -> Result<(), OllamaEndpointPolicyError> {
        if endpoint.scope() == OllamaEndpointScope::LocalLoopback {
            Ok(())
        } else {
            Err(OllamaEndpointPolicyError::Denied)
        }
    }
}

/// Endpoint was not covered by current local or explicit remote authorization.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OllamaEndpointPolicyError {
    /// Exact configured endpoint is not authorized.
    Denied,
}

impl fmt::Display for OllamaEndpointPolicyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("model endpoint is not authorized by current policy")
    }
}

impl Error for OllamaEndpointPolicyError {}

#[cfg(test)]
mod tests {
    use super::{
        LocalOnlyOllamaEndpointPolicy, OllamaEndpoint, OllamaEndpointError, OllamaEndpointPolicy,
        OllamaEndpointPolicyError, OllamaEndpointScope,
    };

    #[derive(Debug)]
    struct ApproveExactEndpoint(OllamaEndpoint);

    impl OllamaEndpointPolicy for ApproveExactEndpoint {
        fn authorize(&self, endpoint: &OllamaEndpoint) -> Result<(), OllamaEndpointPolicyError> {
            if endpoint == &self.0 {
                Ok(())
            } else {
                Err(OllamaEndpointPolicyError::Denied)
            }
        }
    }

    #[test]
    fn localhost_is_normalized_and_remote_requires_https_plus_policy()
    -> Result<(), Box<dyn std::error::Error>> {
        let local = OllamaEndpoint::parse("http://localhost:11434")?;
        assert_eq!(local.scope(), OllamaEndpointScope::LocalLoopback);
        assert!(LocalOnlyOllamaEndpointPolicy.authorize(&local).is_ok());
        assert!(!format!("{local:?}").contains("localhost"));

        assert_eq!(
            OllamaEndpoint::parse("http://192.0.2.1:11434"),
            Err(OllamaEndpointError::InsecureRemote)
        );
        let remote = OllamaEndpoint::parse("https://models.example.invalid")?;
        assert_eq!(remote.scope(), OllamaEndpointScope::Remote);
        assert!(!format!("{remote:?}").contains("models.example.invalid"));
        assert!(LocalOnlyOllamaEndpointPolicy.authorize(&remote).is_err());
        assert!(
            ApproveExactEndpoint(remote.clone())
                .authorize(&remote)
                .is_ok()
        );
        assert!(OllamaEndpoint::parse("http://user:secret@localhost:11434").is_err());
        assert!(OllamaEndpoint::parse("http://localhost:11434/api").is_err());
        Ok(())
    }
}
