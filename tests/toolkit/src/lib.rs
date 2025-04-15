use std::any::Any;
use std::net::SocketAddr;
use std::sync::Arc;

use atrium::server::ServerState;
use atrium::telemetry;
use atrium_core::Config;
use atrium_core::FoyerEngine;
use atrium_core::LogsConfig;
use atrium_core::ServerConfig;
use atrium_core::StorageConfig;
use atrium_core::TelemetryConfig;

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
    _drop_guards: Vec<DropGuard>,
}

pub fn start_test_server(
    _test_name: &str,
    rt: &tokio::runtime::Runtime,
) -> Option<TestServerState> {
    let mut drop_guard = Vec::<DropGuard>::new();
    drop_guard.extend(
        telemetry::init(
            "atrium",
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
    let listen_addr = SocketAddr::new(host, 0).to_string();

    let default_config = Config::default();
    let config = Config {
        server: ServerConfig {
            listen_addr,
            advertise_addr: None,
        },
        storage: StorageConfig {
            data_dir: temp_dir.path().to_path_buf(),
            ..default_config.storage
        },
        telemetry: TelemetryConfig {
            logs: LogsConfig::disabled(),
            traces: None,
            metrics: None,
        },
    };

    let server_state = rt.block_on(async move {
        let engine = FoyerEngine::try_new(
            &config.storage.data_dir,
            config.storage.memory_capacity,
            config.storage.disk_capacity,
        )
        .await
        .unwrap();
        let ctx = Arc::new(atrium::AtriumContext { engine });
        atrium::server::start_server(&config.server, ctx)
            .await
            .unwrap()
    });

    drop_guard.push(Box::new(temp_dir));
    Some(TestServerState {
        server_state,
        _drop_guards: drop_guard,
    })
}
