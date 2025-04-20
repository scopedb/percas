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

use sha2::Digest;
use sha2::Sha256;

const DEFAULT_REPLICA_COUNT: usize = 256;

/// A consistent hash ring implementation.
/// This implementation uses SHA-256 to hash the nodes and keys.
/// It supports virtual nodes (replicas) to improve load balancing, every added node
/// will be replicated `replica_count` times in the ring.
///
/// # Examples
///
/// ```
/// use percas_cluster::HashRing;
///
/// let ring = HashRing::from(["node-1", "node-2", "node-3"]);
/// assert_eq!(ring.lookup("key1"), Some("node-1"));
/// assert_eq!(ring.lookup("key2"), Some("node-3"));
/// assert_eq!(ring.lookup("key3"), Some("node-3"));
/// ```
pub struct HashRing<T> {
    replica_count: usize,
    nodes: BTreeMap<u64, BTreeSet<T>>,
}

impl<T> Default for HashRing<T>
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

impl<T> Debug for HashRing<T>
where
    T: Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HashRing")
            .field("replica_count", &self.replica_count)
            .field("nodes", &self.nodes)
            .finish()
    }
}

impl<T> HashRing<T> {
    /// Creates a new `HashRing` with the specified replica count.
    pub fn new(replica_count: usize) -> Self {
        Self {
            replica_count,
            nodes: BTreeMap::new(),
        }
    }

    pub fn replica_count(&self) -> usize {
        self.replica_count
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
        let digest = self.hash_key(key.as_ref());
        self.nodes
            .range(digest..)
            .find_map(|(_, node)| node.iter().find(|v| predicate(*v)).cloned())
            .or_else(|| {
                self.nodes
                    .range(..=digest)
                    .find_map(|(_, node)| node.iter().find(|v| predicate(*v)).cloned())
            })
    }

    /// Adds a node to the ring.
    /// The node will be replicated `replica_count` times in the ring.
    pub fn add_node(&mut self, node: T) {
        for i in 0..self.replica_count {
            let digest = self.hash_node(&node, i);
            self.nodes.entry(digest).or_default().insert(node.clone());
        }
    }

    fn hash_key(&self, key: &[u8]) -> u64 {
        let mut hasher = Sha256::new();
        hasher.update(key);
        let digest = hasher.finalize();
        u64::from_be_bytes(digest[..8].try_into().unwrap())
    }

    fn hash_node(&self, node: &T, replica: usize) -> u64 {
        let mut hasher = Sha256::new();
        hasher.update(node.as_ref());
        hasher.update(replica.to_be_bytes());
        let digest = hasher.finalize();
        u64::from_be_bytes(digest[..8].try_into().unwrap())
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
            @r#"HashRing { replica_count: 3, nodes: {1130331173203730818: {"node1"}, 3453462956149404857: {"node3"}, 4664643935122212079: {"node1"}, 4945359727197601621: {"node2"}, 8109777462452160152: {"node1"}, 9540586358148544740: {"node2"}, 15364601984093359477: {"node3"}, 16459957557864277864: {"node2"}, 17005984661365267147: {"node3"}} }"#
        );
        assert_compact_debug_snapshot!(ring.lookup("key1"), @r#"Some("node2")"#);
        assert_compact_debug_snapshot!(ring.lookup("key2"), @r#"Some("node3")"#);
        assert_compact_debug_snapshot!(ring.lookup("key3"), @r#"Some("node1")"#);

        let ring = make_ring(&["node1", "node2", "node3"], 1);
        assert_compact_debug_snapshot!(
            ring,
            @r#"HashRing { replica_count: 1, nodes: {4664643935122212079: {"node1"}, 9540586358148544740: {"node2"}, 15364601984093359477: {"node3"}} }"#
        );
        assert_compact_debug_snapshot!(ring.lookup("key1"), @r#"Some("node2")"#);
        assert_compact_debug_snapshot!(ring.lookup("key2"), @r#"Some("node3")"#);
        assert_compact_debug_snapshot!(ring.lookup("key3"), @r#"Some("node1")"#);
    }
}
