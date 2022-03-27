use crate::package_archive::{PathEntry, Paths};
use anyhow::Context;
use bytes::Bytes;
use futures::{future, Stream, TryFutureExt, TryStreamExt};
use itertools::Itertools;
use once_cell::sync::Lazy;
use reqwest_middleware::{ClientBuilder, ClientWithMiddleware};
use reqwest_retry::policies::ExponentialBackoff;
use reqwest_retry::RetryTransientMiddleware;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use thiserror::Error;
use tokio::fs;
use tokio::io;
use tokio_stream::StreamExt;
use tokio_tar::Archive;
use tokio_util::codec::{BytesCodec, FramedRead};
use tokio_util::io::StreamReader;
use url::Url;

pub trait Package {
    /// Returns the unique identifier of this package
    fn filename(&self) -> &str;

    /// The URL to download the package content from
    fn url(&self) -> &Url;

    /// Returns the filenames of the packages that this package depends on or None if this cannot
    /// be determined. If this can not be determine the contents of the package is examined to
    /// find the dependencies.
    fn dependencies(&self) -> Option<&[&str]> {
        None
    }
}

/// Constructs a `reqwest` client.
fn construct_client() -> ClientWithMiddleware {
    let retry_policy = ExponentialBackoff::builder().build_with_max_retries(3);
    ClientBuilder::new(reqwest::Client::new())
        .with(RetryTransientMiddleware::new_with_policy(retry_policy))
        .build()
}

type LazyClient = Arc<Lazy<ClientWithMiddleware>>;

/// Installs the specified packages to the specified destination.
pub async fn install_prefix<P: Package>(
    packages: impl IntoIterator<Item = P>,
    prefix: impl AsRef<Path>,
    package_cache_path: impl AsRef<Path>,
) -> anyhow::Result<()> {
    let package_cache_path = package_cache_path.as_ref().to_path_buf();
    tokio::fs::create_dir_all(&package_cache_path).await?;

    let client: LazyClient = Arc::new(Lazy::new(construct_client));
    let packages = packages.into_iter().collect_vec();

    // Download all the package archives
    let _result: Vec<_> = future::try_join_all(packages.iter().map(|package| {
        tokio::spawn(ensure_package_archive(
            package.filename().to_owned(),
            package.url().clone(),
            client.clone(),
            package_cache_path.clone(),
        ))
        .unwrap_or_else(|e| anyhow::Result::Err(e.into()))
    }))
    .await?;

    // Determine a topological ordering of all the packages

    Ok(())
}

/// Ensures that the package with the given `package_file_name` exists in the directory specified by
/// `package_cache_path`. If the archive already exists it is validated. If it doesnt exist or is
/// not valid, the archive is re-downloaded.
async fn ensure_package_archive(
    package_file_name: String,
    url: Url,
    client: LazyClient,
    package_cache_path: PathBuf,
) -> anyhow::Result<PathBuf> {
    // Determine archive format and name
    let (name, format) = PackageArchiveFormat::from_file_name(&package_file_name)
        .ok_or_else(|| anyhow::anyhow!("unsupported package archive format"))?;

    // Determine where the package should be stored
    let destination = package_cache_path.join(name);

    // If the package already exists, check if it's valid
    if destination.is_dir() {
        match validate_package(&destination).await {
            Ok(()) => {
                log::debug!("contents of `{}` succesfully validated", &package_file_name);
                return Ok(destination);
            }
            Err(e) => log::warn!("contents of `{}` is invalid: {e}", &package_file_name),
        }
    }

    // Clean the previous directory to ensure no files remain
    create_clean_dir_all(&destination).await?;

    // Download the package
    let client = (**client).clone();
    fetch_and_extract(client, url.clone(), format, destination.clone()).await?;

    Ok(destination)
}

/// Ensures there is a clean directory at the specified location, this means that there is a
/// directory which doesnt contain anything.
async fn create_clean_dir_all(path: impl AsRef<Path>) -> io::Result<()> {
    let path = path.as_ref();
    if path.is_dir() {
        fs::remove_dir_all(path).await?;
    } else if path.is_file() {
        fs::remove_file(path).await?;
    }
    fs::create_dir_all(path).await
}

/// Downloads the specified package to a package cache directory. This function always overwrites
/// whatever was there.
async fn fetch_and_extract(
    client: ClientWithMiddleware,
    package_url: Url,
    format: PackageArchiveFormat,
    destination: PathBuf,
) -> anyhow::Result<()> {
    // Start downloading the package
    let response = client
        .get(package_url.clone())
        .send()
        .await?
        .error_for_status()?;

    // Construct stream of byte chunks from the download
    let bytes = response.bytes_stream();

    // Extract the contents of the package
    format.unpack(bytes, &destination).await?;

    // Report success
    log::debug!("extracted {package_url} to {}", destination.display());

    Ok(())
}

/// Extracts a `.tar.bz2` archive to the specified destination
async fn extract_tar_bz2(
    bytes: impl Stream<Item = reqwest::Result<Bytes>> + Unpin,
    destination: &Path,
) -> anyhow::Result<()> {
    let decompressed_bytes = async_compression::tokio::bufread::BzDecoder::new(StreamReader::new(
        bytes.map_err(|e| io::Error::new(io::ErrorKind::Other, e)),
    ));
    Archive::new(decompressed_bytes).unpack(destination).await?;
    Ok(())
}

/// Extracts a `.conda` archive to the specified destination
async fn extract_conda(
    bytes: impl Stream<Item = reqwest::Result<Bytes>> + Unpin,
    destination: &Path,
) -> anyhow::Result<()> {
    Err(anyhow::anyhow!("unsupported"))
}

#[derive(Debug, Error)]
enum ValidationError {
    #[error("could not open paths.json")]
    CouldNotOpenPathsJson(#[source] io::Error),

    #[error("could not deserialize paths.json")]
    CouldNotDeserializePaths(#[source] serde_json::Error),

    #[error("could not determine metadata of '{0}'")]
    FileMetaDataError(String, #[source] io::Error),

    #[error("`{0}` is not a file")]
    NotAFile(String),

    #[error("`{0}` size mismatch: exptected {1}, got {2}")]
    FileSizeMismatch(String, u64, u64),

    #[error("error computing file digest")]
    DigestError(#[source] anyhow::Error),

    #[error("`{0}` digest mismatch, expected {1}, got {2}")]
    DigestMismatch(String, String, String),

    #[error("unknown error")]
    Unknown(#[source] anyhow::Error),
}

/// Computes the sha256 digest for the file at the given path.
async fn compute_sha256_digest(path: &Path) -> anyhow::Result<String> {
    let file = tokio::fs::File::open(path)
        .await
        .with_context(|| format!("unable to open {}", path.display()))?;

    let mut ctx = Sha256::new();
    let mut frames = FramedRead::new(file, BytesCodec::new());
    while let Some(frame) = frames.next().await {
        ctx.update(&frame.with_context(|| format!("failed to read '{}'", path.display()))?);
    }

    Ok(format!("{:x}", ctx.finalize()))
}

/// Validates the contents of an extracted package entry.
async fn validate_package_entry(
    archive_path: PathBuf,
    entry: PathEntry,
) -> Result<(), ValidationError> {
    let entry_path = archive_path.join(&entry.relative_path);
    let metadata = tokio::fs::metadata(&entry_path).await.map_err(|e| {
        ValidationError::FileMetaDataError(entry.relative_path.display().to_string(), e)
    })?;

    // Make sure the file is a file, and not something else
    if !metadata.is_file() {
        return Err(ValidationError::NotAFile(
            entry.relative_path.display().to_string(),
        ));
    }

    // Make sure the size of the file matches what we expect
    if metadata.len() != entry.size_in_bytes {
        return Err(ValidationError::FileSizeMismatch(
            entry.relative_path.display().to_string(),
            entry.size_in_bytes,
            metadata.len(),
        ));
    }

    let digest = compute_sha256_digest(&entry_path)
        .await
        .map_err(|e| ValidationError::DigestError(e))?;

    if entry.sha256 != digest {
        return Err(ValidationError::DigestMismatch(
            entry.relative_path.display().to_string(),
            entry.sha256.clone(),
            digest,
        ));
    }

    Ok(())
}

/// Validates extracted package contents
async fn validate_package(archive_path: &PathBuf) -> Result<(), ValidationError> {
    // Read the contents of the paths.json file
    let paths_file = tokio::fs::File::open(&archive_path.join("info/paths.json"))
        .await
        .map_err(ValidationError::CouldNotOpenPathsJson)?
        .into_std()
        .await;
    let paths: Paths =
        serde_json::from_reader(paths_file).map_err(ValidationError::CouldNotDeserializePaths)?;

    // Iterate over all files and determine whether they are valid
    let _result = future::try_join_all(
        paths
            .paths
            .iter()
            .map(|entry| validate_package_entry(archive_path.to_path_buf(), entry.clone()))
            .map(|e| tokio::spawn(e).unwrap_or_else(|e| Err(ValidationError::Unknown(e.into())))),
    )
    .await?;

    Ok(())
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
enum PackageArchiveFormat {
    TarBz2,
    Conda,
}

impl PackageArchiveFormat {
    /// Determine the format of an archive based on the file name of a package. Returns the format
    /// and the original name of the package (without archive extension).
    pub fn from_file_name(file_name: &str) -> Option<(&str, Self)> {
        if let Some(name) = file_name.strip_suffix(".tar.bz2") {
            Some((name, PackageArchiveFormat::TarBz2))
        // } else if let Some(name) = file_name.strip_suffix(".conda") {
        //     Some((name, PackageArchiveFormat::Conda))
        } else {
            None
        }
    }

    /// Given an archive data stream extract the contents to a specific location
    pub async fn unpack(
        &self,
        bytes: impl Stream<Item = reqwest::Result<Bytes>> + Unpin,
        destination: &Path,
    ) -> anyhow::Result<()> {
        match self {
            PackageArchiveFormat::TarBz2 => extract_tar_bz2(bytes, destination).await,
            PackageArchiveFormat::Conda => extract_conda(bytes, destination).await,
        }
    }
}
