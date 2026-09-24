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

use cache2::Cache;
use cache2::CacheConfig;
use cache2::RuntimeOptions;
use cache2::StorageOptions;

use crate::Runtime;
use crate::StorageConfig;
use crate::StorageError;
use crate::StorageStatistics;

/// A bounded RAM and disk cache hosted on the dedicated I/O runtime.
pub struct Cache2Engine {
    inner: Cache,
    capacity: u64,
}

impl Cache2Engine {
    pub async fn try_new(rt: &Runtime, config: &StorageConfig) -> Result<Self, StorageError> {
        if config.disk_throttle.is_some() {
            return Err(StorageError::InvalidConfig(
                "disk_throttle is only supported by the foyer engine".to_string(),
            ));
        }
        let cache_config = Self::configuration(config)?;
        let capacity = cache_config.storage().peak_disk_bytes();
        std::fs::create_dir_all(&config.data_dir).map_err(|err| {
            StorageError::InvalidConfig(format!("failed to create data directory: {err}"))
        })?;
        let inner = Cache::open_with_handle(
            config.data_dir.join("cache2.data"),
            cache_config,
            rt.handle(),
        )
        .await?;
        log::info!(
            "cache2 opened with startup mode: {:?}",
            inner.startup_mode()
        );
        Ok(Self { inner, capacity })
    }

    fn configuration(config: &StorageConfig) -> Result<CacheConfig, StorageError> {
        let options = config.cache2.clone().unwrap_or_default();
        let budget = config.disk_capacity.as_u64();
        let mut runtime = RuntimeOptions::default();
        if let Some(shards) = options.append_shards {
            runtime.append_shards = shards.get();
        }
        runtime.managed_memory_limit_bytes = usize::try_from(config.memory_capacity.as_u64())
            .map_err(|err| StorageError::InvalidConfig(err.to_string()))?;
        runtime.stats.activity_counters = true;
        let region_size = options
            .region_size
            .map_or(StorageOptions::new(budget).region_size_bytes, |size| {
                size.as_u64()
            });
        let minimum_regions = u64::from(runtime.append_shards) + 1;
        let minimum_capacity = region_size
            .checked_mul(minimum_regions)
            .ok_or_else(|| StorageError::InvalidConfig("cache2 layout overflows".to_string()))?;
        let layout = |capacity| {
            let mut options = StorageOptions::new(capacity);
            options.region_size_bytes = region_size;
            options.build()
        };
        // Validate region geometry before dividing by the region size.
        layout(minimum_capacity)?;
        let mut low = minimum_regions;
        let mut high = budget / region_size;
        let mut storage = None;
        while low <= high {
            let regions = low + (high - low) / 2;
            let candidate = layout(regions * region_size)?;
            if candidate.peak_disk_bytes() <= budget {
                storage = Some(candidate);
                low = regions + 1;
            } else {
                high = regions - 1;
            }
        }
        let storage = storage.ok_or_else(|| {
            StorageError::InvalidConfig(
                "cache2 disk budget must fit append_shards + 1 regions and recovery metadata"
                    .to_string(),
            )
        })?;
        if let Some(capacity) = options.l1_capacity {
            runtime.l1_capacity_bytes = usize::try_from(capacity.as_u64())
                .map_err(|err| StorageError::InvalidConfig(err.to_string()))?;
            return Ok(CacheConfig::new(storage, runtime)?);
        }
        // Fit L1 around fixed engine allocations instead of assuming that half
        // the managed budget is always enough for buffers and index metadata.
        let mut high = runtime
            .l1_capacity_bytes
            .min(runtime.managed_memory_limit_bytes / 2);
        runtime.l1_capacity_bytes = 0;
        let mut best = CacheConfig::new(storage.clone(), runtime.clone())?;
        let mut low = 1;
        while low <= high {
            let capacity = low + (high - low) / 2;
            runtime.l1_capacity_bytes = capacity;
            match CacheConfig::new(storage.clone(), runtime.clone()) {
                Ok(config) => {
                    best = config;
                    low = capacity + 1;
                }
                Err(_) => {
                    high = capacity - 1;
                }
            }
        }
        Ok(best)
    }

    pub async fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>, StorageError> {
        Ok(self.inner.get(key).await?.map(|value| value.to_vec()))
    }

    pub fn put(&self, key: &[u8], value: &[u8]) -> Result<(), StorageError> {
        self.inner.put(key, value)?;
        Ok(())
    }

    pub fn delete(&self, key: &[u8]) -> Result<(), StorageError> {
        self.inner.delete(key)?;
        Ok(())
    }

    /// Waits for accepted writes to publish without creating a recovery image.
    pub async fn drain(&self) -> Result<(), StorageError> {
        self.inner.drain().await?;
        Ok(())
    }

    pub async fn close(&self) -> Result<(), StorageError> {
        self.inner.close_warm().await?;
        Ok(())
    }

    pub fn capacity(&self) -> u64 {
        self.capacity
    }

    pub fn statistics(&self) -> Result<StorageStatistics, StorageError> {
        let io = self.inner.snapshot()?.io;
        Ok(StorageStatistics {
            disk_read_bytes: io.read.buffered.bytes + io.read.direct.bytes,
            disk_write_bytes: io.write.buffered.bytes + io.write.direct.bytes,
            disk_read_ios: io.read.buffered.operations + io.read.direct.operations,
            disk_write_ios: io.write.buffered.operations + io.write.direct.operations,
        })
    }
}

#[cfg(test)]
mod tests {
    use bytesize::ByteSize;

    use super::*;
    use crate::Cache2Options;

    #[test]
    fn auto_l1_fits_fixed_allocations_and_explicit_options_are_validated() {
        let mut config = crate::Config::default().storage;
        config.memory_capacity = ByteSize::mib(512).into();
        let automatic = Cache2Engine::configuration(&config).unwrap();
        assert!(automatic.minimum_memory_bytes() <= config.memory_capacity.as_u64() as usize);
        assert!(automatic.runtime().l1_capacity_bytes < ByteSize::mib(256).as_u64() as usize);
        config.cache2 = Some(Cache2Options {
            region_size: Some(ByteSize::mib(8).into()),
            append_shards: Some(2.try_into().unwrap()),
            l1_capacity: Some(ByteSize::mib(32).into()),
        });
        let explicit = Cache2Engine::configuration(&config).unwrap();
        assert_eq!(
            explicit.storage().region_size_bytes(),
            ByteSize::mib(8).as_u64()
        );
        assert_eq!(explicit.runtime().append_shards, 2);
        assert_eq!(
            explicit.runtime().l1_capacity_bytes,
            ByteSize::mib(32).as_u64() as usize
        );
        config.cache2.as_mut().unwrap().region_size = Some(ByteSize::b(0).into());
        assert!(Cache2Engine::configuration(&config).is_err());
        config.cache2.as_mut().unwrap().region_size = Some(ByteSize::mib(8).into());
        config.cache2.as_mut().unwrap().l1_capacity = Some(ByteSize::gib(2).into());
        assert!(Cache2Engine::configuration(&config).is_err());
    }
}
