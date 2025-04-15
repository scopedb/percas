#![feature(random)]

use atrium_core::FoyerEngine;
use criterion::BenchmarkId;
use criterion::Criterion;
use criterion::black_box;
use criterion::criterion_group;
use criterion::criterion_main;
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
            FoyerEngine::try_new(dir.path(), 0, 4 * 1024 * 1024 * 1024)
                .await
                .unwrap()
        });
        [4 * 1024, 16 * 1024, 256 * 1024]
            .into_iter()
            .for_each(|len| {
                let payload = gen_payload(len);
                c.bench_with_input(
                    BenchmarkId::new("put", humansize::format_size(len, humansize::BINARY)),
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
                    FoyerEngine::try_new(dir.path(), 0, 4 * 1024 * 1024 * 1024)
                        .await
                        .unwrap()
                });
                let payload = gen_payload(len);
                let keys = (0..1000).map(|_| gen_key(32)).collect::<Vec<_>>();
                keys.iter().for_each(|key| {
                    engine.put(key, &payload);
                });
                c.bench_with_input(
                    BenchmarkId::new("get", humansize::format_size(len, humansize::BINARY)),
                    &keys,
                    |b, s| {
                        b.to_async(&runtime).iter(|| async {
                            let key = &s[std::random::random::<usize>() % keys.len()];
                            black_box(engine.get(key).await);
                        })
                    },
                );
            });
    }
}

fn gen_key(len: usize) -> Vec<u8> {
    (0..len).map(|_| std::random::random::<u8>()).collect()
}

fn gen_payload(len: usize) -> Vec<u8> {
    vec![0x11; len]
}
