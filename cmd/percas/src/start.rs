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
use percas_cluster::GossipState;
use percas_cluster::NodeInfo;
use percas_cluster::Proxy;
use percas_core::Config;
use percas_core::FoyerEngine;
use percas_core::Runtime;
use percas_core::ServerConfig;
use percas_core::make_runtime;
use percas_core::node_file_path;
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

#[derive(Debug, Clone, PartialEq, Eq)]
enum ServerMode {
    Standalone,
    Cluster,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FlattenConfig {
    mode: ServerMode,
    dir: PathBuf,
    listen_addr: String,
    advertise_addr: Option<String>,
    listen_peer_addr: Option<String>,
    advertise_peer_addr: Option<String>,
    initial_peer_addrs: Option<Vec<String>>,
}

impl From<&ServerConfig> for FlattenConfig {
    fn from(config: &ServerConfig) -> Self {
        match config {
            ServerConfig::Standalone {
                dir,
                listen_addr,
                advertise_addr,
            } => FlattenConfig {
                mode: ServerMode::Standalone,
                dir: dir.clone(),
                listen_addr: listen_addr.clone(),
                advertise_addr: advertise_addr.clone(),
                listen_peer_addr: None,
                advertise_peer_addr: None,
                initial_peer_addrs: None,
            },
            ServerConfig::Cluster {
                dir,
                listen_addr,
                advertise_addr,
                listen_peer_addr,
                advertise_peer_addr,
                initial_peer_addrs,
            } => FlattenConfig {
                mode: ServerMode::Cluster,
                dir: dir.clone(),
                listen_addr: listen_addr.clone(),
                advertise_addr: advertise_addr.clone(),
                listen_peer_addr: Some(listen_peer_addr.clone()),
                advertise_peer_addr: advertise_peer_addr.clone(),
                initial_peer_addrs: initial_peer_addrs.clone(),
            },
        }
    }
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

    let flatten_config = FlattenConfig::from(&config.server);

    let listen_addr = flatten_config.listen_addr.clone();
    let advertise_addr = flatten_config
        .advertise_addr
        .unwrap_or_else(|| listen_addr.clone());

    let cluster_proxy = if flatten_config.mode == ServerMode::Cluster {
        let listen_peer_addr = flatten_config
            .listen_peer_addr
            .ok_or_else(|| Error("listen peer address is required for cluster mode".to_string()))?;
        let advertise_peer_addr = flatten_config
            .advertise_peer_addr
            .unwrap_or_else(|| listen_peer_addr.clone());
        let initial_peer_addrs = flatten_config.initial_peer_addrs.ok_or_else(|| {
            Error("initial peer addresses are required for cluster mode".to_string())
        })?;

        let current_node = if let Some(node) =
            NodeInfo::load(&node_file_path(&flatten_config.dir)).change_context_lazy(make_error)?
        {
            node
        } else {
            let node = NodeInfo::init(
                None,
                "percas".to_string(),
                advertise_addr.clone(),
                advertise_peer_addr,
            );
            node.persist(&node_file_path(&flatten_config.dir))
                .change_context_lazy(make_error)?;
            node
        };

        let gossip = Arc::new(GossipState::new(current_node, initial_peer_addrs));

        // TODO: gracefully shutdown gossip
        gossip
            .clone()
            .start(rt, listen_peer_addr.clone())
            .await
            .change_context_lazy(make_error)?;

        Some(Proxy::new(gossip))
    } else {
        None
    };

    let (server, shutdown_tx) =
        percas_server::server::start_server(rt, ctx, listen_addr, advertise_addr, cluster_proxy)
            .await
            .change_context_lazy(|| {
                Error("A fatal error has occurred in server process.".to_string())
            })?;

    ctrlc::set_handler(move || shutdown_tx.shutdown())
        .change_context_lazy(|| Error("failed to setup ctrl-c signal handle".to_string()))?;

    server.await_shutdown().await;
    Ok(())
}
