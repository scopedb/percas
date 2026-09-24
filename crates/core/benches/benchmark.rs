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

use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;
use std::time::Instant;

use bytesize::ByteSize;
use criterion::BenchmarkId;
use criterion::Criterion;
use criterion::Throughput;
use criterion::criterion_group;
use criterion::criterion_main;
use percas_core::Cache2Options;
use percas_core::StorageEngine;
use percas_core::StorageEngineKind;

criterion_group!(benches, storage_engines);
criterion_main!(benches);

fn storage_engines(c: &mut Criterion) {
    let runtime = percas_core::make_runtime("benchmark", "benchmark", 4);
    for kind in [StorageEngineKind::Cache2, StorageEngineKind::Foyer] {
        for disk_only in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let mut config = percas_core::Config::default().storage;
            config.engine = kind;
            config.data_dir = dir.path().to_path_buf();
            config.disk_capacity = ByteSize::mib(512).into();
            config.memory_capacity = ByteSize::gib(1).into();
            if disk_only {
                match kind {
                    StorageEngineKind::Cache2 => {
                        config.cache2 = Some(Cache2Options {
                            l1_capacity: Some(ByteSize::b(0).into()),
                            ..Default::default()
                        })
                    }
                    StorageEngineKind::Foyer => config.memory_capacity = ByteSize::b(0).into(),
                }
            }
            let engine = runtime
                .block_on(StorageEngine::try_new(&runtime, &config, None))
                .unwrap();
            let mode = if disk_only { "disk" } else { "memory" };
            let mut group = c.benchmark_group(format!("{kind:?}/{mode}"));
            for size in [4096, 16384] {
                let payload = vec![0x11; size];
                let key = format!("hit-{size}");
                engine.put(key.as_bytes(), &payload).unwrap();
                runtime.block_on(engine.drain()).unwrap();
                assert_eq!(
                    runtime.block_on(engine.get(key.as_bytes())).unwrap(),
                    Some(payload.clone())
                );
                group.throughput(Throughput::Bytes(size as u64));
                group.bench_function(BenchmarkId::new("get_hit", size), |b| {
                    b.iter(|| {
                        let value = runtime
                            .block_on(engine.get(key.as_bytes()))
                            .unwrap()
                            .expect("hit benchmark must not measure misses");
                        std::hint::black_box(value);
                    });
                });
                let accepted = AtomicU64::new(0);
                let overloaded = AtomicU64::new(0);
                // Admission attempts include overload outcomes; report both
                // counts rather than presenting rejected puts as throughput.
                group.bench_function(BenchmarkId::new("put_attempt", size), |b| {
                    b.iter(|| match engine.put(b"write-key", &payload) {
                        Ok(()) => {
                            accepted.fetch_add(1, Ordering::Relaxed);
                        }
                        Err(err) if err.is_overloaded() => {
                            overloaded.fetch_add(1, Ordering::Relaxed);
                        }
                        Err(err) => panic!("{err}"),
                    });
                });
                eprintln!(
                    "{kind:?}/{mode}/{size}: accepted={}, overloaded={}",
                    accepted.load(Ordering::Relaxed),
                    overloaded.load(Ordering::Relaxed)
                );
                runtime.block_on(engine.drain()).unwrap();
            }
            group.finish();
            runtime.block_on(engine.close()).unwrap();
        }
    }

    let dir = tempfile::tempdir().unwrap();
    let mut config = percas_core::Config::default().storage;
    config.data_dir = dir.path().to_path_buf();
    config.memory_capacity = ByteSize::gib(1).into();
    let mut engine = runtime
        .block_on(StorageEngine::try_new(&runtime, &config, None))
        .unwrap();
    engine.put(b"warm", b"value").unwrap();
    c.bench_function("Cache2/warm_open", |b| {
        b.iter_custom(|iterations| {
            let mut elapsed = std::time::Duration::ZERO;
            for _ in 0..iterations {
                runtime.block_on(engine.close()).unwrap();
                let started = Instant::now();
                engine = runtime
                    .block_on(StorageEngine::try_new(&runtime, &config, None))
                    .unwrap();
                elapsed += started.elapsed();
                assert_eq!(
                    runtime.block_on(engine.get(b"warm")).unwrap(),
                    Some(b"value".to_vec())
                );
            }
            elapsed
        });
    });
    runtime.block_on(engine.close()).unwrap();
}
