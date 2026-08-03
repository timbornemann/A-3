use a3_domain::Platform;

pub(crate) struct SystemPlatform;

impl SystemPlatform {
    pub(crate) const fn current() -> Platform {
        #[cfg(target_os = "windows")]
        {
            Platform::Windows
        }

        #[cfg(target_os = "linux")]
        {
            Platform::Linux
        }

        #[cfg(target_os = "macos")]
        {
            Platform::MacOs
        }

        #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
        {
            Platform::Unsupported
        }
    }
}
