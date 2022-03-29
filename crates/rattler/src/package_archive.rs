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
    pub path_type: String,
    pub sha256: String,
    pub size_in_bytes: u64,
}
