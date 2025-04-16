use std::process::ExitCode;

use percas_client::Client;
use percas_client::ClientBuilder;
use tests_toolkit::make_test_name;

pub struct Testkit {
    pub client: Client,
}

pub fn harness<T, Fut>(test: impl Send + FnOnce(Testkit) -> Fut) -> ExitCode
where
    T: std::process::Termination,
    Fut: Send + Future<Output = T>,
{
    let rt = tokio::runtime::Runtime::new().unwrap();

    let test_name = make_test_name::<Fut>();
    let Some(state) = tests_toolkit::start_test_server(&test_name, &rt) else {
        return ExitCode::SUCCESS;
    };

    rt.block_on(async move {
        let server_addr = format!("http://{}/", state.server_state.server_advertise_addr());
        let client = ClientBuilder::new(server_addr).build();

        let exit_code = test(Testkit { client }).await.report();

        state.server_state.shutdown();
        state.server_state.await_shutdown().await;
        exit_code
    })
}

pub fn render_hex<T: AsRef<[u8]>>(data: T) -> String {
    let config = pretty_hex::HexConfig {
        width: 8,
        group: 0,
        ..Default::default()
    };
    format!(
        "{:?}",
        pretty_hex::PrettyHex::hex_conf(&data.as_ref(), config)
    )
}
