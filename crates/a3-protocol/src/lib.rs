//! Versioned, infrastructure-independent IPC boundary types for A^3.

mod health;
mod version;

pub use health::{HealthResponseV1, HealthStatusV1};
pub use version::ProtocolVersion;
