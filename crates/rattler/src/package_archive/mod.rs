use crate::{
    utils::{version_from_str, MatchSpecStr},
    MatchSpec, Version,
};
use anyhow::Context;
use serde::{Deserialize, Deserializer, Serialize};
use serde_with::serde_as;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use tokio::io::{AsyncBufRead, BufReader};
use tokio_tar::Archive;

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

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(rename_all = "lowercase")]
pub enum FileMode {
    Binary,
    Text,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Hash)]
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
    pub fn is_binary(&self) -> bool {
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

/// All supported package archives supported by Rattler.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum PackageArchiveFormat {
    TarBz2,
    TarZst,
    Conda,
}

impl PackageArchiveFormat {
    /// Determine the format of an archive based on the file name of a package. Returns the format
    /// and the original name of the package (without archive extension).
    pub fn from_file_name(file_name: &str) -> Option<(&str, Self)> {
        if let Some(name) = file_name.strip_suffix(".tar.bz2") {
            Some((name, PackageArchiveFormat::TarBz2))
        } else if let Some(name) = file_name.strip_suffix(".conda") {
            Some((name, PackageArchiveFormat::Conda))
        } else if let Some(name) = file_name.strip_suffix(".tar.zst") {
            Some((name, PackageArchiveFormat::TarZst))
        } else {
            None
        }
    }

    /// Given an archive data stream extract the contents to a specific location
    pub async fn unpack(
        &self,
        bytes: impl AsyncBufRead + Send + Unpin,
        destination: &Path,
    ) -> anyhow::Result<()> {
        match self {
            PackageArchiveFormat::TarBz2 => extract_tar_bz2(bytes, destination).await,
            PackageArchiveFormat::Conda => extract_conda(bytes, destination).await,
            PackageArchiveFormat::TarZst => extract_tar_zstd(bytes, destination).await,
        }
    }
}

/// Extracts a `.tar.bz2` archive to the specified destination
async fn extract_tar_bz2(
    bytes: impl AsyncBufRead + Send + Unpin,
    destination: &Path,
) -> anyhow::Result<()> {
    let decompressed_bytes = async_compression::tokio::bufread::BzDecoder::new(bytes);
    Archive::new(decompressed_bytes).unpack(destination).await?;
    Ok(())
}

/// Extracts a `.tar.zstd` archive to the specified destination
async fn extract_tar_zstd(
    bytes: impl AsyncBufRead + Send + Unpin,
    destination: &Path,
) -> anyhow::Result<()> {
    let decompressed_bytes = async_compression::tokio::bufread::ZstdDecoder::new(bytes);
    Archive::new(decompressed_bytes).unpack(destination).await?;
    Ok(())
}

/// Extracts a `.conda` archive to the specified destination
async fn extract_conda(
    bytes: impl AsyncBufRead + Send + Unpin,
    destination: &Path,
) -> anyhow::Result<()> {
    let mut zip_reader = async_zip::read::stream::ZipFileReader::new(bytes);
    while let Some(mut entry) = zip_reader
        .entry_reader()
        .await
        .with_context(|| format!("failed to read zip entry"))?
    {
        let entry_name = entry.entry().name();

        // Skip metadata
        if entry_name == "metadata.json" {
            entry.read_to_end_crc().await?;
            continue;
        }

        let (_, archive_format) = PackageArchiveFormat::from_file_name(entry_name)
            .ok_or_else(|| anyhow::anyhow!("unknown archive format for `{entry_name}`"))?;

        let buf_reader = BufReader::new(&mut entry);
        match archive_format {
            PackageArchiveFormat::TarBz2 => extract_tar_bz2(buf_reader, destination).await?,
            PackageArchiveFormat::TarZst => extract_tar_zstd(buf_reader, destination).await?,
            PackageArchiveFormat::Conda => {
                anyhow::bail!("conda archive cannot contain more conda archives")
            }
        }

        if !entry.compare_crc() {
            anyhow::bail!("CRC of zip entry does not match read content")
        }
    }

    Ok(())
}
