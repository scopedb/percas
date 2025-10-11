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

#![feature(random)]

use criterion::BenchmarkId;
use criterion::Criterion;
use criterion::criterion_group;
use criterion::criterion_main;
use percas_core::FoyerEngine;
use rand::Rng;
use tempfile::tempdir_in;

criterion_group!(benches, foyer_engine);
criterion_main!(benches);

fn foyer_engine(c: &mut Criterion) {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    {
        let dir = tempdir_in("/tmp").unwrap();
        let engine = runtime.block_on(async {
            FoyerEngine::try_new(dir.path(), Some(0), 4 * 1024 * 1024 * 1024, None, None)
                .await
                .unwrap()
        });
        [4 * 1024, 16 * 1024, 256 * 1024]
            .into_iter()
            .for_each(|len| {
                let payload = gen_payload(len);
                c.bench_with_input(
                    BenchmarkId::new("put", bytesize::ByteSize::b(len as u64)),
                    &payload,
                    |b, s| {
                        b.to_async(&runtime).iter(|| async {
                            let key = gen_key(32);
                            engine.put(&key, s);
                        })
                    },
                );
            });
    }

    {
        [4 * 1024, 16 * 1024, 256 * 1024]
            .into_iter()
            .for_each(|len| {
                let dir = tempdir_in("/tmp").unwrap();
                let engine = runtime.block_on(async {
                    FoyerEngine::try_new(dir.path(), Some(0), 4 * 1024 * 1024 * 1024, None, None)
                        .await
                        .unwrap()
                });
                let payload = gen_payload(len);
                let keys = (0..1000).map(|_| gen_key(32)).collect::<Vec<_>>();
                keys.iter().for_each(|key| {
                    engine.put(key, &payload);
                });
                c.bench_with_input(
                    BenchmarkId::new("get", bytesize::ByteSize::b(len as u64)),
                    &keys,
                    |b, s| {
                        b.to_async(&runtime).iter(|| async {
                            let key = &s[rand::rng().random_range(0..keys.len())];
                            std::hint::black_box(engine.get(key).await);
                        })
                    },
                );
            });
    }
}

fn gen_key(len: usize) -> Vec<u8> {
    (0..len).map(|_| rand::rng().random()).collect()
}

fn gen_payload(len: usize) -> Vec<u8> {
    vec![0x11; len]
}
