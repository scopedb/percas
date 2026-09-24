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

use mixtrics::registry::opentelemetry_0_32::OpenTelemetryMetricsRegistry;

use crate::Cache2Engine;
use crate::FoyerEngine;
use crate::Runtime;
use crate::StorageConfig;
use crate::StorageEngineKind;

#[derive(Debug)]
pub enum StorageError {
    Cache2(cache2::Error),
    Foyer(String),
    InvalidConfig(String),
}

impl std::fmt::Display for StorageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cache2(err) => write!(f, "cache2: {err}"),
            Self::Foyer(err) => write!(f, "foyer: {err}"),
            Self::InvalidConfig(err) => write!(f, "invalid storage configuration: {err}"),
        }
    }
}

impl std::error::Error for StorageError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Cache2(err) => Some(err),
            _ => None,
        }
    }
}

impl From<cache2::Error> for StorageError {
    fn from(err: cache2::Error) -> Self {
        Self::Cache2(err)
    }
}

impl StorageError {
    pub fn is_overloaded(&self) -> bool {
        matches!(self, Self::Cache2(err) if err.kind() == cache2::ErrorKind::Overloaded)
    }

    pub fn is_invalid_input(&self) -> bool {
        matches!(self, Self::Cache2(err) if err.kind() == cache2::ErrorKind::InvalidInput)
    }
}

#[derive(Debug, Default)]
pub struct StorageStatistics {
    pub disk_read_bytes: u64,
    pub disk_write_bytes: u64,
    pub disk_read_ios: u64,
    pub disk_write_ios: u64,
}

/// Common storage interface used by the server and telemetry.
pub enum StorageEngine {
    Cache2(Box<Cache2Engine>),
    Foyer(FoyerEngine),
}

impl StorageEngine {
    pub async fn try_new(
        rt: &Runtime,
        config: &StorageConfig,
        metrics: Option<OpenTelemetryMetricsRegistry>,
    ) -> Result<Self, StorageError> {
        if config.disk_throttle.is_some()
            && config
                .foyer
                .as_ref()
                .is_some_and(|options| options.disk_throttle.is_some())
        {
            return Err(StorageError::InvalidConfig("set only storage.foyer.disk_throttle; the legacy storage.disk_throttle alias cannot also be set".to_string()));
        }
        match config.engine {
            StorageEngineKind::Cache2 => Ok(Self::Cache2(Box::new(
                Cache2Engine::try_new(rt, config).await?,
            ))),
            StorageEngineKind::Foyer => FoyerEngine::try_new(
                rt,
                &config.data_dir,
                config.memory_capacity.clone().into(),
                config.disk_capacity.clone().into(),
                config
                    .foyer
                    .as_ref()
                    .and_then(|options| options.disk_throttle.clone())
                    .or_else(|| config.disk_throttle.clone()),
                metrics,
            )
            .await
            .map(Self::Foyer)
            .map_err(|err| StorageError::Foyer(err.to_string())),
        }
    }

    pub async fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>, StorageError> {
        match self {
            Self::Cache2(engine) => engine.get(key).await,
            Self::Foyer(engine) => engine.get_result(key).await,
        }
    }

    pub fn put(&self, key: &[u8], value: &[u8]) -> Result<(), StorageError> {
        match self {
            Self::Cache2(engine) => engine.put(key, value),
            Self::Foyer(engine) => {
                engine.put(key, value);
                Ok(())
            }
        }
    }

    pub fn delete(&self, key: &[u8]) -> Result<(), StorageError> {
        match self {
            Self::Cache2(engine) => engine.delete(key),
            Self::Foyer(engine) => {
                engine.delete(key);
                Ok(())
            }
        }
    }

    /// Waits for queued disk writes; memory-resident Foyer entries are not flushed.
    pub async fn drain(&self) -> Result<(), StorageError> {
        match self {
            Self::Cache2(engine) => engine.drain().await,
            Self::Foyer(engine) => {
                engine.drain().await;
                Ok(())
            }
        }
    }

    pub async fn close(&self) -> Result<(), StorageError> {
        match self {
            Self::Cache2(engine) => engine.close().await,
            Self::Foyer(engine) => engine.close().await,
        }
    }

    pub fn capacity(&self) -> u64 {
        match self {
            Self::Cache2(engine) => engine.capacity(),
            Self::Foyer(engine) => engine.capacity(),
        }
    }

    pub fn statistics(&self) -> Result<StorageStatistics, StorageError> {
        match self {
            Self::Cache2(engine) => engine.statistics(),
            Self::Foyer(engine) => {
                let stats = engine.statistics();
                Ok(StorageStatistics {
                    disk_read_bytes: stats.disk_read_bytes() as u64,
                    disk_write_bytes: stats.disk_write_bytes() as u64,
                    disk_read_ios: stats.disk_read_ios() as u64,
                    disk_write_ios: stats.disk_write_ios() as u64,
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use bytesize::ByteSize;

    use super::*;

    fn config(dir: &std::path::Path, engine: StorageEngineKind) -> StorageConfig {
        StorageConfig {
            engine,
            cache2: None,
            foyer: None,
            data_dir: dir.to_path_buf(),
            disk_capacity: ByteSize::mib(256).into(),
            memory_capacity: ByteSize::gib(1).into(),
            disk_throttle: None,
        }
    }

    #[test]
    fn both_engines_support_cache_operations() {
        let rt = crate::make_runtime("storage_test", "storage_test", 2);
        rt.block_on(async {
            for kind in [StorageEngineKind::Cache2, StorageEngineKind::Foyer] {
                let dir = tempfile::tempdir().unwrap();
                let config = config(dir.path(), kind);
                let engine = StorageEngine::try_new(&rt, &config, None).await.unwrap();
                assert_eq!(engine.get(b"missing").await.unwrap(), None);
                engine.put(b"key", b"value").unwrap();
                assert_eq!(
                    engine.get(b"key").await.unwrap().as_deref(),
                    Some(b"value".as_slice())
                );
                engine.put(b"key", b"updated").unwrap();
                assert_eq!(
                    engine.get(b"key").await.unwrap().as_deref(),
                    Some(b"updated".as_slice())
                );
                engine.delete(b"key").unwrap();
                assert_eq!(engine.get(b"key").await.unwrap(), None);
                assert!(engine.capacity() <= config.disk_capacity.as_u64());
                engine.statistics().unwrap();
                engine.close().await.unwrap();
            }
        });
    }

    #[test]
    fn cache2_recovers_only_after_warm_close() {
        let rt = crate::make_runtime("recovery_test", "recovery_test", 2);
        rt.block_on(async {
            let dir = tempfile::tempdir().unwrap();
            let config = config(dir.path(), StorageEngineKind::Cache2);
            let engine = StorageEngine::try_new(&rt, &config, None).await.unwrap();
            engine.put(b"kept", b"value").unwrap();
            engine.put(b"deleted", b"value").unwrap();
            engine.close().await.unwrap();
            drop(engine);
            let engine = StorageEngine::try_new(&rt, &config, None).await.unwrap();
            assert_eq!(
                engine.get(b"kept").await.unwrap().as_deref(),
                Some(b"value".as_slice())
            );
            let stats = engine.statistics().unwrap();
            assert!(stats.disk_read_bytes > 0);
            assert!(stats.disk_read_ios > 0);
            // Delete after publication: cache2 permits stale hits from writes
            // still in flight when a best-effort delete is performed.
            engine.delete(b"deleted").unwrap();
            engine.close().await.unwrap();
            drop(engine);
            let engine = StorageEngine::try_new(&rt, &config, None).await.unwrap();
            assert_eq!(engine.get(b"deleted").await.unwrap(), None);
            drop(engine);
            let engine = StorageEngine::try_new(&rt, &config, None).await.unwrap();
            assert_eq!(engine.get(b"kept").await.unwrap(), None);
            engine.close().await.unwrap();
        });
    }

    #[test]
    fn cache2_rejects_invalid_inputs_and_budgets() {
        let rt = crate::make_runtime("validation_test", "validation_test", 2);
        rt.block_on(async {
            let dir = tempfile::tempdir().unwrap();
            let mut config = config(dir.path(), StorageEngineKind::Cache2);
            config.disk_capacity = ByteSize::mib(1).into();
            assert!(StorageEngine::try_new(&rt, &config, None).await.is_err());
            config.disk_capacity = ByteSize::mib(256).into();
            config.memory_capacity = ByteSize::mib(1).into();
            assert!(StorageEngine::try_new(&rt, &config, None).await.is_err());
            config.memory_capacity = ByteSize::gib(1).into();
            config.disk_throttle = Some(crate::newtype::DiskThrottle {
                write_iops: None,
                read_iops: None,
                write_throughput: None,
                read_throughput: None,
                iops_counter: crate::newtype::IopsCounter::PerIo,
            });
            assert!(matches!(
                StorageEngine::try_new(&rt, &config, None).await,
                Err(StorageError::InvalidConfig(_))
            ));
            config.disk_throttle = None;
            let engine = StorageEngine::try_new(&rt, &config, None).await.unwrap();
            assert!(
                engine
                    .put(&vec![b'x'; 4097], b"value")
                    .unwrap_err()
                    .is_invalid_input()
            );
            engine.close().await.unwrap();
            assert!(engine.put(b"closed", b"value").is_err());
        });
    }
}
