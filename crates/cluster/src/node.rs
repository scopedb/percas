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

use error_stack::Result;
use error_stack::ResultExt;
use error_stack::bail;
use serde::Deserialize;
use serde::Serialize;
use uuid::Uuid;

use crate::ClusterError;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NodeInfo {
    pub id: Uuid,
    pub name: String,
    pub addr: String,
    pub peer_addr: String,
    pub incarnation: u64,
}

impl NodeInfo {
    pub fn init(id: Option<Uuid>, name: String, addr: String, peer_addr: String) -> Self {
        Self {
            id: id.unwrap_or_else(Uuid::new_v4),
            name,
            addr,
            peer_addr,
            incarnation: 0,
        }
    }

    pub fn persist(&self, path: &Path) -> Result<(), std::io::Error> {
        let data = serde_json::to_string_pretty(self).unwrap();
        std::fs::write(path, data)?;
        Ok(())
    }

    pub fn load(path: &Path) -> Result<Option<Self>, ClusterError> {
        let make_error = || {
            ClusterError::Internal(format!(
                "failed to load node info from file: {}",
                path.display()
            ))
        };

        if path.exists() {
            let data = std::fs::read_to_string(path).change_context_lazy(make_error)?;
            if let Ok(info) = serde_json::from_str::<Self>(&data) {
                return Ok(Some(info));
            } else {
                bail!(make_error());
            }
        }
        Ok(None)
    }
}
