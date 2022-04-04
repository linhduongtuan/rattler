mod link;
mod python;

use crate::install::python::PythonInfo;
use crate::package_archive::{Index, NoArchType, PackageArchiveFormat, PathEntry, Paths};
use anyhow::Context;
use futures::stream::FuturesUnordered;
use futures::{FutureExt, TryFutureExt, TryStreamExt};
use itertools::Itertools;
use once_cell::sync::Lazy;
use reqwest_middleware::{ClientBuilder, ClientWithMiddleware};
use reqwest_retry::policies::ExponentialBackoff;
use reqwest_retry::RetryTransientMiddleware;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use thiserror::Error;
use tokio::fs;
use tokio::io;
use tokio::io::BufReader;
use tokio::sync::watch;
use tokio_stream::StreamExt;
use tokio_util::codec::{BytesCodec, FramedRead};
use tokio_util::io::StreamReader;
use url::Url;

#[derive(Debug, Clone)]
pub struct InstallSpec {
    /// The name of the package
    pub name: String,

    /// The location where we can find the package archive.
    pub url: Url,
}

/// Constructs a `reqwest` client.
fn construct_client() -> ClientWithMiddleware {
    let retry_policy = ExponentialBackoff::builder().build_with_max_retries(3);
    ClientBuilder::new(reqwest::Client::new())
        .with(RetryTransientMiddleware::new_with_policy(retry_policy))
        .build()
}

type LazyClient = Arc<Lazy<ClientWithMiddleware>>;

#[derive(Clone, Debug)]
struct PythonLinkStatus {
    has_python: bool,
    sender: Arc<Mutex<watch::Sender<Option<Arc<PythonInfo>>>>>,
    receiver: watch::Receiver<Option<Arc<PythonInfo>>>,
}

#[derive(Clone, Debug, Error)]
enum PythonLinkStatusError {
    #[error("no package provides python")]
    NoPackageProvidesPython,
}

impl PythonLinkStatus {
    fn new(has_python: bool) -> Self {
        let (sender, receiver) = watch::channel(None);
        Self {
            has_python,
            sender: Arc::new(Mutex::new(sender)),
            receiver,
        }
    }

    async fn wait_for_info(&self) -> Result<Arc<PythonInfo>, PythonLinkStatusError> {
        if !self.has_python {
            return Err(PythonLinkStatusError::NoPackageProvidesPython);
        }

        let mut local = self.receiver.clone();
        loop {
            if let Some(value) = local.borrow().as_ref() {
                return Ok(value.clone());
            }
            local.changed().await.unwrap()
        }
    }

    fn set(&self, info: PythonInfo) {
        let lock = self.sender.lock().expect("lock is poisoned");
        let _ = lock.send(Some(Arc::new(info)));
    }
}

/// Installs the specified packages to the specified destination.
pub async fn install_prefix(
    packages: impl IntoIterator<Item = InstallSpec>,
    prefix: impl AsRef<Path>,
    package_cache_path: impl AsRef<Path>,
) -> anyhow::Result<()> {
    let prefix = prefix.as_ref().to_path_buf();
    let package_cache_path = package_cache_path.as_ref().to_path_buf();
    tokio::fs::create_dir_all(&package_cache_path).await?;

    let client: LazyClient = Arc::new(Lazy::new(construct_client));
    let packages = packages.into_iter().collect_vec();

    // Determine if a python package is installed. This is required to be able to do no arch python
    // package compilation.
    let has_python_package = packages.iter().find(|p| p.name == "python").is_some();
    let python_link_status = PythonLinkStatus::new(has_python_package);

    // Create tasks to download all packages
    let mut download_tasks = FuturesUnordered::new();
    for package in packages.iter() {
        let prefix = prefix.clone();
        let package_name = package.name.clone();
        let package_task = tokio::spawn(install_package(
            prefix,
            package_name.to_owned(),
            package.url.clone(),
            client.clone(),
            package_cache_path.clone(),
            python_link_status.clone(),
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

async fn install_package(
    prefix: PathBuf,
    package_name: String,
    url: Url,
    client: LazyClient,
    package_cache_path: PathBuf,
    python_link_state: PythonLinkStatus,
) -> anyhow::Result<()> {
    // Ensure that the content of the package is stored on disk.
    let archive_path = fetch_package_archive(&url, client, &package_cache_path).await?;

    // Read the contents of the index.json and paths.json files
    let index_future = {
        let index_archive_path = archive_path.clone();
        tokio::task::spawn_blocking(move || read_index_from_archive(&index_archive_path))
            .unwrap_or_else(|e| Err(e.into()))
    };
    let paths_future = {
        let index_archive_path = archive_path.clone();
        tokio::task::spawn_blocking(move || read_paths_from_archive(&index_archive_path))
            .unwrap_or_else(|e| Err(e.into()))
    };
    let (index, paths) = tokio::try_join!(index_future, paths_future)?;

    // Wait for python to complete before linking noarch packages
    let python_info = if matches!(index.noarch, Some(NoArchType::Python)) {
        Some(python_link_state.wait_for_info().await?)
    } else {
        None
    };

    // Install all files
    let mut link_tasks = FuturesUnordered::new();
    for entry in paths.paths.into_iter() {
        let archive_path = archive_path.clone();
        let prefix = prefix.clone();

        // Determine the source & destination path
        let source_path = archive_path.join(&entry.relative_path);
        let destination_path = if let Some(python_info) = python_info.as_ref() {
            prefix.join(
                python_info
                    .get_python_noarch_target_path(&entry.relative_path)
                    .as_ref(),
            )
        } else {
            prefix.join(&entry.relative_path)
        };

        // Spawn the actual file system operation on the rayon threadpool. This is a blocking
        // operation which performs much better when running in a rayon threadpool instead of in
        // the tokio threadpool.
        // TODO: Maybe in the future this might no longer be the case.
        let link_task = tokio_rayon::spawn(move || {
            log::trace!("linking {}", entry.relative_path.display());
            link::link_file(
                &prefix,
                &source_path,
                &destination_path,
                entry.prefix_placeholder.as_ref().map(String::as_str),
                entry.path_type,
                entry.file_mode,
                !entry.no_link,
            )
            .with_context(move || format!("error linking `{}`", entry.relative_path.display()))
        });
        link_tasks.push(link_task);
    }

    // Wait for all tasks to complete
    while let Some(link_task) = link_tasks.next().await {
        let _ = link_task?;
    }

    // If we just installed python, update the python information channel so other packages that
    // require python in their linking process can continue.
    if package_name == "python" {
        python_link_state.set(PythonInfo::from_version(&index.version)?);
    }

    log::info!("finished linking {}", &package_name);

    Ok(())
}

/// Reads the contents of the paths.json file from a package cache. Because parsing a json file is
/// blocking, this call is blocking.
fn read_paths_from_archive(archive_path: &Path) -> anyhow::Result<Paths> {
    std::fs::File::open(&archive_path.join("info/paths.json"))
        .map_err(anyhow::Error::new)
        .and_then(|f| {
            serde_json::from_reader(std::io::BufReader::new(f)).map_err(anyhow::Error::new)
        })
}

/// Reads the contents of the index.json file from a package cache. Because parsing a json file is
/// blocking, this call is blocking.
fn read_index_from_archive(archive_path: &Path) -> anyhow::Result<Index> {
    std::fs::File::open(&archive_path.join("info/index.json"))
        .map_err(anyhow::Error::new)
        .and_then(|f| {
            serde_json::from_reader(std::io::BufReader::new(f)).map_err(anyhow::Error::new)
        })
}

/// Ensures that the package with the given `package_file_name` exists in the directory specified by
/// `package_cache_path`. If the archive already exists it is validated. If it doesnt exist or is
/// not valid, the archive is re-downloaded.
async fn fetch_package_archive(
    url: &Url,
    client: LazyClient,
    package_cache_path: &Path,
) -> anyhow::Result<PathBuf> {
    let package_file_name = url
        .path_segments()
        .and_then(|segments| segments.last())
        .ok_or_else(|| {
            anyhow::anyhow!("could not determine package archive filename from url `{url}`")
        })?;

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

    let metadata = match tokio::join!(
        tokio::fs::metadata(&entry_path),
        tokio::fs::symlink_metadata(&entry_path)
    ) {
        (Err(e), Err(_)) => {
            return Err(ValidationError::FileMetaDataError(
                entry.relative_path.display().to_string(),
                e,
            ))
        }
        (_, Ok(_)) => {
            // TODO: Do something with this?
            return Ok(());
        }
        (Ok(metadata), _) => metadata,
    };

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
    let paths: Paths = {
        let archive_path = archive_path.clone();
        tokio::task::spawn_blocking(move || {
            std::fs::File::open(&archive_path.join("info/paths.json"))
                .map_err(ValidationError::CouldNotOpenPathsJson)
                .and_then(|f| {
                    serde_json::from_reader(std::io::BufReader::new(f))
                        .map_err(ValidationError::CouldNotDeserializePaths)
                })
        })
        .unwrap_or_else(|e| Err(ValidationError::Unknown(e.into())))
    }
    .await?;

    // Iterate over all files and determine whether they are valid
    for entry in paths.paths.iter() {
        validate_package_entry(archive_path.to_path_buf(), entry.clone()).await?;
    }

    Ok(())
}
