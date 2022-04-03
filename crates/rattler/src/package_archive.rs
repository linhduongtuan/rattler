use crate::{
    utils::{version_from_str, MatchSpecStr},
    MatchSpec, Version,
};
use serde::{Deserialize, Deserializer, Serialize};
use serde_with::serde_as;
use std::collections::HashSet;
use std::path::PathBuf;

#[derive(Debug, Copy, Clone)]
pub enum NoArchType {
    GenericV1,
    GenericV2,
    Python,
}

#[serde_as]
#[derive(Clone, Debug, Deserialize)]
pub struct Index {
    pub arch: Option<String>,

    #[serde(deserialize_with = "deserialize_no_arch", default)]
    pub noarch: Option<NoArchType>,

    pub build: String,
    pub build_number: usize,
    pub license: Option<String>,
    pub license_family: Option<String>,
    pub name: String,
    pub subdir: String,
    pub timestamp: Option<usize>,

    #[serde(deserialize_with = "version_from_str")]
    pub version: Version,

    #[serde_as(as = "Vec<MatchSpecStr>")]
    pub depends: Vec<MatchSpec>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Paths {
    pub paths_version: usize,
    pub paths: HashSet<PathEntry>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub struct PathEntry {
    #[serde(rename = "_path")]
    pub relative_path: PathBuf,
    pub path_type: PathType,
    pub sha256: String,
    pub size_in_bytes: u64,

    #[serde(default, skip_serializing_if = "FileMode::is_binary")]
    pub file_mode: FileMode,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prefix_placeholder: Option<String>,

    #[serde(
        default = "no_link_default",
        skip_serializing_if = "is_no_link_default"
    )]
    pub no_link: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(rename_all = "lowercase")]
pub enum FileMode {
    Binary,
    Text,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(rename_all = "lowercase")]
pub enum PathType {
    HardLink,
    SoftLink,
    Directory,
}

impl Default for FileMode {
    fn default() -> Self {
        FileMode::Binary
    }
}

impl FileMode {
    fn is_binary(&self) -> bool {
        matches!(self, FileMode::Binary)
    }
}

/// Returns the default value for the "no_link" value of a [`PathEntry`]
fn no_link_default() -> bool {
    false
}

/// Returns true if the value is equal to the default value for the "no_link" value of a [`PathEntry`]
fn is_no_link_default(value: &bool) -> bool {
    *value == no_link_default()
}

#[derive(Clone, Debug, Deserialize)]
#[serde(untagged)]
enum NoArchSerde {
    OldFormat(bool),
    NewFormat(NoArchTypeSerde),
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
enum NoArchTypeSerde {
    Python,
    Generic,
}

// Helper function to parse the `noarch` field in conda package index.json.
fn deserialize_no_arch<'de, D>(deserializer: D) -> Result<Option<NoArchType>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Option::<NoArchSerde>::deserialize(deserializer)?;
    Ok(value.and_then(|value| match value {
        NoArchSerde::OldFormat(true) => Some(NoArchType::GenericV1),
        NoArchSerde::OldFormat(false) => None,
        NoArchSerde::NewFormat(NoArchTypeSerde::Python) => Some(NoArchType::Python),
        NoArchSerde::NewFormat(NoArchTypeSerde::Generic) => Some(NoArchType::GenericV2),
    }))
}
