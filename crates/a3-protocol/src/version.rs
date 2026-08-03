/// Version of the A^3 IPC protocol carried by every boundary message.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ProtocolVersion(u16);

impl ProtocolVersion {
    /// First supported protocol version.
    pub const V1: Self = Self(1);

    /// Protocol version emitted by this build.
    pub const CURRENT: Self = Self::V1;

    /// Creates a protocol version from its wire integer.
    #[must_use]
    pub const fn new(value: u16) -> Self {
        Self(value)
    }

    /// Returns the integer representation used at the IPC boundary.
    #[must_use]
    pub const fn get(self) -> u16 {
        self.0
    }
}
