# Percas: PERsistent CAche Service

Percas is a distributed persistent cache service optimized for high performance NVMe SSD. It aims to provide the capability to scale-out without pain and with stable performance.

## Getting Started

### Prerequisites

To get started with Percas, you can follow these steps:

1. **Install Rust**: Make sure you have Rust installed on your system. You can install it using [rustup](https://rustup.rs/).
2. **Clone the Repository**: Clone the Percas repository from GitHub:
   ```shell
   git clone https://github.com/scopedb/percas.git
   cd percas
   ```
3. **Build the Project**: Use Cargo to build the project:
   ```shell
   cargo x build
   ```

### One Node Cluster

To run a one node cluster, you can use the following command:

```shell
./target/debug/percas start --config-file dev/standalone/config.toml
```

This will start a one node cluster of Percas listening on `localhost:7654`.

### Distributed Cluster

Percas is a decentralized distributed cache service. Each node in the cluster operates independently without relying on a central coordinator, allowing for excellent scalability and fault tolerance.

To quickly start a simple 3-node cluster for development or testing, you can run:

```shell
./target/debug/percas start --config-file dev/cluster/config-0.toml &
./target/debug/percas start --config-file dev/cluster/config-1.toml &
./target/debug/percas start --config-file dev/cluster/config-2.toml &
```

You can interact with the cluster through any node, in this example they are `localhost:7654`, `localhost:7656` and `localhost:7658`.

Percas will automatically handle data distribution and request routing across all nodes.

### HTTP API

Percas provides a simple HTTP API for interacting with the cache. You can use any HTTP client to send requests to the cache.

Here are some examples of how to use the HTTP API (use `-L` with `curl` to follow redirects):

```shell
curl -L -X PUT http://localhost:7654/my/lovely/key -d 'my_lovely_value'
curl -L -X GET http://localhost:7654/my/lovely/key
curl -L -X DELETE http://localhost:7654/my/lovely/key
```

## License

This work is licensed by [Apache License 2.0](https://www.apache.org/licenses/LICENSE-2.0).
