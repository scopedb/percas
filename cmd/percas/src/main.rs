// Copyright 2025 ScopeDB <contact@scopedb.io>
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use clap::Parser;
use clap::ValueHint;
use error_stack::Result;
use error_stack::ResultExt;
use fastimer::schedule::SimpleActionExt;
use percas_core::Config;
use percas_core::FoyerEngine;
use percas_core::num_cpus;
use percas_server::PercasContext;
use percas_server::runtime::Runtime;
use percas_server::runtime::make_runtime;
use percas_server::runtime::timer;
use percas_server::scheduled::ReportMetricsAction;
use percas_server::telemetry;
use thiserror::Error;

#[derive(clap::Parser)]
struct Command {
    #[clap(short, long, help = "Path to config file", value_hint = ValueHint::FilePath)]
    config_file: PathBuf,
}

#[derive(Debug, Error)]
#[error("{0}")]
struct Error(String);

fn make_telemetry_runtime() -> Runtime {
    make_runtime("telemetry_runtime", "telemetry_thread", 1)
}

fn make_server_runtime() -> Runtime {
    let parallelism = num_cpus().get();
    make_runtime("server_runtime", "server_thread", parallelism)
}

async fn run_server(rt: &Runtime, config: Config) -> Result<(), Error> {
    let make_error = || Error("failed to start server".to_string());

    let engine = FoyerEngine::try_new(
        &config.storage.data_dir,
        config.storage.memory_capacity,
        config.storage.disk_capacity,
    )
    .await
    .change_context_lazy(make_error)?;

    let ctx = Arc::new(PercasContext { engine });

    // Scheduled actions
    ReportMetricsAction::new(ctx.clone()).schedule_with_fixed_delay(
        rt,
        timer(),
        None,
        Duration::from_secs(60),
    );

    log::info!("config: {config:#?}");

    let server = percas_server::server::start_server(&config.server, ctx)
        .await
        .inspect_err(|err| {
            log::error!("server stopped: {}", err);
        })
        .change_context_lazy(make_error)?;
    server.await_shutdown().await;
    Ok(())
}

fn main() -> Result<(), Error> {
    let cmd = Command::parse();

    let file = cmd.config_file;
    let config = std::fs::read_to_string(&file)
        .change_context_lazy(|| Error(format!("failed to read config file: {}", file.display())))?;
    let config = toml::from_str::<Config>(config.as_str())
        .change_context_lazy(|| Error("failed to parse config content".to_string()))?;

    let telemetry_runtime = make_telemetry_runtime();
    let mut drop_guards = telemetry::init(&telemetry_runtime, "percas", config.telemetry.clone());
    drop_guards.push(Box::new(telemetry_runtime));

    let server_runtime = make_server_runtime();
    server_runtime.block_on(run_server(&server_runtime, config))
}
