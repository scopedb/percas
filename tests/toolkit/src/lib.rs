use std::any::Any;
use std::net::SocketAddr;
use std::sync::Arc;

use atrium::server::ServerState;
use atrium_core::Config;
use atrium_core::FoyerEngine;

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
    let temp_dir = tempfile::tempdir().unwrap();

    let host = local_ip_address::local_ip().unwrap();
    let listen_addr = SocketAddr::new(host, 0).to_string();

    let config = Config {
        listen_addr,
        path: temp_dir.path().to_path_buf(),
        ..Default::default()
    };

    let server_state = rt.block_on(async move {
        let engine =
            FoyerEngine::try_new(&config.path, config.memory_capacity, config.disk_capacity)
                .await
                .unwrap();
        let ctx = Arc::new(atrium::Context { engine });
        atrium::server::start_server(&config, ctx).await.unwrap()
    });

    drop_guard.push(Box::new(temp_dir));
    Some(TestServerState {
        server_state,
        _drop_guards: drop_guard,
    })
}
