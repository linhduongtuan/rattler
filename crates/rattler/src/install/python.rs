use crate::Version;
use std::borrow::Cow;
use std::path::{Path, PathBuf};
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

    /// Path to the binary directory
    bin_dir: PathBuf,
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

        // Binary directory
        #[cfg(windows)]
        let bin_dir = PathBuf::from("Scripts");
        #[cfg(not(windows))]
        let bin_dir = PathBuf::from("bin");

        Ok(Self {
            short_version: (major, minor),
            path,
            site_packages_path,
            bin_dir,
        })
    }

    /// Returns the path to the python executable
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Returns the target location of a file in a noarch python package given its location in its
    /// package archive.
    pub fn get_python_noarch_target_path<'a>(&self, relative_path: &'a Path) -> Cow<'a, Path> {
        if let Ok(rest) = relative_path.strip_prefix("site-packages/") {
            self.site_packages_path.join(rest).into()
        } else if let Ok(rest) = relative_path.strip_prefix("python-scripts/") {
            self.bin_dir.join(rest).into()
        } else {
            relative_path.into()
        }
    }
}
