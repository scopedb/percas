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

use std::process::ExitCode;

use percas_client::Client;
use percas_client::ClientBuilder;
use percas_core::make_runtime;
use tests_toolkit::make_test_name;

pub struct Testkit {
    pub client: Client,
}

pub fn harness<T, Fut>(test: impl Send + FnOnce(Testkit) -> Fut) -> ExitCode
where
    T: std::process::Termination,
    Fut: Send + Future<Output = T>,
{
    let rt = make_runtime("test_runtime", "test_thread", 4);

    let test_name = make_test_name::<Fut>();
    let Some(state) = tests_toolkit::start_test_server(&test_name, &rt) else {
        return ExitCode::SUCCESS;
    };

    rt.block_on(async move {
        let addr = state
            .server_state
            .listen_addr()
            .as_socket_addr()
            .cloned()
            .unwrap();
        let server_addr = format!("http://{}/", addr);
        let client = ClientBuilder::new(server_addr).build().unwrap();

        let exit_code = test(Testkit { client }).await.report();

        state.shutdown().await;
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
