use crate::{utils::version_from_str, MatchSpec, Version};
use serde::{Deserialize, Serialize};
use sha2::digest::Output;
use sha2::Sha256;
use std::collections::HashSet;
use std::path::PathBuf;

// #[derive(Clone, Debug, Serialize, Deserialize)]
// pub struct Index {
//     pub arch: String,
//     pub build: String,
//     pub build_number: usize,
//     pub license: String,
//     pub name: String,
//     pub subdir: String,
//     pub timestamp: usize,
//
//     #[serde(deserialize_with = "version_from_str")]
//     pub version: Version,
//
//     pub depends: Vec<MatchSpec>,
// }

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
