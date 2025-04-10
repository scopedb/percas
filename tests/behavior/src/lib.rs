use std::process::ExitCode;

use tests_toolkit::make_test_name;

pub struct Testkit {}

pub fn harness<T, Fut>(test: impl Send + FnOnce(Testkit) -> Fut) -> ExitCode
where
    T: std::process::Termination,
    Fut: Send + Future<Output = T>,
{
    let test_name = make_test_name::<Fut>();
    let Some(_state) = tests_toolkit::start_test_server(&test_name) else {
        return ExitCode::SUCCESS;
    };

    let rt = tokio::runtime::Runtime::new().unwrap();

    rt.block_on(async move {
        let exit_code = test(Testkit {}).await.report();
        exit_code
    })
}
