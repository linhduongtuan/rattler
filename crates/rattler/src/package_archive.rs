use crate::{
    utils::{version_from_str, MatchSpecStr},
    MatchSpec, Version,
};
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use sha2::digest::Output;
use sha2::Sha256;
use std::collections::HashSet;
use std::path::PathBuf;

#[serde_as]
#[derive(Clone, Debug, Deserialize)]
pub struct Index {
    pub arch: Option<String>,
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

#[serde(rename_all = "lowercase")]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub enum FileMode {
    Binary,
    Text,
}

#[serde(rename_all = "lowercase")]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Hash)]
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
