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
    Suspect,
    Dead,
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

    pub fn into_members(self) -> BTreeMap<Uuid, MemberState> {
        self.members
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

                // For equal incarnations, newer observations win. At equal
                // timestamps, severity breaks ties independently of merge order.
                let rank = |status| match status {
                    MemberStatus::Alive => 0,
                    MemberStatus::Suspect => 1,
                    MemberStatus::Dead => 2,
                };
                if member.heartbeat < current.heartbeat
                    || (member.heartbeat == current.heartbeat
                        && rank(member.status) <= rank(current.status))
                {
                    return false;
                }
                *current = member;
                true
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

impl FromIterator<MemberState> for Membership {
    fn from_iter<T: IntoIterator<Item = MemberState>>(iter: T) -> Self {
        let mut membership = Membership::default();
        for member in iter {
            membership.update_member(member);
        }
        membership
    }
}

#[cfg(test)]
mod membership_tests {
    use jiff::Timestamp;
    use reqwest::Url;
    use uuid::Uuid;

    use super::*;

    fn make_node(id: Uuid, _inc: u64) -> NodeInfo {
        NodeInfo::new(
            id,
            "cluster".to_string(),
            Url::parse("http://127.0.0.1:7654").unwrap(),
            Url::parse("http://127.0.0.1:7655").unwrap(),
        )
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
    use reqwest::Url;
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
                advertise_data_url: Url::parse("http://127.0.0.1:7654").unwrap(),
                advertise_ctrl_url: Url::parse("http://127.0.0.1:7655").unwrap(),
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
                "advertise_data_url": "http://127.0.0.1:7654/",
                "advertise_ctrl_url": "http://127.0.0.1:7655/",
                "incarnation": 1
              },
              "status": "alive",
              "heartbeat": "1970-01-01T03:25:45.000006789Z"
            }
            "#
        );
    }
}

#[cfg(test)]
mod merge_regressions {
    use super::*;

    fn member(status: MemberStatus, heartbeat: i64) -> MemberState {
        MemberState {
            info: NodeInfo::new(
                Uuid::nil(),
                "cluster".into(),
                "http://localhost:7654".parse().unwrap(),
                "http://localhost:7655".parse().unwrap(),
            ),
            status,
            heartbeat: Timestamp::from_second(heartbeat).unwrap(),
        }
    }

    #[test]
    fn stale_status_cannot_override_newer_observation() {
        for (current, stale) in [
            (MemberStatus::Alive, MemberStatus::Dead),
            (MemberStatus::Dead, MemberStatus::Alive),
        ] {
            let mut membership = Membership::from_iter([member(current, 20)]);
            assert!(!membership.update_member(member(stale, 10)));
            assert_eq!(membership.members()[&Uuid::nil()].status, current);
        }
    }

    #[test]
    fn equal_timestamp_merge_is_independent_of_delivery_order() {
        for (first, second) in [
            (MemberStatus::Alive, MemberStatus::Dead),
            (MemberStatus::Dead, MemberStatus::Alive),
            (MemberStatus::Suspect, MemberStatus::Dead),
        ] {
            let mut membership = Membership::from_iter([member(first, 20)]);
            membership.update_member(member(second, 20));
            assert_eq!(
                membership.members()[&Uuid::nil()].status,
                MemberStatus::Dead
            );
        }
    }
}
