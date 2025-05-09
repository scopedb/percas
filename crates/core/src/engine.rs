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
use std::sync::Arc;

use error_stack::Result;
use error_stack::bail;
use error_stack::report;
use foyer::DirectFsDeviceOptions;
use foyer::FifoConfig;
use foyer::HybridCache;
use foyer::HybridCacheBuilder;
use foyer::HybridCachePolicy;
use foyer::RecoverMode;
use foyer::RuntimeOptions;
use sysinfo::Pid;
use thiserror::Error;

use crate::newtype::DiskThrottle;
use crate::num_cpus;

const DEFAULT_MEMORY_CAPACITY_FACTOR: f64 = 0.8;

#[derive(Debug, Error)]
#[error("{0}")]
pub struct EngineError(pub String);

pub struct FoyerEngine {
    capacity: u64,
    inner: HybridCache<Vec<u8>, Vec<u8>>,
}

impl FoyerEngine {
    pub async fn try_new(
        data_dir: &Path,
        memory_capacity: Option<u64>,
        disk_capacity: u64,
        disk_throttle: Option<DiskThrottle>,
    ) -> Result<Self, EngineError> {
        let _ = std::fs::create_dir_all(data_dir);
        if !data_dir.exists() {
            bail!(EngineError(format!(
                "failed to create data dir: {}",
                data_dir.display()
            )));
        }

        let mut dev = DirectFsDeviceOptions::new(data_dir)
            .with_capacity(disk_capacity as usize)
            .with_file_size(64 * 1024 * 1024);
        if let Some(throttle) = disk_throttle {
            dev = dev.with_throttle(throttle.into());
        }

        let parallelism = num_cpus().get();
        let cache = HybridCacheBuilder::new()
            .with_policy(HybridCachePolicy::WriteOnInsertion)
            .memory(
                (memory_capacity.map_or_else(
                    || {
                        let s = sysinfo::System::new_all();
                        s.process(Pid::from_u32(std::process::id()))
                            .unwrap()
                            .memory() as usize
                    },
                    |v| v as usize,
                ) as f64
                    * DEFAULT_MEMORY_CAPACITY_FACTOR) as usize,
            )
            .with_weighter(|key: &Vec<u8>, value: &Vec<u8>| {
                let key_size = key.len();
                let value_size = value.len();
                key_size + value_size
            })
            .with_shards(parallelism)
            .with_eviction_config(FifoConfig::default())
            .storage(foyer::Engine::Large)
            .with_device_options(dev)
            .with_recover_mode(RecoverMode::Quiet)
            .with_runtime_options(RuntimeOptions::Unified(Default::default()))
            .build()
            .await
            .map_err(|err| report!(EngineError(err.to_string())))?;

        Ok(FoyerEngine {
            capacity: disk_capacity,
            inner: cache,
        })
    }

    pub async fn get(&self, key: &[u8]) -> Option<Vec<u8>> {
        self.inner
            .get(&key.to_owned())
            .await
            .map_err(|e| report!(EngineError(e.to_string())))
            .ok()
            .flatten()
            .map(|v| v.value().clone())
    }

    pub fn put(&self, key: &[u8], value: &[u8]) {
        self.inner.insert(key.to_owned(), value.to_owned());
    }

    pub fn delete(&self, key: &[u8]) {
        self.inner.remove(key);
    }

    pub fn capacity(&self) -> u64 {
        self.capacity
    }

    pub fn statistics(&self) -> &Arc<foyer::Statistics> {
        self.inner.statistics()
    }
}

#[cfg(test)]
mod tests {
    use insta::assert_compact_debug_snapshot;

    use super::*;

    #[tokio::test]
    async fn test_get() {
        let temp_dir = tempfile::tempdir().unwrap();

        let engine = FoyerEngine::try_new(temp_dir.path(), Some(512 * 1024), 1024 * 1024, None)
            .await
            .unwrap();
        engine.put(b"foo".to_vec().as_ref(), b"bar".to_vec().as_ref());

        assert_compact_debug_snapshot!(
            engine.get(b"foo".to_vec().as_ref()).await,
            @"Some([98, 97, 114])"
        );
    }
}
