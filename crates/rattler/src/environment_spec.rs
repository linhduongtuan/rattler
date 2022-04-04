use crate::{Channel, ParseVersionError, Version};
use futures::{future, StreamExt, TryStreamExt};
use std::collections::HashSet;
use std::convert::TryFrom;
use std::path::Path;
use std::str::FromStr;
use thiserror::Error;
use tokio::fs::File;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::pin;
use tokio_stream::wrappers::LinesStream;
use url::Url;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnvironmentSpec {
    Explicit(ExplicitEnvironment),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExplicitEnvironment {
    pub specs: HashSet<ExplicitPackageSpec>,
}

impl EnvironmentSpec {
    pub async fn from_file(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let path = path.as_ref();
        match path.extension().and_then(|s| s.to_str()) {
            Some("txt") => Ok(Self::Explicit(ExplicitEnvironment::from_file(path).await?)),
            _ => anyhow::bail!("unknown extension"),
        }
    }
}

impl ExplicitEnvironment {
    pub async fn from_file(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let file = File::open(path).await?;
        let lines = LinesStream::new(BufReader::new(file).lines())
            .try_filter(|line| future::ready(!line.starts_with('#')))
            .map_err(|err| anyhow::Error::from(err));

        pin!(lines);

        // The first line must be the explicit string
        let first_line = lines.next().await.ok_or_else(|| {
            anyhow::anyhow!("invalid explicit environment spec: the specified file is empty")
        })??;
        match first_line.as_str() {
            "@EXPLICIT" => {}
            _ => anyhow::bail!("invalid explicit environment spec: the specified file does not start with @EXPLICIT"),
        };

        // Followed by explicit package specificiations
        let specs = lines
            .and_then(|line| async move { Ok(line.parse()?) })
            .try_collect()
            .await?;

        Ok(ExplicitEnvironment { specs })
    }
}

#[derive(Debug, Clone, Error)]
pub enum ParseExplicitSpecError {
    #[error("cannot parse url: {0}")]
    UrlParseError(#[from] url::ParseError),

    #[error("url does not refer to a package archive")]
    NotAPackageArchive,

    #[error("invalid package archive name: '{0}'")]
    InvalidPackageArchiveName(String),

    #[error("invalid version")]
    InvalidVersion(#[from] ParseVersionError),
}

#[derive(Clone, Eq, PartialEq, Hash, Debug)]
pub struct ExplicitPackageSpec {
    pub url: Url,
    pub name: String,
    pub version: Version,
    pub channel: Channel,
    pub build_string: String,
}

impl FromStr for ExplicitPackageSpec {
    type Err = ParseExplicitSpecError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        ExplicitPackageSpec::try_from(Url::parse(s)?)
    }
}

impl TryFrom<Url> for ExplicitPackageSpec {
    type Error = ParseExplicitSpecError;

    fn try_from(url: Url) -> Result<Self, Self::Error> {
        // Parse a channel part from the URL
        let channel = Channel::from_url(&url, None);

        // Get the package archive name from the URL
        // TODO: Maybe extract this into a function?
        let package_archive_name =
            if let Some(last_segment) = url.path_segments().and_then(|s| s.last()) {
                if let Some(name) = last_segment.strip_suffix(".tar.bz2") {
                    name
                } else if let Some(name) = last_segment.strip_suffix(".conda") {
                    name
                } else {
                    return Err(ParseExplicitSpecError::NotAPackageArchive);
                }
            } else {
                return Err(ParseExplicitSpecError::NotAPackageArchive);
            };

        // Extract information of the package from the filename
        let (name, version, build) = match package_archive_name
            .rsplit_once('-')
            .map(|(rest, build)| (rest.rsplit_once('-'), build))
        {
            Some((Some((name, version)), build)) => (name, version, build),
            _ => {
                return Err(ParseExplicitSpecError::InvalidPackageArchiveName(
                    package_archive_name.to_owned(),
                ))
            }
        };

        // Parse the version
        let version = Version::from_str(version)?;

        Ok(Self {
            name: name.to_owned(),
            build_string: build.to_owned(),
            version,
            channel,
            url,
        })
    }
}
