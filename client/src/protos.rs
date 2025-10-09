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

//! Protocol definitions for the Percas client.

use serde::Deserialize;
use serde::Serialize;

/// Version information about the Percas server.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Version {
    /// Git branch name.
    pub branch: String,
    /// Git commit hash.
    pub commit: String,
    /// Short Git commit hash.
    pub commit_short: String,
    /// Whether the build was clean.
    pub clean: bool,
    /// The source time of the build.
    pub source_time: String,
    /// The build time.
    pub build_time: String,
    /// The Rust compiler version.
    pub rustc: String,
    /// The target triple.
    pub target: String,
    /// The Percas version.
    pub version: String,
}
