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

use jiff::Timestamp;
use serde::Deserialize;
use serde::Serialize;
use uuid::Uuid;

use crate::node::NodeInfo;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MemberStatus {
    Alive,
    Dead,
}

impl MemberStatus {
    /// Downgrade the status of a member.
    pub fn downgrade_to(&mut self, other: &MemberStatus) {
        match (&self, other) {
            (MemberStatus::Alive, MemberStatus::Alive) => {}
            _ => {
                *self = *other;
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MemberState {
    pub info: NodeInfo,
    pub status: MemberStatus,
    pub heartbeat: Timestamp,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Membership {
    members: BTreeMap<Uuid, MemberState>,
}

impl Membership {
    pub fn members(&self) -> &BTreeMap<Uuid, MemberState> {
        &self.members
    }

    pub fn is_dead(&self, id: Uuid) -> bool {
        self.members
            .get(&id)
            .is_some_and(|member| member.status == MemberStatus::Dead)
    }

    /// Update a member's state. Returns `true` if the membership map was
    /// modified (added, replaced, or had its status/heartbeat changed).
    ///
    /// Merge rules:
    /// - If incoming incarnation > current incarnation -> replace entry.
    /// - If incoming incarnation < current incarnation -> ignore.
    /// - If incarnation equal:
    ///     * Use heartbeat as a tiebreaker: the larger heartbeat is considered the fresher
    ///       observation.
    ///     * Status changes are accepted if the incoming observation is at least as fresh
    ///       (heartbeat >= current heartbeat). This avoids flipping status based on stale reports.
    pub fn update_member(&mut self, member: MemberState) -> bool {
        match self.members.entry(member.info.node_id) {
            Entry::Occupied(mut entry) => {
                let current = entry.get_mut();
                // incoming has higher incarnation -> authoritative replace
                if current.info.incarnation < member.info.incarnation {
                    log::info!(target: "gossip", "advancing member incarnation from [{}] to [{}]: {member:?}", current.info.incarnation, member.info.incarnation);
                    *current = member;
                    return true;
                }

                // incoming is older incarnation -> ignore
                if current.info.incarnation > member.info.incarnation {
                    return false;
                }

                // same incarnation: decide based on heartbeat and status
                let prev_status = current.status;
                let prev_heartbeat = current.heartbeat;

                // Update heartbeat to the freshest observation
                current.heartbeat = current.heartbeat.max(member.heartbeat);

                // Accept status change only if the incoming observation is at
                // least as fresh (prevents stale dead reports from overriding)
                if member.heartbeat >= prev_heartbeat && member.status != current.status {
                    current.status = member.status;
                    if current.status == MemberStatus::Dead {
                        log::info!(target: "gossip", "member confirmed dead: {current:?}");
                    }
                } else {
                    // Still allow explicit downgrade_to behavior for the common
                    // case where a Dead report should override an Alive when
                    // appropriate (keeps compatibility with previous logic).
                    current.status.downgrade_to(&member.status);
                }

                // Return true if either status or heartbeat changed
                current.status != prev_status || current.heartbeat != prev_heartbeat
            }
            Entry::Vacant(entry) => {
                log::info!(target: "gossip", "adding new member: {member:?}");
                entry.insert(member);
                true
            }
        }
    }

    pub fn remove_member(&mut self, id: Uuid) {
        log::info!(target: "gossip", "removing member: {id}");
        self.members.remove(&id);
    }
}

#[cfg(test)]
mod membership_tests {
    use jiff::Timestamp;
    use uuid::Uuid;

    use super::*;

    fn make_node(id: Uuid, _inc: u64) -> NodeInfo {
        NodeInfo::init(id, "c".to_string(), "a".to_string(), "p".to_string())
    }

    #[test]
    fn add_new_member() {
        let mut m = Membership::default();
        let id = Uuid::nil();
        let node = make_node(id, 0);
        m.update_member(MemberState {
            info: node.clone(),
            status: MemberStatus::Alive,
            heartbeat: Timestamp::now(),
        });

        assert!(m.members().contains_key(&id));
    }

    #[test]
    fn heartbeat_and_incarnation_merge() {
        let mut m = Membership::default();
        let id = Uuid::nil();
        let node = make_node(id, 0);

        // insert with heartbeat t0
        let t0 = Timestamp::now();
        m.update_member(MemberState {
            info: node.clone(),
            status: MemberStatus::Alive,
            heartbeat: t0,
        });

        // same incarnation but later heartbeat t1
        let t1 = Timestamp::now();
        m.update_member(MemberState {
            info: node.clone(),
            status: MemberStatus::Alive,
            heartbeat: t1,
        });

        let stored = m.members().get(&id).unwrap();
        assert!(stored.heartbeat >= t0);
        assert!(stored.heartbeat >= t1);
    }

    #[test]
    fn higher_incarnation_replaces() {
        let mut m = Membership::default();
        let id = Uuid::nil();
        let node = make_node(id, 0);

        m.update_member(MemberState {
            info: NodeInfo {
                incarnation: 1,
                ..node.clone()
            },
            status: MemberStatus::Alive,
            heartbeat: Timestamp::now(),
        });

        // higher incarnation
        m.update_member(MemberState {
            info: NodeInfo {
                incarnation: 2,
                ..node.clone()
            },
            status: MemberStatus::Dead,
            heartbeat: Timestamp::now(),
        });

        let stored = m.members().get(&id).unwrap();
        assert_eq!(stored.info.incarnation, 2);
        assert_eq!(stored.status, MemberStatus::Dead);
    }
}

#[cfg(test)]
mod tests {
    use insta::assert_json_snapshot;
    use jiff::Timestamp;
    use uuid::Uuid;

    use crate::NodeInfo;
    use crate::member::MemberState;
    use crate::member::MemberStatus;

    #[test]
    fn test_member_serde() {
        let member = MemberState {
            info: NodeInfo {
                node_id: Uuid::from_u64_pair(1234, 5678),
                cluster_id: "cluster".to_string(),
                advertise_addr: "127.0.0.1:9000".to_string(),
                advertise_peer_addr: "127.0.0.1:9001".to_string(),
                incarnation: 1,
            },
            status: MemberStatus::Alive,
            heartbeat: Timestamp::constant(12345, 6789),
        };

        assert_json_snapshot!(
            member,
            @r#"
            {
              "info": {
                "node_id": "00000000-0000-04d2-0000-00000000162e",
                "cluster_id": "cluster",
                "advertise_addr": "127.0.0.1:9000",
                "advertise_peer_addr": "127.0.0.1:9001",
                "incarnation": 1
              },
              "status": "alive",
              "heartbeat": "1970-01-01T03:25:45.000006789Z"
            }
            "#
        );
    }
}
