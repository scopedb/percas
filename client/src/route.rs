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

use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct RouteTable {
    ring: BTreeMap<u32, BTreeMap<Uuid, String>>,
}

impl RouteTable {
    pub fn new() -> Self {
        Self {
            ring: BTreeMap::new(),
        }
    }

    pub fn insert(&mut self, hash: u32, node_id: Uuid, addr: String) {
        match self.ring.entry(hash) {
            Entry::Vacant(entry) => {
                let mut map = BTreeMap::new();
                map.insert(node_id, addr);
                entry.insert(map);
            }
            Entry::Occupied(mut entry) => {
                entry.get_mut().insert(node_id, addr);
            }
        }
    }

    pub fn lookup(&self, key: &str) -> Option<(&Uuid, &String)> {
        let hash = murmur3::murmur3_32(&mut key.as_bytes(), 0).unwrap();

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
