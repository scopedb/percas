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

use bytesize::ByteSize;
use exn::Result;
use exn::bail;
use foyer::BlockEngineConfig;
use foyer::DeviceBuilder;
use foyer::FsDeviceBuilder;
use foyer::HybridCache;
use foyer::HybridCacheBuilder;
use foyer::HybridCachePolicy;
use foyer::IoEngineConfig;
use foyer::IopsCounter;
use foyer::LfuConfig;
use foyer::RecoverMode;
use foyer::Spawner;
use mixtrics::registry::noop::NoopMetricsRegistry;
use mixtrics::registry::opentelemetry_0_31::OpenTelemetryMetricsRegistry;
use parse_display::Display;

use crate::newtype::DiskThrottle;
use crate::num_cpus;
use crate::runtime;

const DEFAULT_MEMORY_CAPACITY_FACTOR: f64 = 0.5; // 50% of available memory
const DEFAULT_BLOCK_SIZE: ByteSize = ByteSize::mib(64);
const DEFAULT_FLUSHERS: usize = 4; // Number of flushers for the block engine

#[derive(Debug, Display)]
pub struct EngineError(String);

impl std::error::Error for EngineError {}

pub struct FoyerEngine {
    inner: HybridCache<Vec<u8>, Vec<u8>>,
    capacity: ByteSize,
}

impl FoyerEngine {
    pub async fn try_new(
        io_runtime: &runtime::Runtime,
        data_dir: &Path,
        memory_capacity: ByteSize,
        disk_capacity: ByteSize,
        disk_throttle: Option<DiskThrottle>,
        metrics_registry: Option<OpenTelemetryMetricsRegistry>,
    ) -> Result<Self, EngineError> {
        let _ = std::fs::create_dir_all(data_dir);
        if !data_dir.exists() {
            bail!(EngineError(format!(
                "failed to create data dir: {}",
                data_dir.display()
            )));
        }

        let mut db = FsDeviceBuilder::new(data_dir).with_capacity(disk_capacity.0 as usize);
        if let Some(throttle) = disk_throttle {
            db = db.with_throttle(throttle.into());
        } else {
            const DEFAULT_THROUGHPUT_PER_CORE: usize = 187_500_000; // ~1.5Gbps
            const IOPS_PER_CORE: usize = 10_000; // 10k IOPS
            let throughput = DEFAULT_THROUGHPUT_PER_CORE * num_cpus().get();
            let write_throughput_quota = throughput / 4; // 25% of throughput for writes
            let read_throughput_quota = throughput - write_throughput_quota; // Remaining for reads
            let iops = IOPS_PER_CORE * num_cpus().get();
            let write_iops_quota = iops / 4; // 25% of IOPS for writes
            let read_iops_quota = iops - write_iops_quota; // Remaining for reads
            let throttle = foyer::Throttle {
                write_iops: Some(write_iops_quota.try_into().unwrap()),
                read_iops: Some(read_iops_quota.try_into().unwrap()),
                write_throughput: Some(write_throughput_quota.try_into().unwrap()),
                read_throughput: Some(read_throughput_quota.try_into().unwrap()),
                iops_counter: IopsCounter::PerIo,
            };
            db = db.with_throttle(throttle);
        }
        let dev = db
            .build()
            .map_err(|err| EngineError(format!("failed to create device: {err}")))?;

        let io_engine: Box<dyn IoEngineConfig> = {
            #[cfg(target_os = "linux")]
            {
                use foyer::UringIoEngineConfig;
                Box::new(UringIoEngineConfig::new().with_sqpoll(true))
            }

            #[cfg(not(target_os = "linux"))]
            {
                use foyer::PsyncIoEngineConfig;
                Box::new(PsyncIoEngineConfig::new())
            }
        };

        let parallelism = num_cpus().get();
        let cache = HybridCacheBuilder::new()
            .with_policy(HybridCachePolicy::WriteOnEviction)
            .with_metrics_registry(match metrics_registry {
                Some(registry) => Box::new(registry),
                None => Box::new(NoopMetricsRegistry),
            })
            .memory((memory_capacity.0 as f64 * DEFAULT_MEMORY_CAPACITY_FACTOR) as usize)
            .with_weighter(|key: &Vec<u8>, value: &Vec<u8>| {
                let key_size = key.len();
                let value_size = value.len();
                key_size + value_size
            })
            .with_shards(parallelism.max(32))
            .with_eviction_config(LfuConfig::default())
            .storage()
            .with_engine_config(
                BlockEngineConfig::new(dev)
                    .with_recover_concurrency(parallelism)
                    .with_block_size(DEFAULT_BLOCK_SIZE.0 as usize)
                    .with_flushers(DEFAULT_FLUSHERS),
            )
            .with_io_engine_config(io_engine)
            .with_recover_mode(RecoverMode::Quiet)
            .with_spawner(io_runtime.spawn_blocking(Spawner::current).await)
            .build()
            .await
            .map_err(|err| EngineError(err.to_string()))?;

        Ok(FoyerEngine {
            inner: cache,
            capacity: disk_capacity,
        })
    }

    /// Get a value from the engine by key.
    pub async fn get(&self, key: &[u8]) -> Option<Vec<u8>> {
        if let Ok(Some(value)) = self.inner.get(&key.to_owned()).await {
            Some(value.value().clone())
        } else {
            None
        }
    }

    /// Put a key-value pair into the engine.
    pub fn put(&self, key: &[u8], value: &[u8]) {
        self.inner.insert(key.to_owned(), value.to_owned());
    }

    /// Delete a key-value pair from the engine by key.
    pub fn delete(&self, key: &[u8]) {
        self.inner.remove(key);
    }

    /// Return the disk capacity of the engine in bytes.
    pub fn capacity(&self) -> u64 {
        self.capacity.as_u64()
    }

    pub fn statistics(&self) -> &Arc<foyer::Statistics> {
        self.inner.statistics()
    }
}

#[cfg(test)]
mod tests {
    use insta::assert_compact_debug_snapshot;

    use super::*;

    #[test]
    fn test_get() {
        let runtime = runtime::make_runtime("test_runtime", "test_thread", 2);

        runtime.block_on(async {
            let temp_dir = tempfile::tempdir().unwrap();

            let engine = FoyerEngine::try_new(
                &runtime,
                temp_dir.path(),
                ByteSize::kib(512),
                ByteSize::mib(1),
                None,
                None,
            )
            .await
            .unwrap();

            engine.put(b"foo".to_vec().as_ref(), b"bar".to_vec().as_ref());

            assert_compact_debug_snapshot!(
                engine.get(b"foo".to_vec().as_ref()).await,
                @"Some([98, 97, 114])"
            );
        });
    }
}
