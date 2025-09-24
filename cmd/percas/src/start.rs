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
use exn::Result;
use exn::ResultExt;
use mea::shutdown::ShutdownRecv;
use mixtrics::registry::opentelemetry_0_30::OpenTelemetryMetricsRegistry;
use percas_cluster::GossipFuture;
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
    cluster_id: Option<String>,
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
                cluster_id: None,
            },
            ServerConfig::Cluster {
                dir,
                listen_addr,
                advertise_addr,
                listen_peer_addr,
                advertise_peer_addr,
                initial_advertise_peer_addrs,
                cluster_id,
            } => FlattenConfig {
                mode: ServerMode::Cluster,
                dir: dir.clone(),
                listen_addr: listen_addr.clone(),
                advertise_addr: advertise_addr.clone(),
                listen_peer_addr: Some(listen_peer_addr.clone()),
                advertise_peer_addr: advertise_peer_addr.clone(),
                initial_peer_addrs: initial_advertise_peer_addrs.clone(),
                cluster_id: Some(cluster_id.clone()),
            },
        }
    }
}

async fn run_server(
    server_rt: &Runtime,
    gossip_rt: &Runtime,
    node_id: Uuid,
    config: Config,
) -> Result<(), Error> {
    let make_error = || Error("failed to start server".to_string());

    let engine = FoyerEngine::try_new(
        &config.storage.data_dir,
        config.storage.memory_capacity,
        config.storage.disk_capacity,
        config.storage.disk_throttle,
        Some(OpenTelemetryMetricsRegistry::new(
            GlobalMetrics::get().meter.clone(),
        )),
    )
    .await
    .or_raise(make_error)?;

    let (shutdown_tx, shutdown_rx) = mea::shutdown::new_pair();
    let ctx = Arc::new(PercasContext { engine });

    let flatten_config = FlattenConfig::from(&config.server);

    let (acceptor, advertise_addr) = make_acceptor_and_advertise_addr(
        flatten_config.listen_addr.as_str(),
        flatten_config.advertise_addr.as_deref(),
    )
    .await
    .or_raise(make_error)?;

    let (cluster_proxy, gossip_futs) = match flatten_config.mode {
        ServerMode::Standalone => (None, vec![]),
        ServerMode::Cluster => {
            let advertise_addr = advertise_addr.to_string();
            let shutdown_rx = shutdown_rx.clone();
            let (proxy, futs) = run_gossip_proxy(
                gossip_rt,
                shutdown_rx,
                flatten_config,
                node_id,
                advertise_addr,
            )
            .await?;
            (Some(proxy), futs)
        }
    };

    let server = percas_server::server::start_server(
        server_rt,
        shutdown_rx,
        ctx,
        acceptor,
        advertise_addr,
        cluster_proxy,
        gossip_futs,
    )
    .await
    .or_raise(|| Error("A fatal error has occurred in server process.".to_string()))?;

    ctrlc::set_handler(move || shutdown_tx.shutdown())
        .or_raise(|| Error("failed to setup ctrl-c signal handle".to_string()))?;

    server.await_shutdown().await;
    Ok(())
}

async fn run_gossip_proxy(
    gossip_rt: &Runtime,
    shutdown_rx: ShutdownRecv,
    flatten_config: FlattenConfig,
    node_id: Uuid,
    advertise_addr: String,
) -> Result<(Proxy, Vec<GossipFuture>), Error> {
    let make_error = || Error("failed to start gossip proxy".to_string());

    let listen_peer_addr = flatten_config
        .listen_peer_addr
        .ok_or_else(|| Error("listen peer address is required for cluster mode".to_string()))?;

    let (acceptor, advertise_peer_addr) = make_acceptor_and_advertise_addr(
        listen_peer_addr.as_str(),
        flatten_config.advertise_peer_addr.as_deref(),
    )
    .await
    .or_raise(make_error)?;
    let advertise_peer_addr = advertise_peer_addr.to_string();

    let initial_peer_addrs = flatten_config
        .initial_peer_addrs
        .ok_or_else(|| Error("initial peer addresses are required for cluster mode".to_string()))?;
    let cluster_id = flatten_config
        .cluster_id
        .ok_or_else(|| Error("cluster id is required for cluster mode".to_string()))?;

    let current_node = if let Some(mut node) = NodeInfo::load(
        &node_file_path(&flatten_config.dir),
        advertise_addr.clone(),
        advertise_peer_addr.clone(),
    )
    .or_raise(make_error)?
    {
        node.advance_incarnation();
        node.persist(&node_file_path(&flatten_config.dir))
            .or_raise(make_error)?;
        node
    } else {
        let node = NodeInfo::init(
            node_id,
            cluster_id,
            advertise_addr.clone(),
            advertise_peer_addr,
        );
        node.persist(&node_file_path(&flatten_config.dir))
            .or_raise(make_error)?;
        node
    };

    let gossip = Arc::new(GossipState::new(
        current_node,
        initial_peer_addrs,
        flatten_config.dir.clone(),
    ));

    let futs = gossip
        .clone()
        .start(gossip_rt, shutdown_rx, acceptor)
        .await
        .or_raise(make_error)?;

    Ok((Proxy::new(gossip), futs))
}
