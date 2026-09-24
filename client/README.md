# Percas Client

[![Crates.io][crates-badge]][crates-url]
[![Documentation][docs-badge]][docs-url]
[![Apache 2.0 licensed][license-badge]][license-url]
[![Build Status][actions-badge]][actions-url]

[crates-badge]: https://img.shields.io/crates/v/percas-client.svg
[crates-url]: https://crates.io/crates/percas-client
[docs-badge]: https://docs.rs/percas-client/badge.svg
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
use percas_client::ClientBuilder;

#[tokio::main]
async fn main() -> Result<(), percas_client::Error> {
    let client = ClientBuilder::new("http://localhost:7654", "http://localhost:7655")
        .control_peer("http://localhost:7657") // optional additional control seed
        .build()?;
    client.put("example/key?with#syntax", b"example_value").await?;
    let value = client.get("example/key?with#syntax").await?;
    println!("{value:?}");
    client.delete("example/key?with#syntax").await?;
    Ok(())
}
```

Enable Tokio's `macros` and `rt-multi-thread` features for this example.
The client lazily refreshes membership in the background on the calling Tokio
runtime. Control failures do not block data operations; the last usable routing
table is retained, and additional configured or discovered control peers are
tried. The first request uses the data seed and may be redirected by the server.

Keys are encoded in `/v1/cache?key=...` rather than interpreted as URLs.
Upgrade the Percas servers before using this client. Each data request has a
five-second deadline, including when a custom HTTP client is supplied.

## License

This work is licensed by [Apache License 2.0](https://www.apache.org/licenses/LICENSE-2.0).
