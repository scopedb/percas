# Percas

Percas is a persistent cache service.

## Getting Started

To get started with Percas, you can follow these steps:

1. Build the project: `cargo x build`
2. Start a standalone server: `./target/debug/percas start --config-file dev/standalone/config.toml`

Now, you can put and get a key-value pair into the cache using the following command:

```shell
curl -X PUT http://localhost:7654/my/lovely/key -d 'my_lovely_value'
curl -X GET http://localhost:7654/my/lovely/key
```

## Cluster Mode

Percas is a decentralized distributed cache service.

Each node in the cluster operates independently without relying on a central coordinator, allowing for excellent scalability and fault tolerance.

To quickly start a simple 3-node cluster for development or testing, you can run:

```shell
./target/debug/percas start --config-file dev/cluster/config-0.toml >dev/cluster/node-0.log 2>&1 &
./target/debug/percas start --config-file dev/cluster/config-1.toml >dev/cluster/node-1.log 2>&1 &
./target/debug/percas start --config-file dev/cluster/config-2.toml >dev/cluster/node-2.log 2>&1 &
```

You can interact with the cluster through any node. Percas will automatically handle data distribution and request routing across all nodes.

## License

This work is licensed by [Apache License 2.0](https://www.apache.org/licenses/LICENSE-2.0).

We're still developing a suitable license model. So far, Apache License 2.0 fits it well. Any source code and releases delivered under the current license model can be used following Apache License 2.0 from then on.
