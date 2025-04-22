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
pub enum MemberStatus {
    Alive,
    Dead,
}

impl MemberStatus {
    // Downgrade the status of a member.
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

    pub fn update_member(&mut self, member: MemberState) {
        match self.members.entry(member.info.node_id) {
            Entry::Occupied(mut entry) => {
                let current = entry.get_mut();
                if current.info.incarnation < member.info.incarnation {
                    log::info!(target: "gossip", "advancing member incarnation from [{}] to [{}]: {member:?}", current.info.incarnation, member.info.incarnation);
                    *current = member;
                    return;
                }
                if current.info.incarnation > member.info.incarnation {
                    return;
                }
                // If the incarnation is the same, we only accept downgrades
                current.status.downgrade_to(&member.status);
                if member.status == MemberStatus::Dead {
                    log::info!(target: "gossip", "member confirmed dead: {member:?}");
                }
                current.heartbeat = current.heartbeat.max(member.heartbeat);
            }
            Entry::Vacant(entry) => {
                log::info!(target: "gossip", "adding new member: {member:?}");
                entry.insert(member);
            }
        }
    }

    pub fn remove_member(&mut self, id: Uuid) {
        log::info!(target: "gossip", "removing member: {id}");
        self.members.remove(&id);
    }
}
