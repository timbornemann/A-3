//! Framework-independent domain model and invariants for A^3.

mod health;
mod version;

pub use health::Health;
pub use version::{ApplicationVersion, ApplicationVersionError};
