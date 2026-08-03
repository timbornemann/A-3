//! Framework-independent domain model and invariants for A^3.

mod health;
mod platform;
mod version;

pub use health::Health;
pub use platform::Platform;
pub use version::{ApplicationVersion, ApplicationVersionError};
