use std::sync::Arc;

use atrium::Context;
use atrium_core::Config;
use atrium_core::FoyerEngine;
use clap::Parser;
use error_stack::Result;
use error_stack::ResultExt;
use thiserror::Error;

#[derive(clap::Parser)]
struct Command {
    #[clap(short, long, help = "Path to config file")]
    config: String,
}

#[derive(Debug, Error)]
#[error("{0}")]
struct Error(String);

#[tokio::main]
async fn main() -> Result<(), Error> {
    let make_error = || Error("failed to start server".to_string());

    logforth::builder()
        .dispatch(|d| {
            d.filter(log::LevelFilter::Debug)
                .append(logforth::append::Stdout::default())
        })
        .apply();

    let cmd = Command::parse();
    let config = toml::from_str::<Config>(&std::fs::read_to_string(&cmd.config).unwrap()).unwrap();

    let engine = FoyerEngine::try_new(&config.path, config.memory_capacity, config.disk_capacity)
        .await
        .change_context_lazy(make_error)?;

    let ctx = Arc::new(Context { engine });

    log::info!("config: {config:#?}");

    let server = atrium::server::start_server(&config, ctx)
        .await
        .inspect_err(|err| {
            log::error!("server stopped: {}", err);
        })
        .change_context_lazy(make_error)?;
    server.await_shutdown().await;
    Ok(())
}
