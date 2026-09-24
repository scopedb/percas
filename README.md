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

## Storage engines

Percas uses `cache2` by default. Select an engine in the configuration file:

```toml
[storage]
engine = "cache2" # or "foyer"
data_dir = ".percas/data"
disk_capacity = "512 MiB"
memory_capacity = "1 GiB"
```

`PERCAS_CONFIG_STORAGE_ENGINE` also selects the engine. Existing configurations
without `engine` now use cache2. Foyer remains available with `engine = "foyer"`.
The engines use separate on-disk formats; switching engines does not migrate
cached entries. Cache2 uses `cache2.data` in `data_dir`, leaving existing Foyer
files untouched. Budget disk space for both formats if retaining both.

Cache2 uses buffered POSIX I/O on its dedicated I/O runtime. `disk_capacity`
bounds data regions and recovery metadata. The default layout needs five 32 MiB
regions plus metadata. `memory_capacity` limits cache-managed memory. Automatic
L1 sizing starts at the smaller of 256 MiB and half that budget, then shrinks to
fit fixed buffers and metadata. Explicit L1 budgets are validated without silently
shrinking. Process overhead, HTTP buffers, and the OS page cache are additional.

Engine-specific tuning lives in separate configuration sections:

```toml
[storage.cache2]
region_size = "32 MiB"
append_shards = 4
l1_capacity = "128 MiB" # omit for automatic sizing; zero disables L1
```

At least `append_shards + 1` regions plus recovery metadata must fit the disk
budget. Changing region size changes the persistent layout and may start cold.
Foyer device limits belong under `[storage.foyer.disk_throttle]`. The legacy
`storage.disk_throttle` alias remains supported for Foyer; setting both forms is
an error. Selecting cache2 with the legacy throttle option remains an error.
Settings in the inactive engine's dedicated section are retained for switching
engines but do not apply to the active engine.

Writes acknowledge in-memory admission, not durable storage. Cache2 can return
misses under resource pressure and may return stale values. Values larger than
its L1 admission limit become visible after background region publication.
DELETE is best-effort: writes already in flight may publish afterward and
make a deleted value visible again, including after a warm restart.
Keys are limited to 4 KiB; an encoded key/value record must fit within the
configured region (32 MiB by default). Invalid mutations return HTTP 400 (oversized GET keys are misses),
admission overload returns 429, and
storage failures return 500.

Graceful shutdown stops HTTP requests and metric collection before calling
`close_warm`, allowing the next startup to recover cache contents. An unclean
exit starts with an empty cache. Cache2 is disposable acceleration: callers
must retain an authoritative data source and handle misses. Upgrading cache2
or changing its persistent layout may also cause a cold start.


## Routing and HTTP budgets

The Rust client uses `GET`, `PUT`, and `DELETE /v1/cache?key=<encoded-key>`.
The query field carries the exact UTF-8 key, including slashes, dot segments,
question marks, fragments, percent signs, and empty keys. `/v1/cache` is reserved
for this API. Other paths retain the legacy HTTP API. Upgrade servers before
clients: older servers do not understand the query-key API.

Route refresh is triggered by traffic but runs in the background, with one
refresh per client at a time. Reads and writes can use the bootstrap data URL
immediately and keep using the last usable route table if refresh fails. Refresh
tries configured and discovered control peers with bounded deadlines, rotating
peers between attempts. Dead members are excluded; suspect members remain
eligible during the grace period. A successful refresh is reused for ten seconds;
failed refreshes retry on subsequent traffic after one second. Individual cache
requests have a five-second deadline.

Gossip probes have a two-second HTTP deadline and a 500 ms connection deadline.
Failed probes first mark a node suspect; a subsequent failed probe after a
five-second local grace period can mark it dead. Successful contact clears local
suspicion. This is direct probing with a grace period, not an indirect-probe
protocol. The new `suspect` wire value requires coordinated server upgrades.

```toml
[server.request_limits]
max_body_bytes = 16777216
max_inflight_body_bytes = 67108864
max_concurrent_requests = 64
body_timeout_ms = 10000
```

Local data handlers share these budgets across both API routes. PUT reserves its
declared body size before reading; uploads without Content-Length reserve the
maximum body size. The read also enforces that bound. Requests beyond the
concurrency or upload budget return 429; oversized bodies return 413; stalled
uploads return 408. Reservations are released on completion or cancellation.
The upload budget bounds admitted payload bytes, not total process RSS or
response buffers. Environment overrides use the usual
`PERCAS_CONFIG_SERVER_REQUEST_LIMITS_*` names.

## Storage benchmarks

`cargo bench -p percas-core --bench benchmark` measures both engines with L1
and with disk-only reads, checks that hit benchmarks actually hit, and measures
cache2 warm opens. `put_attempt` includes overload outcomes and prints accepted
and overloaded counts separately; it is not successful-write throughput.
For a short correctness smoke test, add `--profile dev -- --test`.
