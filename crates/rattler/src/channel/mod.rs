mod fetch;

use super::{ParsePlatformError, Platform};
use itertools::Itertools;
use serde::{Deserialize, Serialize};
use smallvec::{smallvec, SmallVec};
use std::path::PathBuf;
use std::str::FromStr;
use thiserror::Error;
use url::Url;

pub use fetch::{FetchRepoDataError, FetchRepoDataProgress};

/// Describes channel configuration which influences how channel strings are interpreted.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelConfig {
    /// A url to prefix to channel names that don't start with a Url. Usually this Url refers to
    /// the `https://conda.anaconda.org` server but users are free to change this. This allows
    /// naming channels just by their name instead of their entire Url (e.g. "conda-forge" actually
    /// refers to "https://conda.anaconda.org/conda-forge").
    channel_alias: Url,
}

impl Default for ChannelConfig {
    fn default() -> Self {
        ChannelConfig {
            channel_alias: Url::from_str("https://conda.anaconda.org")
                .expect("could not parse default channel alias"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Eq, PartialEq, Hash)]
pub struct Channel {
    /// The platforms supported by this channel, or None if no explicit platforms have been
    /// specified.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub platforms: Option<SmallVec<[Platform; 2]>>,

    /// The Url scheme of the channel. Usually http, https or file.
    pub scheme: String,

    /// The server and path part of the Url.
    pub location: String,

    /// The name of the channel
    pub name: String,
}

impl Channel {
    /// Parses a [`Channel`] from a string and a channel configuration.
    pub fn from_str(
        str: impl AsRef<str>,
        config: &ChannelConfig,
    ) -> Result<Self, ParseChannelError> {
        let str = str.as_ref();
        let (platforms, channel) = parse_platforms(str)?;

        let channel = if parse_scheme(channel).is_some() {
            let url = Url::parse(channel)?;
            Channel::from_url(&url, platforms)
        } else if is_path(channel) {
            let path = PathBuf::from(channel);
            let url =
                Url::from_file_path(&path).map_err(|_| ParseChannelError::InvalidPath(path))?;
            Channel::from_url(&url, platforms)
        } else {
            Channel::from_name(channel, platforms, config)
        };

        Ok(channel)
    }

    /// Constructs a new [`Channel`] from a `Url` and associated platforms.
    pub fn from_url(url: &Url, mut platforms: Option<SmallVec<[Platform; 2]>>) -> Self {
        let SplitCondaUrl {
            mut path,
            host,
            port,
            ..
        } = SplitCondaUrl::from(url);

        // Case 1: No path give, channel name is ""
        if path.is_empty() {
            return Self {
                platforms,
                scheme: url.scheme().to_owned(),
                location: url.host_str().unwrap_or("").to_owned(),
                name: String::from(""),
            };
        }

        if let Some(last_path) = path.last() {
            match Platform::from_str(*last_path) {
                Ok(platform) => {
                    // Ends in a platform string, add it to the platforms
                    platforms = Some(
                        platforms
                            .map(|mut platforms| {
                                platforms.push(platform);
                                platforms
                            })
                            .unwrap_or_else(|| smallvec![platform]),
                    );
                    path.pop();
                }
                Err(_) => {
                    // Not a platform string, ignore
                }
            }
        }

        // Case 2: migrated_custom_channels
        // Case 3: migrated_channel_aliases
        // Case 4: custom_channels matches
        // Case 5: channel_alias match

        if let Some(host) = host {
            // Case 7: Fallback
            let location = if let Some(port) = port {
                format!("{}:{}", host, port)
            } else {
                host.to_owned()
            };
            Self {
                platforms,
                scheme: url.scheme().to_owned(),
                location,
                name: path.join("/"),
            }
        } else {
            // Case 6: non-otherwise-specified file://-type urls
            let mut path_iter = path.into_iter().peekable();
            let (location, name) = if let Some(location) = path_iter.next() {
                if path_iter.peek().is_some() {
                    (location, path_iter.join("/"))
                } else {
                    ("/", location.to_owned())
                }
            } else {
                unreachable!("should be unreachable because we check if the path is not empty")
            };
            Self {
                platforms,
                scheme: String::from("file"),
                location: location.to_owned(),
                name: name.to_owned(),
            }
        }
    }

    /// Construct a channel from a name, platform and configuration.
    pub fn from_name(
        name: &str,
        platforms: Option<impl Into<SmallVec<[Platform; 2]>>>,
        config: &ChannelConfig,
    ) -> Self {
        // TODO: custom channels
        Self {
            platforms: platforms.map(Into::into),
            scheme: config.channel_alias.scheme().to_owned(),
            location: format!(
                "{}/{}",
                config.channel_alias.host_str().unwrap_or("/").to_owned(),
                config.channel_alias.path()
            )
            .trim_end_matches('/')
            .to_owned(),
            name: name.to_owned(),
        }
    }

    /// Returns the base Url of the channel. This does not include the platform part.
    pub fn base_url(&self) -> Url {
        Url::from_str(&format!(
            "{}://{}/{}",
            self.scheme, self.location, self.name
        ))
        .expect("could not construct base_url for channel")
    }

    /// Returns the Urls for the given platform
    pub fn platform_url(&self, platform: Platform) -> Url {
        let mut base_url = self.base_url();
        base_url.set_path(&format!("{}/{}/", base_url.path(), platform.as_str()));
        base_url
    }

    /// Returns the Urls for all the supported platforms of this package.
    pub fn platforms_url(&self) -> Vec<(Platform, Url)> {
        self.platforms_or_default()
            .iter()
            .map(|&platform| (platform, self.platform_url(platform)))
            .collect()
    }

    /// Returns the platforms explicitly mentioned in the channel or the default platforms of the
    /// current system.
    pub fn platforms_or_default(&self) -> &[Platform] {
        self.platforms
            .as_ref()
            .map(|platforms| platforms.as_ref())
            .unwrap_or_else(|| default_platforms())
    }

    /// Returns the canonical name of the channel
    pub fn canonical_name(&self) -> String {
        format!("{}://{}/{}", self.scheme, self.location, self.name)
    }
}

struct SplitCondaUrl<'a> {
    scheme: &'a str,
    host: Option<&'a str>,
    port: Option<u16>,
    token: Option<&'a str>,
    path: Vec<&'a str>,
    filename: Option<&'a str>,
}

impl<'a> From<&'a Url> for SplitCondaUrl<'a> {
    fn from(url: &'a Url) -> Self {
        let mut path_segments = url
            .path_segments()
            .map(|segments| segments.collect_vec())
            .unwrap_or_default();

        // Remove the token segments
        let mut token = None;
        let mut segment_iter = path_segments.iter().enumerate().peekable();
        while let Some((idx, segment)) = segment_iter.next() {
            if *segment == "t" {
                if let Some((_, t)) = segment_iter.peek() {
                    token = Some(**t);
                    path_segments.remove(idx);
                    path_segments.remove(idx);
                    break;
                }
            }
        }

        // If the path has a package archive extension, remove it from the path
        let mut filename = None;
        if let Some(last) = path_segments.last() {
            if last.ends_with(".conda") || last.ends_with(".tar.bz2") {
                filename = path_segments.pop();
            }
        }

        Self {
            scheme: url.scheme(),
            host: url.host_str(),
            port: url.port(),
            token,
            path: path_segments,
            filename,
        }
    }
}

#[derive(Debug, Error, Clone, Eq, PartialEq)]
pub enum ParseChannelError {
    #[error("could not parse the platforms")]
    ParsePlatformError(#[source] ParsePlatformError),

    #[error("could not parse url")]
    ParseUrlError(#[source] url::ParseError),

    #[error("invalid path '{0}")]
    InvalidPath(PathBuf),
}

impl From<ParsePlatformError> for ParseChannelError {
    fn from(err: ParsePlatformError) -> Self {
        ParseChannelError::ParsePlatformError(err)
    }
}

impl From<url::ParseError> for ParseChannelError {
    fn from(err: url::ParseError) -> Self {
        ParseChannelError::ParseUrlError(err)
    }
}

/// Extract the platforms from the given human readable channel.
fn parse_platforms(
    channel: &str,
) -> Result<(Option<SmallVec<[Platform; 2]>>, &str), ParsePlatformError> {
    if channel.rfind(']').is_some() {
        if let Some(start_platform_idx) = channel.find('[') {
            let platform_part = &channel[start_platform_idx + 1..channel.len() - 1];
            let platforms = platform_part
                .split(',')
                .map(str::trim)
                .map(FromStr::from_str)
                .collect::<Result<_, _>>()?;
            return Ok((Some(platforms), &channel[0..start_platform_idx]));
        }
    }

    Ok((None, channel))
}

/// Returns the default platforms. These are based on the platform this binary was build for as well
/// as platform agnostic platforms.
pub const fn default_platforms() -> &'static [Platform] {
    const CURRENT_PLATFORMS: [Platform; 2] = [Platform::current(), Platform::NoArch];
    return &CURRENT_PLATFORMS;
}

/// Parses the schema part of the human-readable channel. Returns the scheme part if it exists.
fn parse_scheme(channel: &str) -> Option<&str> {
    let scheme_end = channel.find("://")?;

    // Scheme part is too long
    if scheme_end > 11 {
        return None;
    }

    let scheme_part = &channel[0..scheme_end];
    let mut scheme_chars = scheme_part.chars();

    // First character must be alphabetic
    if scheme_chars.next().map(char::is_alphabetic) != Some(true) {
        return None;
    }

    // The rest must be alpha-numeric
    if scheme_chars.all(char::is_alphanumeric) {
        Some(scheme_part)
    } else {
        None
    }
}

/// Returns true if the specified string is considered to be a path
fn is_path(path: &str) -> bool {
    let re = regex::Regex::new(r"(\./|\.\.|~|/|[a-zA-Z]:[/\\]|\\\\|//)").unwrap();
    re.is_match(path)
}

#[cfg(test)]
mod tests {
    use super::{parse_scheme, Channel, ChannelConfig, Platform};
    use smallvec::smallvec;

    #[test]
    fn test_parse_scheme() {
        assert_eq!(parse_scheme("https://google.com"), Some("https"));
        assert_eq!(parse_scheme("http://google.com"), Some("http"));
        assert_eq!(parse_scheme("google.com"), None);
        assert_eq!(parse_scheme(""), None);
    }

    #[test]
    fn parse_by_name() {
        let config = ChannelConfig::default();

        let channel = Channel::from_str("conda-forge", &config).unwrap();
        assert_eq!(channel.scheme, "https");
        assert_eq!(channel.location, "conda.anaconda.org");
        assert_eq!(channel.name, "conda-forge");
        assert_eq!(channel.platforms, None);
    }

    #[test]
    fn parse_platform() {
        let platform = Platform::Linux32;
        let config = ChannelConfig::default();

        let channel = Channel::from_str(
            format!("https://conda.anaconda.com/conda-forge[{platform}]"),
            &config,
        )
        .unwrap();
        assert_eq!(channel.scheme, "https");
        assert_eq!(channel.location, "conda.anaconda.com");
        assert_eq!(channel.name, "conda-forge");
        assert_eq!(channel.platforms, Some(smallvec![platform]));

        let channel = Channel::from_str(
            format!("https://repo.anaconda.com/pkgs/main[{platform}]"),
            &config,
        )
        .unwrap();
        assert_eq!(channel.scheme, "https");
        assert_eq!(channel.location, "repo.anaconda.com");
        assert_eq!(channel.name, "pkgs/main");
        assert_eq!(channel.platforms, Some(smallvec![platform]));
    }
}
