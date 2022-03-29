use crate::{ChannelConfig, MatchSpec, Version};
use serde::{Deserialize, Deserializer};

/// Parses a version from a string
pub(crate) fn version_from_str<'de, D>(deserializer: D) -> Result<Version, D::Error>
where
    D: serde::Deserializer<'de>,
{
    String::deserialize(deserializer)?
        .parse()
        .map_err(serde::de::Error::custom)
}

macro_rules! regex {
    ($re:literal $(,)?) => {{
        static RE: once_cell::sync::OnceCell<regex::Regex> = once_cell::sync::OnceCell::new();
        RE.get_or_init(|| regex::Regex::new($re).unwrap())
    }};
}
pub use regex;
use serde::de::Error;
use serde_with::DeserializeAs;

pub struct MatchSpecStr;

impl<'de> DeserializeAs<'de, MatchSpec> for MatchSpecStr {
    fn deserialize_as<D>(deserializer: D) -> Result<MatchSpec, D::Error>
    where
        D: Deserializer<'de>,
    {
        let str = String::deserialize(deserializer)?;
        MatchSpec::from_str(&str, &ChannelConfig::default()).map_err(serde::de::Error::custom)
    }
}
