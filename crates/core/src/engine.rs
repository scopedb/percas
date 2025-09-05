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

use exn::IntoExn;
use exn::Result;
use exn::bail;
use foyer::BlockEngineBuilder;
use foyer::DeviceBuilder;
use foyer::FifoConfig;
use foyer::FsDeviceBuilder;
use foyer::HybridCache;
use foyer::HybridCacheBuilder;
use foyer::HybridCachePolicy;
use foyer::IoEngineBuilder;
use foyer::PsyncIoEngineBuilder;
use foyer::RecoverMode;
use foyer::RuntimeOptions;
use thiserror::Error;

use crate::available_memory;
use crate::newtype::DiskThrottle;
use crate::num_cpus;

const DEFAULT_MEMORY_CAPACITY_FACTOR: f64 = 0.5; // 50% of available memory
const DEFAULT_BLOCK_SIZE: usize = 64 * 1024 * 1024; // 64 MiB
const DEFAULT_FLUSHERS: usize = 4; // Number of flushers for the block engine

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

        let mut db = FsDeviceBuilder::new(data_dir).with_capacity(disk_capacity as usize);
        if let Some(throttle) = disk_throttle {
            db = db.with_throttle(throttle.into());
        }
        let dev = db
            .build()
            .map_err(|err| EngineError(format!("failed to create device: {err}")))?;

        let parallelism = num_cpus().get();
        let cache = HybridCacheBuilder::new()
            .with_policy(HybridCachePolicy::WriteOnInsertion)
            .memory(
                (memory_capacity.map_or_else(|| available_memory().get(), |v| v as usize) as f64
                    * DEFAULT_MEMORY_CAPACITY_FACTOR) as usize,
            )
            .with_weighter(|key: &Vec<u8>, value: &Vec<u8>| {
                let key_size = key.len();
                let value_size = value.len();
                key_size + value_size
            })
            .with_shards(parallelism)
            .with_eviction_config(FifoConfig::default())
            .storage()
            .with_engine_config(
                BlockEngineBuilder::new(dev)
                    .with_block_size(DEFAULT_BLOCK_SIZE)
                    .with_flushers(DEFAULT_FLUSHERS),
            )
            .with_io_engine(
                PsyncIoEngineBuilder::new()
                    .build()
                    .await
                    .map_err(|err| EngineError(err.to_string()).into_exn())?,
            )
            .with_recover_mode(RecoverMode::Quiet)
            .with_runtime_options(RuntimeOptions::Unified(Default::default()))
            .build()
            .await
            .map_err(|err| EngineError(err.to_string()).into_exn())?;

        Ok(FoyerEngine {
            capacity: disk_capacity,
            inner: cache,
        })
    }

    pub async fn get(&self, key: &[u8]) -> Option<Vec<u8>> {
        self.inner
            .get(&key.to_owned())
            .await
            .map_err(|e| EngineError(e.to_string()).into_exn())
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
