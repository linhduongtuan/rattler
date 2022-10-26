use super::PackageRecord;
use crate::repo_data::fetch::RepoDataFromBytes;
use bytes::Bytes;
use ouroboros::self_referencing;
use serde::de::{Error, MapAccess};
use serde::Deserializer;
use std::collections::HashMap;
use std::fmt;
use std::marker::PhantomData;
use std::path::Path;
use thiserror::Error;

#[derive(Debug)]
pub struct LazyPackageRecord<'i> {
    raw: &'i serde_json::value::RawValue,
}

#[derive(Debug, serde::Deserialize)]
pub struct LazyRepoData<'i> {
    #[serde(borrow, deserialize_with = "deserialize_packages")]
    pub packages: HashMap<String, Vec<LazyPackageRecord<'i>>>,
}

impl<'i> LazyPackageRecord<'i> {
    pub fn parse(&self) -> Result<PackageRecord, serde_json::Error> {
        serde_json::from_str(self.raw.get())
    }
}

fn deserialize_packages<'i, 'de: 'i, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<HashMap<String, Vec<LazyPackageRecord<'i>>>, D::Error> {
    #[derive(Default)]
    struct PackageVisitor<'i> {
        _data: PhantomData<&'i ()>,
    }

    impl<'i, 'de: 'i> serde::de::Visitor<'de> for PackageVisitor<'i> {
        type Value = HashMap<String, Vec<LazyPackageRecord<'i>>>;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("a package map")
        }

        fn visit_map<M>(self, mut access: M) -> Result<Self::Value, M::Error>
        where
            M: MapAccess<'de>,
        {
            let mut map: HashMap<String, Vec<LazyPackageRecord<'i>>> = HashMap::new();
            while let Some((key, value)) = access.next_entry()? {
                let package = package_from_filename(key).ok_or_else(|| {
                    M::Error::custom("could not extract package name from filename")
                })?;
                match map.get_mut(package) {
                    None => {
                        map.insert(package.to_owned(), vec![value]);
                    }
                    Some(entries) => entries.push(value),
                }
            }

            Ok(map)
        }
    }

    deserializer.deserialize_map(PackageVisitor::default())
}

/// Extract the package name from a conda package filename
fn package_from_filename(filename: &str) -> Option<&str> {
    let (rest, _build_string) = filename.rsplit_once('-')?;
    let (package_name, _version) = rest.rsplit_once('-')?;
    Some(package_name)
}

impl<'i, 'de: 'i> serde::Deserialize<'de> for LazyPackageRecord<'i> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(Self {
            raw: serde::Deserialize::deserialize(deserializer)?,
        })
    }
}

#[self_referencing]
pub struct OwnedLazyRepoData {
    bytes: Bytes,

    #[borrows(bytes)]
    #[covariant]
    repo_data: LazyRepoData<'this>,
}

impl RepoDataFromBytes for OwnedLazyRepoData {
    fn from_bytes(bytes: Bytes) -> Result<Self, serde_json::Error> {
        OwnedLazyRepoDataTryBuilder {
            bytes,
            repo_data_builder: |bytes: &Bytes| serde_json::from_slice(bytes),
        }
        .try_build()
    }
}

impl OwnedLazyRepoData {
    pub fn as_ref(&self) -> &LazyRepoData {
        self.borrow_repo_data()
    }
}

#[derive(Error, Debug)]
pub enum OwnedLazyRepoDataFromFileError {
    #[error("error deserializing")]
    Deserialize(#[from] serde_json::Error),
    #[error("error reading file")]
    IoError(#[from] std::io::Error),
}

impl OwnedLazyRepoData {
    /// Constructs an new instance directory from a file path.
    pub fn from_file(
        path: impl AsRef<Path>,
    ) -> Result<OwnedLazyRepoData, OwnedLazyRepoDataFromFileError> {
        Ok(Self::from_bytes(Bytes::from(std::fs::read(path)?))?)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_package_from_filename() {
        assert_eq!(
            package_from_filename("zstd-1.5.2-h8a70e8d_1.tar.bz2"),
            Some("zstd")
        );
        assert_eq!(
            package_from_filename("jupyter-lsp-0.8.0-py_0.tar.bz2"),
            Some("jupyter-lsp")
        );
        assert_eq!(
            package_from_filename("r-markdown-0.8-r3.3.2_1.tar.bz2"),
            Some("r-markdown")
        )
    }

    #[test]
    fn test_partial() {
        let json = r#"{ "packages": { "foo-1.0.0-build_string": { "name": "foo", "version": "1.0.0", "build": "build_string", "build_number": 0 } } } "#;
        let repo_data: LazyRepoData = serde_json::from_str(json).unwrap();
        assert_eq!(repo_data.packages.get("foo").unwrap().len(), 1);
    }
}
