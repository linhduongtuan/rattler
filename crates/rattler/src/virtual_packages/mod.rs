mod cuda;
mod libc;

use crate::Version;

#[derive(Clone, Eq, PartialEq)]
pub enum VirtualPackage {
    /// Available when running on windows
    Win,

    /// Available when running on OSX or Linux
    Unix,

    /// Available when running on linux
    Linux,

    /// The version of libc supported by the OS.
    LibC(String, Version),

    /// The version of OSX if applicable
    Osx(Version),

    /// The maximum version of Cuda supported by the display driver.
    Cuda(Version),

    /// The architecture spec of the system
    ArchSpec(String),
}

/// Determine OS specific virtual packages
fn detect_os() -> &'static [VirtualPackage] {
    #[cfg(target_os = "linux")]
    {
        static OS_PACKAGES: [VirtualPackage; 2] = [VirtualPackage::Linux, VirtualPackage::Unix];
        return &OS_PACKAGES;
    }
    #[cfg(windows)]
    {
        static OS_PACKAGES: [VirtualPackage; 1] = [VirtualPackage::Win];
        return &OS_PACKAGES;
    }
    #[cfg(target_os = "macos")]
    {
        static OS_PACKAGES: [VirtualPackage; 2] = [VirtualPackage::Osx(..), VirtualPackage::Unix];
        return &OS_PACKAGES;
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
    compile_error!("unsupported target os");
}
