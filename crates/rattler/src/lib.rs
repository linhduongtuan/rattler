mod activated_command;
mod channel;
mod environment_spec;
mod gate;
pub mod install;
mod match_spec;
mod match_spec_constraints;
mod package_archive;
mod platform;
mod range;
mod repo_data;
mod solver;
pub(crate) mod utils;
mod version;
mod version_spec;

pub use gate::Gate;

pub use channel::{
    Channel, ChannelConfig, FetchRepoDataError, FetchRepoDataProgress, ParseChannelError,
};
pub use install::install_prefix;
pub use match_spec::MatchSpec;
pub use match_spec_constraints::MatchSpecConstraints;
pub use platform::{ParsePlatformError, Platform};
pub use repo_data::{ChannelInfo, PackageRecord, RepoData};
pub use solver::{PackageIndex, SolverIndex};
pub use version::{ParseVersionError, ParseVersionErrorKind, Version};
pub use version_spec::VersionSpec;

pub use environment_spec::{EnvironmentSpec, ExplicitPackageSpec};

use range::Range;
