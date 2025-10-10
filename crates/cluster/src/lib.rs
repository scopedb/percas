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

mod gossip;
mod member;
mod node;
mod proxy;
mod ring;

use std::fmt;

pub use gossip::GossipFuture;
pub use gossip::GossipMessage;
pub use gossip::GossipState;
pub use member::MemberState;
pub use member::MemberStatus;
pub use member::Membership;
pub use node::NodeInfo;
pub use proxy::Proxy;
pub use proxy::RouteDest;
pub use ring::HashRing;

#[derive(Debug)]
pub struct GossipError(String);

impl GossipError {
    pub fn new(msg: impl Into<String>) -> Self {
        Self(msg.into())
    }
}

impl fmt::Display for GossipError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for GossipError {}
