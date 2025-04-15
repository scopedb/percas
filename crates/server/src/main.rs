use std::sync::Arc;
use std::time::Duration;

use atrium::AtriumContext;
use atrium::scheduled::ReportMetricsAction;
use atrium::scheduled::Timer;
use atrium::scheduled::TokioSpawn;
use atrium::telemetry;
use atrium_core::Config;
use atrium_core::FoyerEngine;
use clap::Parser;
use error_stack::Result;
use error_stack::ResultExt;
use fastimer::schedule::SimpleActionExt;
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

    let cmd = Command::parse();
    let config = toml::from_str::<Config>(&std::fs::read_to_string(&cmd.config).unwrap()).unwrap();

    let _drop_guards = telemetry::init("atrium", config.telemetry.clone());

    let engine = FoyerEngine::try_new(
        &config.storage.data_dir,
        config.storage.memory_capacity,
        config.storage.disk_capacity,
    )
    .await
    .change_context_lazy(make_error)?;

    let ctx = Arc::new(AtriumContext { engine });

    // Scheduled actions
    ReportMetricsAction::new(ctx.clone()).schedule_with_fixed_delay(
        &TokioSpawn,
        Timer,
        None,
        Duration::from_secs(60),
    );

    log::info!("config: {config:#?}");

    let server = atrium::server::start_server(&config.server, ctx)
        .await
        .inspect_err(|err| {
            log::error!("server stopped: {}", err);
        })
        .change_context_lazy(make_error)?;
    server.await_shutdown().await;
    Ok(())
}
