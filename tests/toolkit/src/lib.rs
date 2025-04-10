use atrium_core::{Config, FoyerEngine};

pub fn make_test_name<TestFn>() -> String {
    let replacer = regex::Regex::new(r"[^a-zA-Z0-9]").unwrap();
    let test_name = std::any::type_name::<TestFn>()
        .rsplit("::")
        .find(|part| *part != "{{closure}}")
        .unwrap();
    replacer.replace_all(test_name, "_").to_string()
}

struct TestServerState {}

pub fn start_test_server(test_name: &str) -> Option<TestServerState> {
    let config = Config {
        listen_addr: "".to_string(),
        advertise_addr: "".to_string(),
        path: "".to_string(),
        disk_capacity: 0,
        memory_capacity: 0,
    };

    let engine = FoyerEngine::try_new(&config.path, config.memory_capacity, config.disk_capacity);

    Some(TestServerState {})
}
