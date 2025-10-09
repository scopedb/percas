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

use std::path::Path;

use exn::Result;
use exn::ResultExt;
use serde::Deserialize;
use serde::Serialize;
use uuid::Uuid;

use crate::ClusterError;

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
    fn load(path: &Path) -> Result<Option<Self>, ClusterError> {
        let make_error =
            || ClusterError(format!("failed to load node info from {}", path.display()));

        if path.exists() {
            let data = std::fs::read_to_string(path).or_raise(make_error)?;
            let info = serde_json::from_str(&data).or_raise(make_error)?;
            Ok(Some(info))
        } else {
            Ok(None)
        }
    }

    fn persist(&self, path: &Path) -> Result<(), ClusterError> {
        let make_error = || {
            ClusterError(format!(
                "failed to persist node info into {}",
                path.display()
            ))
        };

        let data = serde_json::to_string_pretty(self).or_raise(make_error)?;
        std::fs::write(path, data).or_raise(make_error)
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
        let info = PersistentNodeInfo::load(path).expect("unrecoverable: failed to load node info");
        info.map(|info| Self {
            node_id: info.node_id,
            cluster_id: info.cluster_id,
            advertise_addr,
            advertise_peer_addr,
            incarnation: info.incarnation,
        })
    }

    pub fn persist(&self, path: &Path) {
        PersistentNodeInfo {
            node_id: self.node_id,
            cluster_id: self.cluster_id.clone(),
            incarnation: self.incarnation,
        }
        .persist(path)
        .expect("unrecoverable: failed to persist node info")
    }
}
