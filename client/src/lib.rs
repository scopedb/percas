// Copyright 2025 ScopeDB <contact@scopedb.io>
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! A client library for interacting with a Percas cluster.

#![deny(missing_docs)]

mod client;
mod route;

pub use self::client::Client;
pub use self::client::ClientBuilder;

/// Errors that can occur when using the client.
#[derive(Debug)]
pub enum Error {
    /// The server responded with a "429 Too Many Requests" status code.
    TooManyRequests,
    /// An opaque error message from the server.
    Opaque(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::TooManyRequests => write!(f, "Too many requests"),
            Error::Opaque(msg) => write!(f, "{msg}"),
        }
    }
}

impl std::error::Error for Error {}
