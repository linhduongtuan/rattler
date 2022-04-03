use crate::Version;
use std::path::PathBuf;
use thiserror::Error;

/// Information required for linking no-arch python packages.
#[derive(Debug, Clone)]
pub struct PythonInfo {
    /// The major and minor version
    short_version: (usize, usize),

    /// The relative path to the python executable
    path: PathBuf,

    /// The relative path to where site-packages are stored
    site_packages_path: PathBuf,
}

#[derive(Debug, Clone, Error)]
pub enum PythonInfoError {
    #[error("invalid python version '{0}'")]
    InvalidVersion(Version),
}

impl PythonInfo {
    /// Build an instance based on the version of the python package.
    pub fn from_version(version: &Version) -> Result<Self, PythonInfoError> {
        // Determine the major, and minor versions of the version
        let (major, minor) = version
            .as_major_minor()
            .ok_or_else(|| PythonInfoError::InvalidVersion(version.clone()))?;

        // Determine the expected relative path of the executable in a prefix
        #[cfg(windows)]
        let path = PathBuf::from("python.exe");
        #[cfg(not(windows))]
        let path = PathBuf::from(format!("bin/python{}.{}", major, minor));

        // Find the location of the site packages
        #[cfg(windows)]
        let site_packages_path = PathBuf::from("Lib/site-packages");
        #[cfg(not(windows))]
        let site_packages_path =
            PathBuf::from(format!("lib/python{}.{}/site-packages", major, minor));

        Ok(Self {
            short_version: (major, minor),
            path,
            site_packages_path,
        })
    }
}
