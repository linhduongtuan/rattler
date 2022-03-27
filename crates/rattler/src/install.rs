use anyhow::Context;
use bytes::Bytes;
use futures::{future, Stream, TryStreamExt};
use itertools::Itertools;
use once_cell::sync::Lazy;
use reqwest_middleware::{ClientBuilder, ClientWithMiddleware};
use reqwest_retry::policies::ExponentialBackoff;
use reqwest_retry::RetryTransientMiddleware;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use tokio::fs;
use tokio::io;
use tokio_tar::Archive;
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

type LazyClient = Lazy<ClientWithMiddleware>;

/// Installs the specified packages to the specified destination.
pub async fn install_prefix<P: Package>(
    packages: impl IntoIterator<Item = P>,
    prefix: impl AsRef<Path>,
    package_cache_path: impl AsRef<Path>,
) -> anyhow::Result<()> {
    let package_cache_path = package_cache_path.as_ref().to_path_buf();
    tokio::fs::create_dir_all(&package_cache_path).await?;

    let client: Lazy<ClientWithMiddleware> = Lazy::new(construct_client);
    let packages = packages.into_iter().collect_vec();

    // Download all the package archives
    let _result: Vec<_> = future::try_join_all(packages.iter().map(|package| {
        let package_cache_path = package_cache_path.clone();
        let archive_file_name = package.filename().to_owned();
        let source_url = package.url().clone();
        let client = client.deref().clone();
        tokio::spawn(fetch_and_extract(
            client,
            archive_file_name,
            source_url,
            package_cache_path,
        ))
    }))
    .await?;

    // Determine a topological ordering of all the packages

    Ok(())
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
    package_file_name: String,
    package_url: Url,
    package_cache_directory: PathBuf,
) -> anyhow::Result<()> {
    let lower_case_package_name = package_file_name.to_lowercase();

    // Start downloading the package
    let response = client
        .get(package_url.clone())
        .send()
        .await?
        .error_for_status()?;

    // Construct stream of byte chunks from the download
    let bytes = response.bytes_stream();

    // Extract the contents of the package
    let destination = if let Some(package_name) = lower_case_package_name.strip_suffix(".tar.bz2") {
        let destination = package_cache_directory.join(package_name);
        create_clean_dir_all(&destination).await?;
        extract_tar_bz2(bytes, &destination.clone())
            .await
            .with_context(|| {
                format!(
                    "while unpacking tar.bz2 archive to {}",
                    destination.display()
                )
            })?;
        destination
    } else {
        anyhow::bail!("unknown package extension for `{}`", package_file_name);
    };

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
