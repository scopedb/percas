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

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use clap::ValueHint;
use exn::Result;
use exn::ResultExt;
use mixtrics::registry::opentelemetry_0_31::OpenTelemetryMetricsRegistry;
use percas_core::Config;
use percas_core::FoyerEngine;
use percas_core::Runtime;
use percas_core::make_runtime;
use percas_core::num_cpus;
use percas_metrics::GlobalMetrics;
use percas_server::PercasContext;
use percas_server::server::make_acceptor_and_advertise_addr;
use percas_server::telemetry;
use uuid::Uuid;

use crate::Error;
use crate::config::LoadConfigResult;
use crate::config::load_config;

#[derive(Debug, clap::Parser)]
pub struct CommandStart {
    #[clap(short, long, help = "Path to config file", value_hint = ValueHint::FilePath)]
    config_file: PathBuf,
    /// The service name used for telemetry; default to 'scopedb'.
    #[clap(short = 's', long = "service-name")]
    service_name: Option<String>,
}

impl CommandStart {
    pub fn run(self) -> Result<(), Error> {
        let LoadConfigResult { config, warnings } = load_config(self.config_file)?;

        let node_id = Uuid::now_v7();
        let service_name = self.service_name.unwrap_or("percas".to_string()).leak();

        let telemetry_runtime = make_telemetry_runtime();
        let mut drop_guards = telemetry::init(
            &telemetry_runtime,
            service_name,
            node_id,
            config.telemetry.clone(),
        );
        drop_guards.push(Box::new(telemetry_runtime));
        for warning in warnings {
            log::warn!("{warning}");
        }
        log::info!("Percas is starting with loaded config: {config:#?}");

        let server_runtime = make_server_runtime();
        let gossip_runtime = make_gossip_runtime();
        server_runtime.block_on(run_server(
            &server_runtime,
            &gossip_runtime,
            node_id,
            config,
        ))
    }
}

fn make_telemetry_runtime() -> Runtime {
    make_runtime("telemetry_runtime", "telemetry_thread", 1)
}

fn make_server_runtime() -> Runtime {
    let parallelism = num_cpus().get();
    make_runtime("server_runtime", "server_thread", parallelism)
}

fn make_gossip_runtime() -> Runtime {
    make_runtime("gossip_runtime", "gossip_thread", 1)
}

async fn run_server(
    server_rt: &Runtime,
    gossip_rt: &Runtime,
    node_id: Uuid,
    config: Config,
) -> Result<(), Error> {
    let make_error = || Error("failed to start server".to_string());

    let server_config = config.server;
    fs::create_dir_all(&server_config.dir).or_raise(|| {
        Error(format!(
            "failed to create data dir: {}",
            server_config.dir.display()
        ))
    })?;

    let engine = FoyerEngine::try_new(
        config.storage.data_dir.as_path(),
        config.storage.memory_capacity,
        config.storage.disk_capacity,
        config.storage.disk_throttle,
        Some(OpenTelemetryMetricsRegistry::new(
            GlobalMetrics::get().meter.clone(),
        )),
    )
    .await
    .or_raise(make_error)?;
    let ctx = Arc::new(PercasContext::new(engine));

    let (shutdown_tx, shutdown_rx) = mea::shutdown::new_pair();

    let (data_acceptor, advertise_data_addr) = make_acceptor_and_advertise_addr(
        server_config.listen_data_addr,
        server_config.advertise_data_addr,
    )
    .await
    .or_raise(make_error)?;

    let (ctrl_acceptor, advertise_ctrl_addr) = make_acceptor_and_advertise_addr(
        server_config.listen_ctrl_addr,
        server_config.advertise_ctrl_addr,
    )
    .await
    .or_raise(make_error)?;

    let (gossip_state, gossip_futs) = percas_server::server::start_gossip(
        gossip_rt,
        shutdown_rx.clone(),
        server_config,
        node_id,
        ctrl_acceptor,
        advertise_data_addr,
        advertise_ctrl_addr,
    )
    .await
    .or_raise(make_error)?;

    let server = percas_server::server::start_server(
        server_rt,
        shutdown_rx,
        ctx,
        data_acceptor,
        advertise_data_addr,
        advertise_ctrl_addr,
        gossip_state,
        gossip_futs,
    )
    .await
    .or_raise(|| Error("A fatal error has occurred in server process.".to_string()))?;

    ctrlc::set_handler(move || shutdown_tx.shutdown())
        .or_raise(|| Error("failed to setup ctrl-c signal handle".to_string()))?;

    server.await_shutdown().await;
    Ok(())
}
