# CHANGELOG

All significant changes to the `percase-client` crate will be documented in this file.

## Unreleased

## v0.3.0 (2025-10-08)

### Breaking changes

* `ClientFactory` is now `ClientBuilder` with support for setting a custom HTTP client and peer server address.
* `ClientBuilder::new` now requires a `addr` parameter to specify the data server address.
* This client supports only Percas server version 0.3.0 and above.

### Improvements

* `Client` now routes requests internally to optionally improve performance.

## v0.2.2 (2025-09-24)

This is the first standalone release of the `percas-client` crate.
