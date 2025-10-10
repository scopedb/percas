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

use std::io;
use std::path::Path;

use serde::Deserialize;
use serde::Serialize;
use uuid::Uuid;

/// PersistentNodeInfo is used to store the node information in a file.
/// The `advertise_addr` and `advertise_peer_addr` fields are not included in this struct, since
/// addr may change after the node is restarted if the node is deployed in cloud environments.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
struct PersistentNodeInfo {
    node_id: Uuid,
    cluster_id: String,
    incarnation: u64,
}

impl From<NodeInfo> for PersistentNodeInfo {
    fn from(node_info: NodeInfo) -> Self {
        Self {
            node_id: node_info.node_id,
            cluster_id: node_info.cluster_id,
            incarnation: node_info.incarnation,
        }
    }
}

impl PersistentNodeInfo {
    fn load(path: &Path) -> Result<Option<Self>, io::Error> {
        if path.exists() {
            let data = std::fs::read_to_string(path)?;
            let info = serde_json::from_str(&data)?;
            Ok(Some(info))
        } else {
            Ok(None)
        }
    }

    fn persist(&self, path: &Path) -> Result<(), io::Error> {
        let data = serde_json::to_string_pretty(self)?;
        std::fs::write(path, data)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NodeInfo {
    pub node_id: Uuid,
    pub cluster_id: String,
    pub advertise_addr: String,
    pub advertise_peer_addr: String,
    pub incarnation: u64,
}

impl NodeInfo {
    pub fn init(node_id: Uuid, cluster_id: String, addr: String, peer_addr: String) -> Self {
        Self {
            node_id,
            cluster_id,
            advertise_addr: addr,
            advertise_peer_addr: peer_addr,
            incarnation: 0,
        }
    }

    pub fn advance_incarnation(&mut self) {
        self.incarnation += 1;
    }

    pub fn load(path: &Path, advertise_addr: String, advertise_peer_addr: String) -> Option<Self> {
        let info = PersistentNodeInfo::load(path).unwrap_or_else(|err| {
            panic!(
                "unrecoverable: failed to load node info from {}: {err}",
                path.display()
            )
        });

        info.map(|info| Self {
            node_id: info.node_id,
            cluster_id: info.cluster_id,
            advertise_addr,
            advertise_peer_addr,
            incarnation: info.incarnation,
        })
    }

    pub fn persist(&self, path: &Path) {
        let info = PersistentNodeInfo {
            node_id: self.node_id,
            cluster_id: self.cluster_id.clone(),
            incarnation: self.incarnation,
        };

        info.persist(path).unwrap_or_else(|err| {
            panic!(
                "unrecoverable: failed to persist node info to {}: {err}",
                path.display()
            )
        })
    }
}
