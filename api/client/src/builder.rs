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

use crate::client::Client;

pub struct ClientBuilder {
    endpoint: String,
}

impl ClientBuilder {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }

    pub fn build(self) -> Client {
        let builder = reqwest::ClientBuilder::new().no_proxy();
        // FIXME(tisonkun): fallible over unwrap
        Client::new(self.endpoint, builder).unwrap()
    }
}
