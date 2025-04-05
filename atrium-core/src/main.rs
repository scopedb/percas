mod engine;
mod server;

#[tokio::main]
async fn main() {
    logforth::builder()
        .dispatch(|d| {
            d.filter(log::LevelFilter::Debug)
                .append(logforth::append::Stdout::default())
        })
        .apply();

    let _ = server::start_server().await.inspect_err(|err| {
        log::error!("server stopped: {}", err);
    });
}
