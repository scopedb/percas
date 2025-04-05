use error_stack::Result;
use error_stack::ResultExt;
use error_stack::report;
use foyer::DirectFileDeviceOptions;
use foyer::HybridCache;
use foyer::HybridCacheBuilder;
use foyer::HybridCachePolicy;
use foyer::TokioRuntimeOptions;
use thiserror::Error;

use crate::config::data_path;
use crate::util::num_cpus;

#[derive(Debug, Error)]
#[error("{0}")]
pub struct EngineError(pub String);

pub struct FoyerEngine {
    inner: HybridCache<Vec<u8>, Vec<u8>>,
}

impl FoyerEngine {
    pub async fn try_new(
        path: &str,
        memory_capacity: u64,
        disk_capacity: u64,
    ) -> Result<Self, EngineError> {
        let make_error = || EngineError("failed to create foyer engine".to_string());
        std::fs::create_dir_all(path).change_context_lazy(make_error)?;

        let cache = HybridCacheBuilder::new()
            .with_policy(HybridCachePolicy::WriteOnInsertion)
            .memory(memory_capacity as usize)
            .with_shards(num_cpus())
            .storage(foyer::Engine::Large)
            .with_device_options(
                DirectFileDeviceOptions::new(data_path(path)).with_capacity(disk_capacity as usize),
            )
            .with_runtime_options(foyer::RuntimeOptions::Unified(TokioRuntimeOptions {
                worker_threads: num_cpus(),
                max_blocking_threads: num_cpus() * 2,
            }))
            .build()
            .await
            .map_err(|err| report!(EngineError(err.to_string())))?;

        Ok(FoyerEngine { inner: cache })
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
}

#[cfg(test)]
mod tests {
    use insta::assert_compact_debug_snapshot;

    use super::*;

    #[tokio::test]
    async fn test_get() {
        let temp_dir = tempfile::tempdir_in("/tmp/foyer").unwrap();

        let engine =
            FoyerEngine::try_new(temp_dir.path().to_str().unwrap(), 512 * 1024, 1024 * 1024)
                .await
                .unwrap();
        engine.put(b"foo".to_vec().as_ref(), b"bar".to_vec().as_ref());

        assert_compact_debug_snapshot!(
            engine.get(b"foo".to_vec().as_ref()).await,
            @"Some([98, 97, 114])"
        );
    }
}
