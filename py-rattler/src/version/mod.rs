mod component;

use crate::PyRattlerError;
use component::PyComponent;
use pyo3::{basic::CompareOp, pyclass, pymethods};
use rattler_conda_types::Version;
use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    str::FromStr,
};

#[pyclass]
#[repr(transparent)]
#[derive(Clone)]
pub struct PyVersion {
    pub(crate) inner: Version,
}

impl From<Version> for PyVersion {
    fn from(value: Version) -> Self {
        PyVersion { inner: value }
    }
}

impl From<PyVersion> for Version {
    fn from(value: PyVersion) -> Self {
        value.inner
    }
}

#[pymethods]
impl PyVersion {
    #[new]
    pub fn __init__(version: &str) -> pyo3::PyResult<Self> {
        Ok(Version::from_str(version)
            .map(Into::into)
            .map_err(PyRattlerError::from)?)
    }

    /// Returns a string representation of the version.
    pub fn as_str(&self) -> String {
        format!("{}", self.inner)
    }

    /// Returns the epoch of the version.
    pub fn epoch(&self) -> Option<u64> {
        self.inner.epoch_opt()
    }

    /// Returns true if this version has a local segment defined.
    pub fn has_local(&self) -> bool {
        self.inner.has_local()
    }

    /// Returns the major and minor segments from the version.
    pub fn as_major_minor(&self) -> Option<(u64, u64)> {
        self.inner.as_major_minor()
    }

    /// Returns true if the version contains a component name "dev".
    pub fn is_dev(&self) -> bool {
        self.inner.is_dev()
    }

    /// Checks if the version and local segment start
    /// same as other version.
    pub fn starts_with(&self, other: &Self) -> bool {
        self.inner.starts_with(&other.inner)
    }

    /// Checks if this version is compatible with other version.
    pub fn compatible_with(&self, other: &Self) -> bool {
        self.inner.compatible_with(&other.inner)
    }

    /// Pops `n` number of segments from the version and returns
    /// the new version. Returns `None` if the version becomes
    /// invalid due to the operation.
    pub fn pop_segments(&self, n: usize) -> Option<Self> {
        Some(Self {
            inner: self.inner.pop_segments(n)?,
        })
    }

    /// Returns a list of segments of the version. It does not contain
    /// the local segment of the version. See `local_segments` for
    /// local segments in version.
    pub fn segments(&self) -> Vec<Vec<PyComponent>> {
        self.inner
            .segments()
            .map(|s| {
                s.components()
                    .map(|c| c.to_owned().into())
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>()
    }

    /// Returns a list of local segments of the version. It does not
    /// contain the non-local segment of the version.
    pub fn local_segments(&self) -> Vec<Vec<PyComponent>> {
        self.inner
            .local_segments()
            .map(|s| {
                s.components()
                    .map(|c| c.to_owned().into())
                    .collect::<Vec<PyComponent>>()
            })
            .collect::<Vec<_>>()
    }

    /// Returns new version with with segments ranging from `start` to `stop`.
    /// `stop` is exclusive.
    pub fn with_segments(&self, start: usize, stop: usize) -> Option<Self> {
        let range = start..stop;

        Some(Self {
            inner: self.inner.with_segments(range)?,
        })
    }

    /// Returns the number of segments in the version.
    pub fn segment_count(&self) -> usize {
        self.inner.segment_count()
    }

    /// Create a new version with local segment stripped.
    pub fn strip_local(&self) -> Self {
        Self {
            inner: self.inner.strip_local().into_owned(),
        }
    }

    /// Returns a new version where the last numerical segment of this version has been bumped.
    pub fn bump(&self) -> Self {
        Self {
            inner: self.inner.bump(),
        }
    }

    /// Compute the hash of the version.
    fn __hash__(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        self.inner.hash(&mut hasher);
        hasher.finish()
    }

    /// Performs comparison between this version and another.
    pub fn __richcmp__(&self, other: &Self, op: CompareOp) -> bool {
        op.matches(self.inner.cmp(&other.inner))
    }
}
