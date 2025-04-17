# Percas

Percas is a persistent cache service.

## Getting Started

To get started with Percas, you can follow these steps:

1. Build the project: `cargo x build`
2. Start a local server: `./target/debug/percas start --config-file dev/config.toml`

Now, you can put and get a key-value pair into the cache using the following command:

```shell
curl -X PUT http://localhost:7654/data/my/lovely/key -d 'my_lovely_value'
curl -X GET http://localhost:7654/data/my/lovely/key
```

## License

This work is licensed by [Apache License 2.0](https://www.apache.org/licenses/LICENSE-2.0).

We're still developing a suitable license model. So far, Apache License 2.0 fits it well. Any source code and releases delivered under the current license model can be used following Apache License 2.0 from then on.
