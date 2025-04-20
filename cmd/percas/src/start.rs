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

use clap::ValueHint;
use error_stack::Result;
use error_stack::ResultExt;
use percas_core::Config;
use percas_core::FoyerEngine;
use percas_core::Runtime;
use percas_core::make_runtime;
use percas_core::num_cpus;
use percas_server::PercasContext;
use percas_server::telemetry;

use crate::Error;

#[derive(Debug, clap::Parser)]
pub struct CommandStart {
    #[clap(short, long, help = "Path to config file", value_hint = ValueHint::FilePath)]
    config_file: PathBuf,
}

impl CommandStart {
    pub fn run(self) -> Result<(), Error> {
        let file = self.config_file;
        let config = std::fs::read_to_string(&file).change_context_lazy(|| {
            Error(format!("failed to read config file: {}", file.display()))
        })?;
        let config = toml::from_str::<Config>(config.as_str())
            .change_context_lazy(|| Error("failed to parse config content".to_string()))?;

        let telemetry_runtime = make_telemetry_runtime();
        let mut drop_guards =
            telemetry::init(&telemetry_runtime, "percas", config.telemetry.clone());
        drop_guards.push(Box::new(telemetry_runtime));
        log::info!("Percas is starting with loaded config: {config:#?}");

        let server_runtime = make_server_runtime();
        server_runtime.block_on(run_server(&server_runtime, config))
    }
}

fn make_telemetry_runtime() -> Runtime {
    make_runtime("telemetry_runtime", "telemetry_thread", 1)
}

fn make_server_runtime() -> Runtime {
    let parallelism = num_cpus().get();
    make_runtime("server_runtime", "server_thread", parallelism)
}

async fn run_server(rt: &Runtime, config: Config) -> Result<(), Error> {
    let engine = FoyerEngine::try_new(
        &config.storage.data_dir,
        config.storage.memory_capacity,
        config.storage.disk_capacity,
    )
    .await
    .change_context_lazy(|| Error("failed to start server".to_string()))?;

    let ctx = Arc::new(PercasContext { engine });
    let (server, shutdown_tx) = percas_server::server::start_server(rt, &config.server, ctx)
        .await
        .change_context_lazy(|| {
            Error("A fatal error has occurred in server process.".to_string())
        })?;

    ctrlc::set_handler(move || shutdown_tx.shutdown())
        .change_context_lazy(|| Error("failed to setup ctrl-c signal handle".to_string()))?;

    server.await_shutdown().await;
    Ok(())
}
