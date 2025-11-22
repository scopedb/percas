# CHANGELOG

All significant changes to the `percase-client` crate will be documented in this file.

## Unreleased

### New features

* Added `Client::put_owned` to avoid an extra copy when the caller has ownership of the data.

## v0.3.0 (2025-10-14)

### Breaking changes

* `ClientFactory` is now `ClientBuilder` with support for setting a custom HTTP client.
* `ClientBuilder::new` now requires a `data_url` and a `ctrl_url` parameter to specify the data service url and the control service url.
* This client supports only Percas server version 0.4.0 and above.

### Improvements

* `Client` now routes requests internally to improve performance in the best effort.

## v0.2.2 (2025-09-24)

This is the first standalone release of the `percas-client` crate.
