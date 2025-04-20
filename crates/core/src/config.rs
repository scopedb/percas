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

use std::path::Path;
use std::path::PathBuf;

use jiff::SignedDuration;
use serde::Deserialize;
use serde::Serialize;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub server: ServerConfig,
    pub storage: StorageConfig,
    pub telemetry: TelemetryConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(tag = "mode")]
pub enum ServerConfig {
    #[serde(rename = "standalone")]
    Standalone {
        #[serde(default = "default_dir")]
        dir: PathBuf,
        #[serde(default = "default_listen_addr")]
        listen_addr: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        advertise_addr: Option<String>,
    },
    #[serde(rename = "cluster")]
    Cluster {
        #[serde(default = "default_dir")]
        dir: PathBuf,
        #[serde(default = "default_listen_addr")]
        listen_addr: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        advertise_addr: Option<String>,
        #[serde(default = "default_listen_peer_addr")]
        listen_peer_addr: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        advertise_peer_addr: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        initial_peer_addrs: Option<Vec<String>>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    #[serde(default = "default_data_dir")]
    pub data_dir: PathBuf,
    pub disk_capacity: u64,
    pub memory_capacity: u64,
}

fn default_listen_addr() -> String {
    "0.0.0.0:7654".to_string()
}

fn default_listen_peer_addr() -> String {
    "0.0.0.0:7655".to_string()
}

fn default_dir() -> PathBuf {
    PathBuf::from("/var/lib/percas")
}

fn default_data_dir() -> PathBuf {
    PathBuf::from("/var/lib/percas/data")
}

pub fn node_file_path(base_dir: &Path) -> PathBuf {
    base_dir.join("node.json")
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TelemetryConfig {
    #[serde(default = "LogsConfig::disabled")]
    pub logs: LogsConfig,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub traces: Option<TracesConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metrics: Option<MetricsConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LogsConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<FileAppenderConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stderr: Option<StderrAppenderConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub opentelemetry: Option<OpentelemetryAppenderConfig>,
}

impl LogsConfig {
    pub fn disabled() -> Self {
        Self {
            file: None,
            stderr: None,
            opentelemetry: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileAppenderConfig {
    pub filter: String,
    pub dir: String,
    pub max_files: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StderrAppenderConfig {
    pub filter: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OpentelemetryAppenderConfig {
    pub filter: String,
    pub otlp_endpoint: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TracesConfig {
    pub capture_log_filter: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub opentelemetry: Option<OpentelemetryTracesConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OpentelemetryTracesConfig {
    pub otlp_endpoint: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MetricsConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub opentelemetry: Option<OpentelemetryMetricsConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OpentelemetryMetricsConfig {
    pub otlp_endpoint: String,
    #[serde(default = "default_metrics_push_interval")]
    pub push_interval: SignedDuration,
}

fn default_metrics_push_interval() -> SignedDuration {
    SignedDuration::from_secs(30)
}

impl Default for Config {
    fn default() -> Self {
        Self {
            server: ServerConfig::Standalone {
                dir: default_dir(),
                listen_addr: default_listen_addr(),
                advertise_addr: None,
            },
            storage: StorageConfig {
                data_dir: default_data_dir(),
                disk_capacity: 512 * 1024 * 1024,
                memory_capacity: 64 * 1024 * 1024,
            },
            telemetry: TelemetryConfig {
                logs: LogsConfig {
                    file: Some(FileAppenderConfig {
                        filter: "INFO".to_string(),
                        dir: "logs".to_string(),
                        max_files: 64,
                    }),
                    stderr: Some(StderrAppenderConfig {
                        filter: "INFO".to_string(),
                    }),
                    opentelemetry: Some(OpentelemetryAppenderConfig {
                        filter: "INFO".to_string(),
                        otlp_endpoint: "http://127.0.0.1:4317".to_string(),
                    }),
                },
                traces: Some(TracesConfig {
                    capture_log_filter: "INFO".to_string(),
                    opentelemetry: Some(OpentelemetryTracesConfig {
                        otlp_endpoint: "http://127.0.0.1:4317".to_string(),
                    }),
                }),
                metrics: Some(MetricsConfig {
                    opentelemetry: Some(OpentelemetryMetricsConfig {
                        otlp_endpoint: "http://127.0.0.1:4317".to_string(),
                        push_interval: SignedDuration::from_secs(30),
                    }),
                }),
            },
        }
    }
}
