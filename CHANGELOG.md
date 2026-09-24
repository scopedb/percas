# CHANGELOG

All significant changes to this project will be documented in this file.

For the changelog of the `percase-client` crate, please refer to its own [CHANGELOG](client/CHANGELOG.md).

## Unreleased

### Breaking Changes

* Use cache2 as the default storage engine, including for existing configurations
  without `storage.engine`. Set `storage.engine = "foyer"` to retain Foyer.
* Cache2 uses a separate disk format and does not migrate Foyer entries. Clean
  shutdown enables warm recovery; unclean shutdown starts empty. Cache2 provides
  best-effort cache consistency, including potentially stale reads after DELETE.
* Cache2 rejects the Foyer-only `storage.disk_throttle` option and validates disk
  and managed-memory budgets at startup.

### Improvements

* Refresh client routes in the background with bounded multi-peer discovery,
  retaining usable routes on control-plane failures and excluding dead nodes.
* Encode opaque keys in `/v1/cache?key=...`; upgrade servers before clients.
* Serialize gossip writes and publish coherent membership/ring snapshots. Ignore
  stale observations, add bounded probes and a suspect grace period, and reject
  foreign-cluster members. The new suspect wire value needs coordinated upgrades.
* Bound local HTTP concurrency and admitted upload bytes, reject oversized bodies,
  and time out stalled uploads before they consume unbounded resources.
* Separate engine-specific storage tuning, fit automatic cache2 L1 budgets around
  fixed allocations, and benchmark both engines plus cache2 warm recovery.

* Add a shared storage interface, cache2 I/O metrics, and graceful storage close.
* Return HTTP 429 for cache admission overload, 400 for invalid mutations, and
  500 for explicit storage failures.
* Upgrade workspace dependencies and pin Rust to nightly-2026-08-28. Keep
  OpenTelemetry on 0.32 to match the tracing and logging adapters.

## v0.4.0 (2025-10-12)

### Breaking Changes

* Standalone mode is removed. `server.mode` option is removed from the config file.
* `server.listen_addr` is now `server.listen_data_addr`.
* `server.advertise_addr` is now `server.advertise_data_addr`.
* `server.listen_peer_addr` is now `server.listen_ctrl_addr`
* `server.advertise_peer_addr` is now `server.advertise_ctrl_addr`.
* `server.initial_advertise_peer_addrs` is now `initial_peers` and accepts urls with scheme (e.g. `initial_peers = ["http://percas:7655"]`).
* The corresponding environment variables are also renamed accordingly.

## v0.3.0 (2025-10-07)

### New Features

* Supported redirecting request to correct node in the cluster with HTTP 307 status code.

### Improvements

* Improved cluster gossip protocol to reduce unnecessary state updates.
* Bumped dependencies to the latest versions.

## v0.2.6 (2025-09-23)

### Improvements

* Parallelized recovery for foyer by default.

## v0.2.5 (2025-09-19)

### Improvements

* Improved default configuration for throttler.

## v0.2.4 (2025-09-05)

### Improvements

* Improved rate limiter to support burst requests.
* Export `foyer` metrics.

## v0.2.3 (2025-09-05)

### Improvements

* Enable `jemalloc` as the global allocator.
* Bumped `foyer` dependency to `0.19.2`.
* Increased number of flushers to boost write throughput.

## v0.2.2 (2025-09-04)

### Improvements

* Bumped `foyer` dependency to `0.19` to leverage buffered I/O and page cache.

## v0.2.1 (2025-04-24)

### New Features

* Support setting disk io throttle:

```toml
[storage.disk_throttle]
write_iops = 1000
read_iops = 1000
write_throughput = 16_777_216 # 16 MiB per second
read_throughput = 16_777_216 # 16 MiB per second

[storage.disk_throttle.iops_counter]
mode = "per_io" # or "per_io_size"
# size = 1024 # size in bytes if mode = "per_io_size"
```

## v0.2.0 (2025-04-24)

### Breaking Changes

* `ClientBuilder` is now `ClientFactory` for reusing the underlying reqwest client.

## v0.1.4 (2025-04-23)

### Bug Fixes

* Fix the issue that percas cannot limit memory usage as expected.

## v0.1.3 (2025-04-23)

### Bug Fixes

* Fix the issue that percas node may use different advertised address than the one in the config file after restart.

## v0.1.2 (2025-04-22)

### Improvements

* Add rate limiter to reject unprocessable requests.
* Improve default configuration for foyer.

## v0.1.1 (2025-04-21)

### New Features

* Support propagate environment variables to server config.
