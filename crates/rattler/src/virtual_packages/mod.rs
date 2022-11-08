mod cuda;

cfg_if! {
    if #[cfg(target_os = "linux")] {
        mod linux;
        pub use self::linux::DETECTED_LINUX_VERSION;
    } else {
        pub static DETECTED_LINUX_VERSION: Lazy<Option<Version>> = Lazy::new(|| None);
    }
}

cfg_if! {
    if #[cfg(unix)] {
        mod libc;
        pub use self::libc::DETECTED_LIBC_VERSION;
    } else {
        pub static DETECTED_LIBC_VERSION: Lazy<Option<(String, Version)>> = Lazy::new(|| None);
    }
}

pub use self::cuda::DETECTED_CUDA_VERSION;

use crate::{PackageRecord, Version};
use cfg_if::cfg_if;
use once_cell::sync::Lazy;
use std::str::FromStr;

#[derive(Clone, Eq, PartialEq)]
pub enum VirtualPackage {
    /// Available when running on windows
    Win,

    /// Available when running on OSX or Linux
    Unix,

    /// Available when running on linux
    Linux(Version),

    /// The version of libc supported by the OS.
    LibC(String, Version),

    /// The version of OSX if applicable
    Osx(Version),

    /// The maximum version of Cuda supported by the display driver.
    Cuda(Version),

    /// The architecture spec of the system
    ArchSpec(String),
}

impl From<VirtualPackage> for PackageRecord {
    fn from(pkg: VirtualPackage) -> Self {
        match pkg {
            VirtualPackage::Win => PackageRecord::new(
                String::from("__win"),
                Version::from_str("0").unwrap(),
                String::from("0"),
                0,
            ),
            VirtualPackage::Unix => PackageRecord::new(
                String::from("__unix"),
                Version::from_str("0").unwrap(),
                String::from("0"),
                0,
            ),
            VirtualPackage::Linux(version) => {
                PackageRecord::new(String::from("__linux"), version, String::from("0"), 0)
            }
            VirtualPackage::LibC(family, version) => PackageRecord::new(
                format!("__{}", family.to_lowercase()),
                version,
                String::from("0"),
                0,
            ),
            VirtualPackage::Osx(version) => {
                PackageRecord::new(String::from("__osx"), version, String::from("0"), 0)
            }
            VirtualPackage::Cuda(version) => {
                PackageRecord::new(String::from("__cuda"), version, String::from("0"), 0)
            }
            VirtualPackage::ArchSpec(spec) => PackageRecord::new(
                String::from("__archspec"),
                Version::from_str("1").unwrap(),
                spec,
                0,
            ),
        }
    }
}

/// Memoized virtual packages
pub static DETECTED_VIRTUAL_PACKAGES: Lazy<Vec<VirtualPackage>> =
    Lazy::new(detect_virtual_packages);

/// Determine virtual packages for the current environment
fn detect_virtual_packages() -> Vec<VirtualPackage> {
    let mut virtual_packages = Vec::new();

    #[cfg(target_os = "linux")]
    {
        virtual_packages.push(VirtualPackage::Unix);

        if let Some(linux_version) = DETECTED_LINUX_VERSION.as_ref() {
            virtual_packages.push(VirtualPackage::Linux(linux_version.clone()));
        }

        if let Some((libc_family, libc_version)) = DETECTED_LIBC_VERSION.as_ref() {
            virtual_packages.push(VirtualPackage::LibC(
                libc_family.clone(),
                libc_version.clone(),
            ));
        }
    }
    #[cfg(windows)]
    {
        virtual_packages.push(VirtualPackage::Win);
    }
    #[cfg(target_os = "macos")]
    {
        virtual_packages.push(VirtualPackage::Unix);

        // TODO: MacOs version!
    }

    if let Some(cuda_version) = DETECTED_CUDA_VERSION.as_ref() {
        virtual_packages.push(VirtualPackage::Cuda(cuda_version.clone()))
    }

    virtual_packages.push(VirtualPackage::ArchSpec(std::env::consts::ARCH.to_owned()));

    virtual_packages
}
