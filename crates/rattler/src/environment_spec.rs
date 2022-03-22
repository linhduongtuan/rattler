use futures::{future, StreamExt, TryStreamExt};
use std::collections::HashSet;
use std::ffi::OsStr;
use std::io::Error;
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
        let mut lines = LinesStream::new(BufReader::new(file).lines())
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
}

#[derive(Clone, Eq, PartialEq, Hash, Debug)]
pub struct ExplicitPackageSpec {
    url: Url,
}

impl FromStr for ExplicitPackageSpec {
    type Err = ParseExplicitSpecError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(ExplicitPackageSpec {
            url: Url::parse(s)?,
        })
    }
}

impl ExplicitPackageSpec {
    /// Returns the Url of this instance
    pub fn url(&self) -> &Url {
        &self.url
    }

    /// Returns the expected MD5 hash if specified
    pub fn md5(&self) -> Option<&str> {
        self.url.fragment()
    }
}

impl crate::install::Package for ExplicitPackageSpec {
    fn filename(&self) -> &str {
        self.url
            .path_segments()
            .into_iter()
            .flatten()
            .last()
            .expect("invalid url without path")
    }

    fn url(&self) -> &Url {
        &self.url
    }
}
