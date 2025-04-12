use std::path::PathBuf;

use serde::Deserialize;
use serde::Serialize;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "default_listen_addr")]
    pub listen_addr: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub advertise_addr: Option<String>,
    #[serde(default = "default_path")]
    pub path: PathBuf,
    pub disk_capacity: u64,
    pub memory_capacity: u64,
}

fn default_listen_addr() -> String {
    "0.0.0.0:7654".to_string()
}

fn default_path() -> PathBuf {
    PathBuf::from("/usr/local/atrium")
}

pub fn data_path(base: impl Into<PathBuf>) -> PathBuf {
    base.into().join("data")
}

impl Default for Config {
    fn default() -> Self {
        Self {
            listen_addr: default_listen_addr(),
            advertise_addr: None,
            path: default_path(),
            disk_capacity: 512 * 1024,
            memory_capacity: 1024 * 1024,
        }
    }
}
