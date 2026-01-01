use std::{env, os::unix::process::CommandExt as _};

use anyhow::Result;

pub async fn update() -> Result<()> {
    let update_path = env::var("XECUT_UPDATE").unwrap_or("./update.sh".to_owned());
    log::info!("Running update: {update_path}");
    if !tokio::process::Command::new(update_path)
        .status()
        .await
        .unwrap()
        .success()
    {
        anyhow::bail!("Failed to execute update script");
    }

    Ok(())
}

pub fn reexec() -> ! {
    let start_path: String = env::var("XECUT_START").unwrap_or("./run.sh".to_owned());
    log::info!("Starting: {start_path}");
    panic!("{:?}", std::process::Command::new(start_path).exec())
}
