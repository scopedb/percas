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

use serde::Deserialize;
use serde::Serialize;

use crate::newtype::DiskThrottle;
use crate::newtype::SignedDuration;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub server: ServerConfig,
    pub storage: StorageConfig,
    pub telemetry: TelemetryConfig,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
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
        initial_advertise_peer_addrs: Option<Vec<String>>,
        #[serde(default = "default_cluster_id")]
        cluster_id: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct StorageConfig {
    #[serde(default = "default_data_dir")]
    pub data_dir: PathBuf,
    pub disk_capacity: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disk_throttle: Option<DiskThrottle>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_capacity: Option<u64>,
}

fn default_listen_addr() -> String {
    "0.0.0.0:7654".to_string()
}

fn default_listen_peer_addr() -> String {
    "0.0.0.0:7655".to_string()
}

pub fn default_dir() -> PathBuf {
    PathBuf::from("/var/lib/percas")
}

pub fn default_data_dir() -> PathBuf {
    PathBuf::from("/var/lib/percas/data")
}

fn default_cluster_id() -> String {
    "percas-cluster".to_string()
}

pub fn node_file_path(base_dir: &Path) -> PathBuf {
    base_dir.join("node.json")
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
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
#[cfg_attr(test, derive(schemars::JsonSchema))]
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
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct FileAppenderConfig {
    pub filter: String,
    pub dir: String,
    pub max_files: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct StderrAppenderConfig {
    pub filter: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct OpentelemetryAppenderConfig {
    pub filter: String,
    pub otlp_endpoint: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct TracesConfig {
    pub capture_log_filter: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub opentelemetry: Option<OpentelemetryTracesConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct OpentelemetryTracesConfig {
    pub otlp_endpoint: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct MetricsConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub opentelemetry: Option<OpentelemetryMetricsConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct OpentelemetryMetricsConfig {
    pub otlp_endpoint: String,
    #[serde(default = "default_metrics_push_interval")]
    pub push_interval: SignedDuration,
}

fn default_metrics_push_interval() -> SignedDuration {
    jiff::SignedDuration::from_secs(30).into()
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
                disk_throttle: None,
                memory_capacity: None,
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
                        push_interval: jiff::SignedDuration::from_secs(30).into(),
                    }),
                }),
            },
        }
    }
}

#[derive(Clone, Debug, Ord, PartialOrd, Eq, PartialEq)]
pub struct OptionEntry {
    /// The name of the environment variable.
    pub env_name: &'static str,
    /// The path in the config file.
    pub ent_path: &'static str,
    /// The type of the value.
    pub ent_type: &'static str,
}

pub const fn known_option_entries() -> &'static [OptionEntry] {
    &[
        OptionEntry {
            env_name: "PERCAS_CONFIG_SERVER_ADVERTISE_ADDR",
            ent_path: "server.advertise_addr",
            ent_type: "string",
        },
        OptionEntry {
            env_name: "PERCAS_CONFIG_SERVER_ADVERTISE_PEER_ADDR",
            ent_path: "server.advertise_peer_addr",
            ent_type: "string",
        },
        OptionEntry {
            env_name: "PERCAS_CONFIG_SERVER_CLUSTER_ID",
            ent_path: "server.cluster_id",
            ent_type: "string",
        },
        OptionEntry {
            env_name: "PERCAS_CONFIG_SERVER_DIR",
            ent_path: "server.dir",
            ent_type: "string",
        },
        OptionEntry {
            env_name: "PERCAS_CONFIG_SERVER_INITIAL_ADVERTISE_PEER_ADDRS",
            ent_path: "server.initial_advertise_peer_addrs",
            ent_type: "array",
        },
        OptionEntry {
            env_name: "PERCAS_CONFIG_SERVER_LISTEN_ADDR",
            ent_path: "server.listen_addr",
            ent_type: "string",
        },
        OptionEntry {
            env_name: "PERCAS_CONFIG_SERVER_LISTEN_PEER_ADDR",
            ent_path: "server.listen_peer_addr",
            ent_type: "string",
        },
        OptionEntry {
            env_name: "PERCAS_CONFIG_SERVER_MODE",
            ent_path: "server.mode",
            ent_type: "string",
        },
        OptionEntry {
            env_name: "PERCAS_CONFIG_STORAGE_DATA_DIR",
            ent_path: "storage.data_dir",
            ent_type: "string",
        },
        OptionEntry {
            env_name: "PERCAS_CONFIG_STORAGE_DISK_CAPACITY",
            ent_path: "storage.disk_capacity",
            ent_type: "integer",
        },
        OptionEntry {
            env_name: "PERCAS_CONFIG_STORAGE_DISK_THROTTLE_IOPS_COUNTER_MODE",
            ent_path: "storage.disk_throttle.iops_counter.mode",
            ent_type: "string",
        },
        OptionEntry {
            env_name: "PERCAS_CONFIG_STORAGE_DISK_THROTTLE_READ_IOPS",
            ent_path: "storage.disk_throttle.read_iops",
            ent_type: "integer",
        },
        OptionEntry {
            env_name: "PERCAS_CONFIG_STORAGE_DISK_THROTTLE_READ_THROUGHPUT",
            ent_path: "storage.disk_throttle.read_throughput",
            ent_type: "integer",
        },
        OptionEntry {
            env_name: "PERCAS_CONFIG_STORAGE_DISK_THROTTLE_WRITE_IOPS",
            ent_path: "storage.disk_throttle.write_iops",
            ent_type: "integer",
        },
        OptionEntry {
            env_name: "PERCAS_CONFIG_STORAGE_DISK_THROTTLE_WRITE_THROUGHPUT",
            ent_path: "storage.disk_throttle.write_throughput",
            ent_type: "integer",
        },
        OptionEntry {
            env_name: "PERCAS_CONFIG_STORAGE_MEMORY_CAPACITY",
            ent_path: "storage.memory_capacity",
            ent_type: "integer",
        },
        OptionEntry {
            env_name: "PERCAS_CONFIG_TELEMETRY_LOGS_FILE_DIR",
            ent_path: "telemetry.logs.file.dir",
            ent_type: "string",
        },
        OptionEntry {
            env_name: "PERCAS_CONFIG_TELEMETRY_LOGS_FILE_FILTER",
            ent_path: "telemetry.logs.file.filter",
            ent_type: "string",
        },
        OptionEntry {
            env_name: "PERCAS_CONFIG_TELEMETRY_LOGS_FILE_MAX_FILES",
            ent_path: "telemetry.logs.file.max_files",
            ent_type: "integer",
        },
        OptionEntry {
            env_name: "PERCAS_CONFIG_TELEMETRY_LOGS_OPENTELEMETRY_FILTER",
            ent_path: "telemetry.logs.opentelemetry.filter",
            ent_type: "string",
        },
        OptionEntry {
            env_name: "PERCAS_CONFIG_TELEMETRY_LOGS_OPENTELEMETRY_OTLP_ENDPOINT",
            ent_path: "telemetry.logs.opentelemetry.otlp_endpoint",
            ent_type: "string",
        },
        OptionEntry {
            env_name: "PERCAS_CONFIG_TELEMETRY_LOGS_STDERR_FILTER",
            ent_path: "telemetry.logs.stderr.filter",
            ent_type: "string",
        },
        OptionEntry {
            env_name: "PERCAS_CONFIG_TELEMETRY_METRICS_OPENTELEMETRY_OTLP_ENDPOINT",
            ent_path: "telemetry.metrics.opentelemetry.otlp_endpoint",
            ent_type: "string",
        },
        OptionEntry {
            env_name: "PERCAS_CONFIG_TELEMETRY_METRICS_OPENTELEMETRY_PUSH_INTERVAL",
            ent_path: "telemetry.metrics.opentelemetry.push_interval",
            ent_type: "string",
        },
        OptionEntry {
            env_name: "PERCAS_CONFIG_TELEMETRY_TRACES_CAPTURE_LOG_FILTER",
            ent_path: "telemetry.traces.capture_log_filter",
            ent_type: "string",
        },
        OptionEntry {
            env_name: "PERCAS_CONFIG_TELEMETRY_TRACES_OPENTELEMETRY_OTLP_ENDPOINT",
            ent_path: "telemetry.traces.opentelemetry.otlp_endpoint",
            ent_type: "string",
        },
    ]
}

#[cfg(test)]
mod codegen {
    use std::collections::BTreeMap;
    use std::collections::btree_map::Entry;

    use googletest::assert_that;
    use googletest::prelude::container_eq;
    use schemars::schema_for;

    use super::*;

    type Object = serde_json::Map<String, serde_json::Value>;
    type EntryMap = BTreeMap<String, OwnedOptionEntry>;

    #[derive(Clone, Debug)]
    struct OwnedOptionEntry {
        env_name: String,
        ent_path: String,
        ent_type: String,
    }

    impl PartialEq<OwnedOptionEntry> for OptionEntry {
        fn eq(&self, other: &OwnedOptionEntry) -> bool {
            self.env_name == other.env_name
                && self.ent_path == other.ent_path
                && self.ent_type == other.ent_type
        }
    }

    impl PartialEq<OptionEntry> for OwnedOptionEntry {
        fn eq(&self, other: &OptionEntry) -> bool {
            self.env_name == other.env_name
                && self.ent_path == other.ent_path
                && self.ent_type == other.ent_type
        }
    }

    #[test]
    fn test_config_schema() {
        let mut result = EntryMap::new();

        let schema = schema_for!(Config);
        let defs = schema.get("$defs").unwrap().as_object().unwrap();
        let o = schema.as_object().unwrap();
        dump_config_schema("", defs, o, &mut result);

        let options = result.into_values().collect::<Vec<_>>();
        let known_option_entries = known_option_entries().to_vec();
        assert_that!(options, container_eq(known_option_entries));
    }

    fn fetch_ref_object<'a>(defs: &'a Object, r: &str) -> &'a Object {
        const DEFS_PREFIX_LEN: usize = "#/$defs/".len();
        defs.get(&r[DEFS_PREFIX_LEN..])
            .unwrap()
            .as_object()
            .unwrap()
    }

    fn dump_config_schema(prefix: &str, defs: &Object, o: &Object, result: &mut EntryMap) {
        if let Some(r) = o.get("$ref") {
            let r = r.as_str().unwrap();
            let o = fetch_ref_object(defs, r);
            return dump_config_schema(prefix, defs, o, result);
        }

        if let Some(one_of) = o.get("oneOf") {
            let one_of = one_of.as_array().unwrap();
            for o in one_of {
                dump_config_schema(prefix, defs, o.as_object().unwrap(), result);
            }
            return;
        }

        if let Some(any_of) = o.get("anyOf") {
            let any_of = any_of.as_array().unwrap();
            for o in any_of {
                dump_config_schema(prefix, defs, o.as_object().unwrap(), result);
            }
            return;
        }

        let ty = o.get("type").unwrap();
        let types = if let Some(types) = ty.as_array() {
            types.clone()
        } else {
            vec![ty.clone()]
        };

        for ty in types {
            let ty = ty.as_str().unwrap();
            match ty {
                "null" => {}
                "object" => {
                    let props = o.get("properties").unwrap().as_object().unwrap();
                    for (k, v) in props {
                        let prefix = if prefix.is_empty() {
                            k.clone()
                        } else {
                            format!("{prefix}.{k}")
                        };
                        dump_config_schema(&prefix, defs, v.as_object().unwrap(), result);
                    }
                }
                ty => {
                    let path = prefix;
                    let name = prefix.to_ascii_uppercase().replace(".", "_");
                    let name = format!("PERCAS_CONFIG_{name}");
                    match result.entry(prefix.to_string()) {
                        Entry::Vacant(ent) => {
                            ent.insert(OwnedOptionEntry {
                                env_name: name,
                                ent_path: path.to_string(),
                                ent_type: ty.to_string(),
                            });
                        }
                        Entry::Occupied(ent) => {
                            assert_eq!(ent.get().ent_type, ty);
                        }
                    }
                }
            }
        }
    }
}
