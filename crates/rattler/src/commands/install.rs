use rattler::EnvironmentSpec;
use std::path::PathBuf;
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
pub struct Opt {
    #[structopt(required = true)]
    environment: PathBuf,
}

pub async fn install(opt: Opt) -> anyhow::Result<()> {
    let env = EnvironmentSpec::from_file(&opt.environment).await?;

    Ok(())
}
