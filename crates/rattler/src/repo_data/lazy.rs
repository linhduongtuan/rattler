use super::PackageRecord;
use once_cell::sync::OnceCell;
use serde::{Deserialize, Deserializer};
use std::collections::HashMap;

#[derive(Debug)]
pub struct LazyPackageRecord<'i> {
    raw: &'i serde_json::value::RawValue,
    inner: OnceCell<Box<PackageRecord>>,
}

#[derive(Debug, serde::Deserialize)]
pub struct LazyRepoData<'i> {
    #[serde(borrow)]
    pub packages: HashMap<String, LazyPackageRecord<'i>>,
}

impl<'i> LazyPackageRecord<'i> {
    pub fn as_parsed(&self) -> Result<&PackageRecord, serde_json::Error> {
        self.inner
            .get_or_try_init(|| serde_json::from_str(self.raw.get()).map(Box::new))
            .map(Box::as_ref)
    }
}

impl<'i, 'de: 'i> Deserialize<'de> for LazyPackageRecord<'i> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(Self {
            raw: Deserialize::deserialize(deserializer)?,
            inner: Default::default(),
        })
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_partial() {
        let json = r#"{ "packages": { "foo-1.0.0-build_string": { "name": "foo", "version": "1.0.0", "build": "build_string", "build_number": 0 } } } "#;
        let repo_data: LazyRepoData = serde_json::from_str(json).unwrap();
        let _record = repo_data
            .packages
            .get("foo-1.0.0-build_string")
            .unwrap()
            .as_parsed()
            .unwrap();
    }
}
