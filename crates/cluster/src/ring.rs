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
use std::collections::BTreeSet;

use sha2::Digest;
use sha2::Sha256;
use sha2::digest::OutputSizeUser;
use sha2::digest::generic_array::GenericArray;

const DEFAULT_REPLICA_COUNT: usize = 100;

type Sha256Digest = GenericArray<u8, <Sha256 as OutputSizeUser>::OutputSize>;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Ring<T> {
    replica_count: usize,
    nodes: BTreeMap<Sha256Digest, BTreeSet<T>>,
}

impl<T> Default for Ring<T>
where
    T: Clone + AsRef<[u8]> + Ord,
{
    fn default() -> Self {
        Self {
            replica_count: DEFAULT_REPLICA_COUNT,
            nodes: BTreeMap::new(),
        }
    }
}

impl<I, T> From<I> for Ring<T>
where
    I: IntoIterator<Item = T>,
    T: Clone + AsRef<[u8]> + Ord,
{
    fn from(iter: I) -> Self {
        let mut ring = Self::default();
        for node in iter {
            ring.add_node(node);
        }
        ring
    }
}

impl<T> Ring<T>
where
    T: Clone + AsRef<[u8]> + Ord,
{
    pub fn lookup<K>(&self, key: K) -> Vec<T>
    where
        K: AsRef<[u8]>,
    {
        let mut hasher = Sha256::new();
        hasher.update(key);
        let digest = hasher.finalize();
        self.nodes
            .range(..=digest)
            .last()
            .map(|(_, node)| node.iter().cloned().collect::<Vec<T>>())
            .unwrap_or_else(|| {
                self.nodes
                    .iter()
                    .next()
                    .map(|(_, node)| node.iter().cloned().collect::<Vec<T>>())
                    .unwrap_or_default()
            })
    }

    pub fn add_node(&mut self, node: T) {
        for i in 0..self.replica_count {
            let mut hasher = Sha256::new();
            hasher.update(node.as_ref());
            hasher.update(i.to_string());
            let digest = hasher.finalize();
            self.nodes.entry(digest).or_default().insert(node.clone());
        }
    }
}
