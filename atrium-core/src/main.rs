use clap::Parser;

use crate::config::Config;

mod config;
mod engine;
mod server;

#[derive(clap::Parser)]
struct Command {
    #[clap(short, long, help = "Path to config file")]
    config: String,
}

#[tokio::main]
async fn main() {
    logforth::builder()
        .dispatch(|d| {
            d.filter(log::LevelFilter::Debug)
                .append(logforth::append::Stdout::default())
        })
        .apply();

    let cmd = Command::parse();
    let config = toml::from_str::<Config>(&std::fs::read_to_string(&cmd.config).unwrap()).unwrap();

    log::info!("config: {config:#?}");

    let _ = server::start_server().await.inspect_err(|err| {
        log::error!("server stopped: {}", err);
    });
}
