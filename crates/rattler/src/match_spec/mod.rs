use crate::{Channel, PackageRecord, VersionSpec};
use serde::Serialize;
use serde_with::{serde_as, skip_serializing_none, DisplayFromStr};
use std::fmt::{Debug, Display, Formatter};

mod parse;

pub use parse::ParseMatchSpecError;

/// A `MatchSpec` is, fundamentally, a query language for conda packages. Any of the fields that
/// comprise a [`PackageRecord`] can be used to compose a `MatchSpec`.
#[serde_as]
#[skip_serializing_none]
#[derive(Debug, Default, Clone, Serialize, Eq, PartialEq, Hash)]
pub struct MatchSpec {
    pub name: Option<String>,
    pub version: Option<VersionSpec>,
    #[serde_as(as = "Option<DisplayFromStr>")]
    pub build: Option<glob::Pattern>,
    pub build_number: Option<usize>,
    pub filename: Option<String>,
    pub channel: Option<Channel>,
    pub namespace: Option<String>,
}

impl Display for MatchSpec {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        if let Some(channel) = &self.channel {
            // TODO: namespace
            write!(f, "{}::", channel.canonical_name())?;
        }

        match &self.name {
            Some(name) => write!(f, "{}", name),
            None => write!(f, "*"),
        }
    }
}

impl MatchSpec {
    pub fn matches(&self, record: &PackageRecord) -> bool {
        if let Some(name) = self.name.as_ref() {
            if name != &record.name {
                return false;
            }
        }

        if let Some(spec) = self.version.as_ref() {
            if !spec.matches(&record.version) {
                return false;
            }
        }

        if let Some(build_string) = self.build.as_ref() {
            if !build_string.matches(&record.build) {
                return false;
            }
        }

        true
    }
}
