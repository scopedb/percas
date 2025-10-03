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
use std::fmt::Debug;

const DEFAULT_VNODE_COUNT: usize = 64;

/// A consistent hash ring implementation.
/// This implementation uses MurmurHash3 to hash the nodes and keys.
/// It supports virtual nodes to improve load balancing, every added node
/// will be replicated `vnodes` times in the ring.
///
/// # Examples
///
/// ```
/// use percas_cluster::HashRing;
///
/// let ring = HashRing::from(["node-1", "node-2", "node-3"]);
/// assert_eq!(ring.lookup("key1"), Some("node-1"));
/// assert_eq!(ring.lookup("key2"), Some("node-2"));
/// assert_eq!(ring.lookup("key3"), Some("node-2"));
/// ```
pub struct HashRing<T> {
    vnodes: usize,
    nodes: BTreeMap<u32, BTreeSet<T>>,
}

impl<T> Default for HashRing<T>
where
    T: Clone + AsRef<[u8]> + Ord,
{
    fn default() -> Self {
        Self {
            vnodes: DEFAULT_VNODE_COUNT,
            nodes: BTreeMap::new(),
        }
    }
}

impl<T> Debug for HashRing<T>
where
    T: Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HashRing")
            .field("vnodes", &self.vnodes)
            .field("nodes", &self.nodes)
            .finish()
    }
}

impl<T> HashRing<T> {
    /// Creates a new `HashRing` with the specified vnodes count.
    pub fn new(vnodes: usize) -> Self {
        Self {
            vnodes,
            nodes: BTreeMap::new(),
        }
    }
}

impl<I, T> From<I> for HashRing<T>
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

impl<T> HashRing<T>
where
    T: Clone + AsRef<[u8]> + Ord,
{
    /// Lookups the node responsible for the given key.
    pub fn lookup<K>(&self, key: K) -> Option<T>
    where
        K: AsRef<[u8]>,
    {
        let digest = self.hash_key(key.as_ref());
        self.nodes
            .range(digest..)
            .next()
            .and_then(|(_, node)| node.iter().next().cloned())
            .or_else(|| {
                self.nodes
                    .iter()
                    .next()
                    .and_then(|(_, node)| node.iter().next().cloned())
            })
    }

    /// Lookups the node responsible for the given key that satisfies the predicate.
    pub fn lookup_until<K, F>(&self, key: K, predicate: F) -> Option<T>
    where
        K: AsRef<[u8]>,
        F: Fn(&T) -> bool,
    {
        let hash = self.hash_key(key.as_ref());
        self.nodes
            .range(hash..)
            .find_map(|(_, node)| node.iter().find(|v| predicate(*v)).cloned())
            .or_else(|| {
                self.nodes
                    .range(..=hash)
                    .find_map(|(_, node)| node.iter().find(|v| predicate(*v)).cloned())
            })
    }

    /// Lists all virtual nodes (hashes) assigned to the given node.
    pub fn list_vnodes(&self, node: &T) -> impl Iterator<Item = u32> {
        self.nodes.iter().filter_map(|(hash, nodes)| {
            if nodes.contains(node) {
                Some(*hash)
            } else {
                None
            }
        })
    }

    /// Adds a node to the ring.
    /// The node will be replicated `replica_count` times in the ring.
    pub fn add_node(&mut self, node: T) {
        for i in 0..self.vnodes {
            let hash = self.hash_node(&node, i);
            self.nodes.entry(hash).or_default().insert(node.clone());
        }
    }

    fn hash_key(&self, key: &[u8]) -> u32 {
        murmur3::murmur3_32(&mut &key[..], 0).unwrap()
    }

    fn hash_node(&self, node: &T, replica: usize) -> u32 {
        let mut buff = Vec::with_capacity(node.as_ref().len() + 8);
        buff.extend_from_slice(node.as_ref());
        buff.extend_from_slice(&replica.to_le_bytes());
        murmur3::murmur3_32(&mut &buff[..], 0).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use insta::assert_compact_debug_snapshot;

    use super::*;

    #[test]
    fn test_hash_ring() {
        fn make_ring(nodes: &[&'static str], replica_count: usize) -> HashRing<&'static str> {
            let mut ring = HashRing::new(replica_count);
            for node in nodes {
                ring.add_node(*node);
            }
            ring
        }

        let ring = make_ring(&["node1", "node2", "node3"], 3);
        assert_compact_debug_snapshot!(
            ring,
            @r#"HashRing { vnodes: 3, nodes: {135217799: {"node2"}, 265130893: {"node3"}, 265873634: {"node2"}, 968402280: {"node3"}, 1303218410: {"node3"}, 2076807935: {"node2"}, 2708114884: {"node1"}, 2759792820: {"node1"}, 3903532208: {"node1"}} }"#
        );
        assert_compact_debug_snapshot!(ring.lookup("key1"), @r#"Some("node1")"#);
        assert_compact_debug_snapshot!(ring.lookup("key2"), @r#"Some("node2")"#);
        assert_compact_debug_snapshot!(ring.lookup("key3"), @r#"Some("node1")"#);

        let ring = make_ring(&["node1", "node2", "node3"], 1);
        assert_compact_debug_snapshot!(
            ring,
            @r#"HashRing { vnodes: 1, nodes: {135217799: {"node2"}, 265130893: {"node3"}, 2759792820: {"node1"}} }"#
        );
        assert_compact_debug_snapshot!(ring.lookup("key1"), @r#"Some("node1")"#);
        assert_compact_debug_snapshot!(ring.lookup("key2"), @r#"Some("node1")"#);
        assert_compact_debug_snapshot!(ring.lookup("key3"), @r#"Some("node2")"#);
    }
}
