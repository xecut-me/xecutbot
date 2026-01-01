use anyhow::Result;
use clap::Parser;
use xecut_bot::{backend::BackendImpl, selfupdate};

#[derive(Parser, Debug)]
struct Cli {
    #[arg(short = 'c', long = "config", default_value = "xecut_bot")]
    config: Vec<std::path::PathBuf>,
}

#[tokio::main]
async fn main() -> Result<()> {
    tokio::spawn(async move {
        env_logger::init();
        let args = Cli::parse();
        let backend = BackendImpl::new(args.config).await?;
        let should_reexec = backend.run().await?;

        if should_reexec {
            selfupdate::reexec();
        }

        Ok(())
    })
    .await?
}
