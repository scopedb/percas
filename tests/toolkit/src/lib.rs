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

use std::any::Any;
use std::net::SocketAddr;
use std::sync::Arc;

use mea::shutdown::ShutdownSend;
use percas_core::Config;
use percas_core::FoyerEngine;
use percas_core::LogsConfig;
use percas_core::Runtime;
use percas_core::ServerConfig;
use percas_core::StorageConfig;
use percas_core::TelemetryConfig;
use percas_server::server::ServerState;
use percas_server::telemetry;

pub fn make_test_name<TestFn>() -> String {
    let replacer = regex::Regex::new(r"[^a-zA-Z0-9]").unwrap();
    let test_name = std::any::type_name::<TestFn>()
        .rsplit("::")
        .find(|part| *part != "{{closure}}")
        .unwrap();
    replacer.replace_all(test_name, "_").to_string()
}

type DropGuard = Box<dyn Any>;

#[derive(Debug)]
pub struct TestServerState {
    pub server_state: ServerState,
    shutdown_tx_server: ShutdownSend,
    _drop_guards: Vec<DropGuard>,
}

impl TestServerState {
    pub async fn shutdown(self) {
        self.shutdown_tx_server.shutdown();
        self.server_state.await_shutdown().await;
    }
}

pub fn start_test_server(_test_name: &str, rt: &Runtime) -> Option<TestServerState> {
    let mut drop_guard = Vec::<DropGuard>::new();
    drop_guard.extend(
        telemetry::init(
            rt,
            "percas",
            TelemetryConfig {
                logs: LogsConfig::disabled(),
                traces: None,
                metrics: None,
            },
        )
        .into_iter()
        .map(|x| Box::new(x) as DropGuard),
    );

    let temp_dir = tempfile::tempdir().unwrap();

    let host = local_ip_address::local_ip().unwrap();
    let listen_addr = SocketAddr::new(host, 0);

    let default_config = Config::default();
    let config = Config {
        server: ServerConfig::Standalone {
            dir: temp_dir.path().to_path_buf(),
            listen_addr: listen_addr.to_string(),
            advertise_addr: None,
        },
        storage: StorageConfig {
            data_dir: temp_dir.path().to_path_buf().join("data"),
            ..default_config.storage
        },
        telemetry: TelemetryConfig {
            logs: LogsConfig::disabled(),
            traces: None,
            metrics: None,
        },
    };

    let (server_state, shutdown_tx_server) = rt.block_on(async move {
        let engine = FoyerEngine::try_new(
            &config.storage.data_dir,
            config.storage.memory_capacity,
            config.storage.disk_capacity,
        )
        .await
        .unwrap();
        let ctx = Arc::new(percas_server::PercasContext { engine });
        percas_server::server::start_server(
            rt,
            ctx,
            listen_addr.to_string(),
            listen_addr.to_string(),
            None,
        )
        .await
        .unwrap()
    });

    drop_guard.push(Box::new(temp_dir));
    Some(TestServerState {
        server_state,
        shutdown_tx_server,
        _drop_guards: drop_guard,
    })
}
