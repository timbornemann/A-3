//! Contract tests for the desktop health-query boundary.

use a3_desktop::CompositionRoot;
use a3_domain::{ApplicationVersion, ApplicationVersionError};
use a3_protocol::{HealthStatusV1, ProtocolVersion};

#[test]
fn composition_root_maps_domain_health_to_protocol_v1() -> Result<(), ApplicationVersionError> {
    let root = CompositionRoot::new(ApplicationVersion::try_from("1.2.3")?);

    let response = root.query_health();

    assert_eq!(response.protocol_version(), ProtocolVersion::V1);
    assert_eq!(response.application_version(), "1.2.3");
    assert_eq!(response.status(), HealthStatusV1::Ready);
    Ok(())
}

#[test]
fn package_version_builds_a_valid_composition_root() -> Result<(), ApplicationVersionError> {
    let root = CompositionRoot::from_package_version()?;

    assert_eq!(root.query_health().application_version(), "0.1.0");
    Ok(())
}
