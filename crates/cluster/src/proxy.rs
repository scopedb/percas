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

use std::sync::Arc;

use crate::gossip::GossipState;
use crate::member::MemberStatus;

#[derive(Debug, Clone)]
pub enum RouteDest {
    Local,
    RemoteAddr(String),
}

#[derive(Debug, Clone)]
pub struct Proxy {
    gossip: Arc<GossipState>,
}

impl Proxy {
    pub fn new(gossip: Arc<GossipState>) -> Self {
        Self { gossip }
    }

    pub fn route(&self, key: &str) -> RouteDest {
        let ring = self.gossip.ring();

        let membership = self.gossip.membership();
        let members = membership.members();

        if let Some(id) = ring.lookup_until(key, |id| {
            if let Some(member) = members.get(id) {
                if member.status == MemberStatus::Alive {
                    return true;
                }
            }
            false
        }) {
            if let Some(target) = members.get(&id) {
                if target.info.node_id == self.gossip.current().node_id {
                    return RouteDest::Local;
                }

                RouteDest::RemoteAddr(target.info.advertise_addr.clone())
            } else {
                RouteDest::Local
            }
        } else {
            log::debug!(
                "no target found for key: [{key}] , current ring: {:#?}",
                ring
            );
            RouteDest::Local
        }
    }
}
