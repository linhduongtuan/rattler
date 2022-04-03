use rattler::install::InstallSpec;
use rattler::{install_prefix, EnvironmentSpec};
use std::env::current_dir;
use std::path::PathBuf;
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
pub struct Opt {
    #[structopt(required = true)]
    environment: PathBuf,
}

pub async fn install(opt: Opt) -> anyhow::Result<()> {
    let env = EnvironmentSpec::from_file(&opt.environment).await?;

    let explicit_environment = match env {
        EnvironmentSpec::Explicit(env) => env,
    };

    let prefix_dir = current_dir()?.join("env");

    let cache_dir = dirs::cache_dir()
        .ok_or_else(|| anyhow::anyhow!("could not determine application cache dir"))?
        .join("pkgs");
    log::debug!("packages cache dir: {}", cache_dir.display());

    install_prefix(
        explicit_environment
            .specs
            .into_iter()
            .map(|spec| InstallSpec {
                name: spec.name,
                url: spec.url,
            }),
        &prefix_dir,
        cache_dir,
    )
    .await?;

    Ok(())
}
