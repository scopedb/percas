use std::sync::Arc;
use std::time::Duration;

use atrium_core::Config;
use atrium_core::FoyerEngine;
use atrium_server::AtriumContext;
use atrium_server::scheduled::ReportMetricsAction;
use atrium_server::telemetry;
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

fn make_telemetry_runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()
        .expect("failed to create telemetry runtime")
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    let make_error = || Error("failed to start server".to_string());

    let cmd = Command::parse();
    let config = toml::from_str::<Config>(&std::fs::read_to_string(&cmd.config).unwrap()).unwrap();

    let telemetry_runtime = make_telemetry_runtime();
    let mut drop_guards = telemetry::init(&telemetry_runtime, "atrium", config.telemetry.clone());
    drop_guards.push(Box::new(telemetry_runtime));

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
        &fastimer_tokio::TokioSpawn::current(),
        fastimer_tokio::MakeTokioDelay,
        None,
        Duration::from_secs(60),
    );

    log::info!("config: {config:#?}");

    let server = atrium_server::server::start_server(&config.server, ctx)
        .await
        .inspect_err(|err| {
            log::error!("server stopped: {}", err);
        })
        .change_context_lazy(make_error)?;
    server.await_shutdown().await;
    Ok(())
}
