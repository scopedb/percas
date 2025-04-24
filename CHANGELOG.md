# CHANGELOG

All significant changes to this project will be documented in this file.

## Unreleased

## v0.2.0 (2025-04-24)

### Breaking Changes

* `ClientBuilder` is now `ClientFactory` for reusing the underlying reqwest client.

## v0.1.4 (2025-04-23)

### Bug Fixes

* Fixed the issue that percas cannot limit memory usage as expected.

## v0.1.3 (2025-04-23)

### Bug Fixes

* Fixed the issue that percas node may use different advertised address than the one in the config file after restart.

## v0.1.2 (2025-04-22)

### Improvements

* Added rate limiter to reject unprocessable requests.
* Improved default configuration for foyer.

## v0.1.1 (2025-04-21)

### New Features

* Added support for propagate environment variables to server config.
