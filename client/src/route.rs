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

use std::collections::BTreeMap;
use std::collections::btree_map::Entry;

use reqwest::Url;
use uuid::Uuid;

#[derive(Default, Debug, Clone)]
pub(crate) struct RouteTable {
    ring: BTreeMap<u32, BTreeMap<Uuid, Url>>,
}

impl RouteTable {
    pub(crate) fn insert(&mut self, hash: u32, node_id: Uuid, url: Url) {
        match self.ring.entry(hash) {
            Entry::Vacant(entry) => {
                let mut map = BTreeMap::new();
                map.insert(node_id, url);
                entry.insert(map);
            }
            Entry::Occupied(mut entry) => {
                entry.get_mut().insert(node_id, url);
            }
        }
    }

    pub(crate) fn lookup(&self, key: &str) -> Option<(&Uuid, &Url)> {
        let hash = mur3::murmurhash3_x86_32(key.as_bytes(), 0);
        self.ring
            .range(hash..)
            .next()
            .and_then(|(_, nodes)| nodes.iter().next())
            .or_else(|| {
                self.ring
                    .iter()
                    .next()
                    .and_then(|(_, nodes)| nodes.iter().next())
            })
    }
}
