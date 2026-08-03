/// Operating-system family supported by the A^3 desktop product.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Platform {
    /// Microsoft Windows.
    Windows,
    /// Linux distributions supported by Tauri.
    Linux,
    /// Apple macOS.
    MacOs,
    /// A build target outside the V1 support matrix.
    Unsupported,
}
