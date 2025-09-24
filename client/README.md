# Percas Client

[![Crates.io][crates-badge]][crates-url]
[![Documentation][docs-badge]][docs-url]
[![MSRV 1.80][msrv-badge]](https://www.whatrustisit.com)
[![Apache 2.0 licensed][license-badge]][license-url]
[![Build Status][actions-badge]][actions-url]

[crates-badge]: https://img.shields.io/crates/v/percas-client.svg
[crates-url]: https://crates.io/crates/percas-client
[docs-badge]: https://docs.rs/percas-client/badge.svg
[msrv-badge]: https://img.shields.io/badge/MSRV-1.80-green?logo=rust
[docs-url]: https://docs.rs/percas-client
[license-badge]: https://img.shields.io/crates/l/percas-client
[license-url]: LICENSE
[actions-badge]: https://github.com/scopedb/percas/workflows/CI/badge.svg
[actions-url]:https://github.com/scopedb/percas/actions?query=workflow%3ACI

This crate provides a client for interacting with the Percas cache service.

## Getting Started

Add `percas-client` to your `Cargo.toml`:

```shell
cargo add percas-client
```

Create a client instance and connect to the Percas service:

```rust
fn main() {
    let server_addr = "...";
    let factory = ClientFactory::new().unwrap();
    let client = factory.make_client(server_addr).unwrap();

    runtime.block_on(async move {
        let key = "example_key";
        let value = "example_value";
        client.put(key, value.as_bytes()).await.unwrap();
        let value = testkit.client.get(key).await.unwrap().unwrap();
        client.delete(key).await.unwrap();
    });
}
```

## License

This work is licensed by [Apache License 2.0](https://www.apache.org/licenses/LICENSE-2.0).
