use crate::package_archive::{Index, PathEntry, Paths};
use anyhow::Context;
use bytes::Bytes;
use futures::stream::FuturesUnordered;
use futures::{future, FutureExt, Stream, TryFutureExt, TryStreamExt};
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
use tokio::io::{AsyncBufRead, BufReader};
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

    // Create tasks to download all packages
    let mut download_tasks = FuturesUnordered::new();
    for package in packages.iter() {
        let package_name = package.filename();
        let package_task = tokio::spawn(install_package(
            package_name.to_owned(),
            package.url().clone(),
            client.clone(),
            package_cache_path.clone(),
        ))
        .unwrap_or_else(|e| anyhow::Result::Err(e.into()))
        .map(move |r| r.with_context(|| format!("error installing package `{}`", package_name)));
        download_tasks.push(package_task);
    }

    // Wait for all tasks to complete
    while let Some(download_task) = download_tasks.next().await {
        let _ = download_task?;
    }

    Ok(())
}

pub async fn install_package(
    package_file_name: String,
    url: Url,
    client: LazyClient,
    package_cache_path: PathBuf,
) -> anyhow::Result<()> {
    // Ensure that the content of the package is stored on disk.
    let archive_path =
        ensure_package_archive(&package_file_name, &url, client, &package_cache_path).await?;

    // Determine the dependencies from the archive
    let index: Index = tokio::task::block_in_place(|| {
        std::fs::File::open(&archive_path.join("info/index.json"))
            .map_err(anyhow::Error::new)
            .and_then(|f| {
                serde_json::from_reader(std::io::BufReader::new(f)).map_err(anyhow::Error::new)
            })
    })
    .with_context(|| format!("unable to read info/index.json"))?;
    let dependencies = index.depends;

    Ok(())
}

/// Ensures that the package with the given `package_file_name` exists in the directory specified by
/// `package_cache_path`. If the archive already exists it is validated. If it doesnt exist or is
/// not valid, the archive is re-downloaded.
async fn ensure_package_archive(
    package_file_name: &str,
    url: &Url,
    client: LazyClient,
    package_cache_path: &Path,
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
                log::trace!("contents of `{}` succesfully validated", &package_file_name);
                return Ok(destination);
            }
            Err(e) => log::warn!("contents of `{}` is invalid: {e}", &package_file_name),
        }
    }

    // Clean the previous directory to ensure no files remain
    if destination.is_dir() {
        fs::remove_dir_all(&destination).await?;
    } else if destination.is_file() {
        fs::remove_file(&destination).await?;
    }

    // Download the package
    let client = (**client).clone();
    fetch_and_extract(client, url.clone(), format, destination.clone())
        .await
        .with_context(|| format!("failed to download and extract {}", &package_file_name))?;

    Ok(destination)
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
    let byte_stream = StreamReader::new(bytes.map_err(|e| io::Error::new(io::ErrorKind::Other, e)));

    // Extract the contents of the package
    format.unpack(byte_stream, &destination).await?;

    // Report success
    log::debug!("extracted {package_url} to {}", destination.display());

    Ok(())
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

    #[error("cannot read link '{0}': {1}")]
    NotALink(String, #[source] io::Error),

    #[error("`{0}` size mismatch: exptected {1}, got {2}")]
    FileSizeMismatch(String, u64, u64),

    #[error("error computing file digest")]
    DigestError(#[source] anyhow::Error),

    #[error("`{0}` digest mismatch, expected {1}, got {2}")]
    DigestMismatch(String, String, String),

    #[error("{0}")]
    Unknown(#[source] anyhow::Error),
}

/// Computes the sha256 digest for the file at the given path.
async fn compute_sha256_digest(path: &Path) -> anyhow::Result<String> {
    let file = tokio::fs::File::open(path)
        .await
        .with_context(|| format!("unable to open {}", path.display()))?;

    let mut ctx = Sha256::new();
    let mut frames = FramedRead::new(BufReader::new(file), BytesCodec::new());
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

    if entry.path_type == "softlink" {
        // We don't care about softlinks for validation, they are created from other files anyway
        return Ok(());
    }

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

    // TODO: Enable or disable?
    // let digest = compute_sha256_digest(&entry_path)
    //     .await
    //     .map_err(|e| ValidationError::DigestError(e))?;
    // if entry.sha256 != digest {
    //     return Err(ValidationError::DigestMismatch(
    //         entry.relative_path.display().to_string(),
    //         entry.sha256.clone(),
    //         digest,
    //     ));
    // }

    Ok(())
}

/// Validates extracted package contents
async fn validate_package(archive_path: &PathBuf) -> Result<(), ValidationError> {
    // Read the contents of the paths.json file
    let paths: Paths = tokio::task::block_in_place(|| {
        std::fs::File::open(&archive_path.join("info/paths.json"))
            .map_err(ValidationError::CouldNotOpenPathsJson)
            .and_then(|f| {
                serde_json::from_reader(std::io::BufReader::new(f))
                    .map_err(ValidationError::CouldNotDeserializePaths)
            })
    })?;

    // Iterate over all files and determine whether they are valid
    for entry in paths.paths.iter() {
        validate_package_entry(archive_path.to_path_buf(), entry.clone()).await?;
    }

    Ok(())
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
enum PackageArchiveFormat {
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
