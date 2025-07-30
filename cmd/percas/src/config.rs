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

use std::path::PathBuf;
use std::str::FromStr;

use exn::Result;
use exn::ResultExt;
use exn::bail;
use percas_core::Config;
use percas_core::known_option_entries;
use serde::Deserialize;
use serde::de::IntoDeserializer;
use toml_edit::DocumentMut;

use crate::Error;

#[derive(Debug)]
pub struct LoadConfigResult {
    pub config: Config,
    pub warnings: Vec<String>,
}

pub fn load_config(config_file: PathBuf) -> Result<LoadConfigResult, Error> {
    // Layer 0: the config file
    let content = std::fs::read_to_string(&config_file).or_raise(|| {
        Error(format!(
            "failed to read config file: {}",
            config_file.display()
        ))
    })?;
    let mut config = DocumentMut::from_str(&content)
        .or_raise(|| Error("failed to parse config content".to_string()))?;

    // Layer 1: environment variables
    let env = std::env::vars()
        .filter(|(k, _)| k.starts_with("PERCAS_CONFIG_"))
        .collect::<std::collections::HashMap<_, _>>();

    fn set_toml_path(
        doc: &mut DocumentMut,
        key: &str,
        path: &'static str,
        value: toml_edit::Item,
    ) -> Vec<String> {
        let mut current = doc.as_item_mut();
        let mut warnings = vec![];

        let parts = path.split('.').collect::<Vec<_>>();
        let len = parts.len();
        assert!(len > 0, "path must not be empty");

        for part in parts.iter().take(len - 1) {
            if current.get(part).is_none() {
                warnings.push(format!(
                    "[key={key}] config path '{path}' has missing parent '{part}'; created",
                ));
            }
            current = &mut current[part];
        }

        current[parts[len - 1]] = value;
        warnings
    }

    let known_option_entries = known_option_entries();
    let mut warnings = vec![];
    for (k, v) in env {
        let Some(ent) = known_option_entries.iter().find(|e| k == e.env_name) else {
            bail!(Error(format!(
                "failed to parse unknown environment variable {k} with value {v}"
            )))
        };

        let (path, item) = match ent.ent_type {
            "string" => {
                let path = ent.ent_path;
                let value = toml_edit::value(v);
                (path, value)
            }
            "integer" => {
                let path = ent.ent_path;
                let value = v
                    .parse::<i64>()
                    .or_raise(|| Error(format!("failed to parse integer value {v} of key {k}")))?;
                let value = toml_edit::value(value);
                (path, value)
            }
            "boolean" => {
                let path = ent.ent_path;
                let value = v
                    .parse::<bool>()
                    .or_raise(|| Error(format!("failed to parse boolean value {v} of key {k}")))?;
                let value = toml_edit::value(value);
                (path, value)
            }
            ty => {
                bail!(Error(format!(
                    "failed to parse environment variable {k} with value {v} and resolved type {ty}"
                )))
            }
        };
        let new_warnings = set_toml_path(&mut config, &k, path, item);
        warnings.extend(new_warnings);
    }

    let config = Config::deserialize(config.into_deserializer())
        .or_raise(|| Error("failed to deserialize config".to_string()))?;
    Ok(LoadConfigResult { config, warnings })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use percas_core::ServerConfig;
    use percas_core::default_data_dir;
    use percas_core::default_dir;
    use sealed_test::prelude::rusty_fork_test;
    use sealed_test::prelude::sealed_test;
    use sealed_test::prelude::tempfile;

    use super::*;

    #[test]
    fn test_default_config() {
        let workspace = env!("CARGO_WORKSPACE_DIR");
        let mut dev_config = load_config(PathBuf::from(format!(
            "{workspace}/dev/standalone/config.toml"
        )))
        .unwrap()
        .config;

        dev_config.storage.data_dir = default_data_dir();
        if let ServerConfig::Standalone {
            dir,
            advertise_addr,
            ..
        } = &mut dev_config.server
        {
            *dir = default_dir();
            *advertise_addr = None;
        }

        assert_eq!(dev_config, Config::default());
    }

    #[sealed_test(env = [("PERCAS_FOO_BAR", "baz")])]
    fn test_percas_prefix_no_conflict() {
        let workspace = env!("CARGO_WORKSPACE_DIR");
        let mut dev_config = load_config(PathBuf::from(format!(
            "{workspace}/dev/standalone/config.toml"
        )))
        .unwrap()
        .config;

        dev_config.storage.data_dir = default_data_dir();
        if let ServerConfig::Standalone {
            dir,
            advertise_addr,
            ..
        } = &mut dev_config.server
        {
            *dir = default_dir();
            *advertise_addr = None;
        }

        assert_eq!(dev_config, Config::default());
    }

    #[sealed_test(env = [
        ("PERCAS_CONFIG_TELEMETRY_LOGS_OPENTELEMETRY_OTLP_ENDPOINT", "http://192.168.1.14:4317"),
    ])]
    fn test_override_advertise_addr() {
        let workspace = env!("CARGO_WORKSPACE_DIR");
        let dev_config = load_config(PathBuf::from(format!(
            "{workspace}/dev/standalone/config.toml"
        )))
        .unwrap()
        .config;
        assert_eq!(
            dev_config
                .telemetry
                .logs
                .opentelemetry
                .unwrap()
                .otlp_endpoint,
            "http://192.168.1.14:4317"
        );
    }
}
