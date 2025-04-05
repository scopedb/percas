use serde::Deserialize;
use serde::Serialize;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "default_listen_addr")]
    pub listen_addr: String,
    #[serde(default = "default_advertise_addr")]
    pub advertise_addr: String,
    #[serde(default = "default_path")]
    pub path: String,
    pub capacity: u64,
}

fn default_listen_addr() -> String {
    "0.0.0.0:7654".to_string()
}

fn default_advertise_addr() -> String {
    "127.0.0.1:7654".to_string()
}

fn default_path() -> String {
    "/usr/local/atrium".to_string()
}
