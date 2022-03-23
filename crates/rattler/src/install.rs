use crate::ExplicitPackageSpec;
use async_compression::futures::bufread::BzDecoder;
use bytes::Bytes;
use futures::{future, SinkExt, Stream, StreamExt, TryFutureExt, TryStreamExt};
use http_cache_reqwest::{Cache, CacheMode, HttpCache};
use itertools::Itertools;
use once_cell::sync::{Lazy, OnceCell};
use reqwest_middleware::{ClientBuilder, ClientWithMiddleware};
use reqwest_retry::policies::ExponentialBackoff;
use reqwest_retry::RetryTransientMiddleware;
use std::collections::{HashMap, HashSet};
use std::ffi::OsStr;
use std::io::{ErrorKind, Write};
use std::ops::Deref;
use std::path::{Path, PathBuf};
use tempdir::TempDir;
use tokio::fs::File;
use tokio::io;
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};
use tokio_tar::Archive;
use tokio_util::compat::FuturesAsyncReadCompatExt;
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
    let package_cache_path = package_cache_path.as_ref();
    tokio::fs::create_dir_all(&package_cache_path).await?;

    let client: Lazy<ClientWithMiddleware> = Lazy::new(construct_client);
    let packages = packages.into_iter().collect_vec();

    // Download all the package archives
    future::try_join_all(packages.iter().map(|package| {
        let client = client.deref().clone();
        download(
            client,
            package.filename(),
            package.url(),
            package_cache_path,
        )
    }))
    .await?;

    // Determine a topological ordering of all the packages

    Ok(())
}

/// Modifies the given path to end in .tmp
fn build_temporary_file_path(path: impl AsRef<Path>) -> PathBuf {
    let path = path.as_ref();
    let new_extension = path
        .extension()
        .map(|ext| format!("{}.tmp", ext.to_string_lossy()))
        .unwrap_or_else(|| String::from("tmp"));
    path.with_extension(new_extension)
}

async fn stream_response(
    mut reader: impl Stream<Item = reqwest::Result<Bytes>> + Unpin,
    mut writer: impl AsyncWrite + Unpin,
) -> anyhow::Result<()> {
    while let Some(bytes) = reader.next().await {
        let bytes = bytes?;
        writer
            .write_all(bytes.as_ref())
            .await
            .map_err(|e| anyhow::anyhow!("write_all failed: {e}"))?;
    }
    Ok(())
}

/// Downloads the specified package to a package cache directory. This function always overwrites
/// whatever was there.
async fn download(
    client: ClientWithMiddleware,
    package_file_name: &str,
    package_url: &Url,
    package_cache_directory: &Path,
) -> anyhow::Result<Option<PathBuf>> {
    let package_identifier = if let Some(package_identifier) = package_file_name.strip_suffix(".tar.bz2") {
        package_identifier
    } else {
        return Ok(None)
    };

    // Determine the final location of the archive and delete it if it exists
    let archive_file_path = package_cache_directory.join(package_identifier);
    if archive_file_path.is_dir() {
        tokio::fs::remove_dir_all(&archive_file_path).await?;
    }

    // Start downloading the package
    let response = client
        .get(package_url.clone())
        .send()
        .await?
        .error_for_status()?;

    // Construct stream of bytes from the remote
    let bytes = response
        .bytes_stream()
        .map_err(|e| std::io::Error::new(ErrorKind::Other, e))
        .into_async_read();

    // Determine the archive location
    let tmp_dir = TempDir::new_in(&package_cache_directory, &format!(".{package_identifier}.tmp"))?;

    // Extract the contents of the archive while reading the stream
    Archive::new(BzDecoder::new(bytes).compat())
        .unpack(tmp_dir.path())
        .map_err(|e| anyhow::anyhow!("failed to extract: {e}"))
        .await?;

    // Rename the temporary directory to the final archive location
    tokio::fs::rename(tmp_dir, &archive_file_path).await?;

    log::debug!("extracted {package_file_name} to {}", archive_file_path.display());

    Ok(Some(archive_file_path))
}
