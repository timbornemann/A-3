//! Versioned, infrastructure-independent IPC boundary types for A^3.

mod health;
mod version;

pub use health::{
    CommandErrorV1, ErrorCodeV1, HealthRequestV1, HealthResponseV1, HealthStatusV1, PlatformV1,
};
pub use version::ProtocolVersion;
