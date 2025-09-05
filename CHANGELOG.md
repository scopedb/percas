# CHANGELOG

All significant changes to this project will be documented in this file.

## Unreleased

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
