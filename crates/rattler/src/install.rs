use crate::ExplicitPackageSpec;
use futures::SinkExt;
use http_cache_reqwest::{Cache, CacheMode, HttpCache};
use once_cell::sync::{Lazy, OnceCell};
use reqwest_middleware::{ClientBuilder, ClientWithMiddleware};
use reqwest_retry::policies::ExponentialBackoff;
use reqwest_retry::RetryTransientMiddleware;
use std::collections::{HashMap, HashSet};
use std::ops::Deref;
use std::path::{Path, PathBuf};
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
    let client: Lazy<ClientWithMiddleware> = Lazy::new(construct_client);

    // For each package create a state so we can keep track of any information we figure out underway.
    let mut state: HashMap<String, PackageState> = packages
        .into_iter()
        .map(|package| (package.filename().to_owned(), package.into()))
        .collect();

    // Determine a topological ordering of all the packages

    Ok(())
}

/// Current state of a package to be installed.
struct PackageState {
    /// The filename of the package
    filename: String,

    /// The url to get the package from
    url: Url,

    /// The dependencies of this package
    dependencies: Option<HashSet<String>>,
}

impl<P: Package> From<P> for PackageState {
    fn from(package: P) -> Self {
        PackageState {
            filename: package.filename().to_owned(),
            url: package.url().clone(),
            dependencies: package
                .dependencies()
                .map(|deps| deps.into_iter().map(ToString::to_string).collect()),
        }
    }
}

impl PackageState {
    /// Returns the name of the directory that will hold the package content
    fn cache_dir_name(&self) -> &str {
        // TODO: support more
        self.filename
            .strip_suffix(".tar.bz2")
            .expect("can only deal with tar.bz2 packages")
    }

    async fn download_and_extract(
        &self,
        client: &LazyClient,
        package_cache_path: &Path,
    ) -> anyhow::Result<PathBuf> {
        // Create a client
        let client = client.deref().clone();

        // Determine where to store each package
        let package_dir = package_cache_path.join(self.cache_dir_name());

        // Start downloading the package
        let response = client
            .get(self.url.clone())
            .send()
            .await?
            .error_for_status()?;

        Ok(package_dir)
    }
}
