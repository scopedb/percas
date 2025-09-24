# How to release ...

## `percas-client`

Releasing the `percas-client` crate involves the following steps:

1. Change the working directory to the `client` folder.
2. Execute `cargo release [LEVEL] -x` for release. The LEVEL is typically one of `patch`, `minor`, or `major`.

## `percas`

Releasing the `percas` server and deliver artifacts involves the following steps:

1. Head to the [Cargo.toml](Cargo.toml) file and bump the version number. The version number should follow the [Semantic Versioning](https://semver.org/) specification.
2. After that, assuming the new version is `${version}`, run the following command to create a new release:

```shell
git tag -s -m "release v${version}" v${version}
git push origin v${version}
```
