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

use std::sync::RwLock;
use std::time::Duration;
use std::time::Instant;

use reqwest::StatusCode;
use reqwest::Url;
use reqwest::redirect::Policy;
use serde::Deserialize;
use serde::Serialize;
use uuid::Uuid;

use crate::Error;
use crate::route::RouteTable;

fn make_opaque_error(msg: impl ToString) -> Error {
    Error::Opaque(msg.to_string())
}

#[derive(Debug, Clone)]
pub struct ClientBuilder {
    client: reqwest::Client,
    addr: Option<String>,
    peer_addr: Option<String>,
}

impl ClientBuilder {
    pub fn new() -> Result<Self, Error> {
        let client = reqwest::ClientBuilder::new()
            .no_proxy()
            .redirect(Policy::limited(2))
            .build()
            .map_err(make_opaque_error)?;
        Ok(Self {
            client,
            addr: None,
            peer_addr: None,
        })
    }

    pub fn addr(mut self, addr: impl Into<String>) -> Self {
        self.addr = Some(addr.into());
        self
    }

    pub fn peer_addr(mut self, peer_addr: impl Into<String>) -> Self {
        self.peer_addr = Some(peer_addr.into());
        self
    }

    pub fn http_client(mut self, client: reqwest::Client) -> Self {
        self.client = client;
        self
    }

    pub fn build(self) -> Result<Client, Error> {
        let addr = self.addr.map_or_else(
            || Err(Error::Opaque("addr cannot be empty".to_string())),
            |addr| Url::parse(&addr).map_err(make_opaque_error),
        )?;

        Ok(Client {
            client: self.client,
            addr,
            peer_addr: None,

            last_updated: RwLock::new(
                Instant::now() - Client::UPDATE_ROUTE_TABLE_INTERVAL - Duration::from_secs(1),
            ),
            route_table: RwLock::new(None),
        })
    }
}

pub struct Client {
    client: reqwest::Client,
    addr: Url,
    peer_addr: Option<Url>,

    last_updated: RwLock<Instant>,
    route_table: RwLock<Option<RouteTable>>,
}

impl Client {
    pub async fn get(&self, key: &str) -> Result<Option<Vec<u8>>, Error> {
        self.update_route_table_if_needed().await?;

        do_get(self, key).await
    }

    pub async fn put(&self, key: &str, value: &[u8]) -> Result<(), Error> {
        self.update_route_table_if_needed().await?;

        do_put(self, key, value).await
    }

    pub async fn delete(&self, key: &str) -> Result<(), Error> {
        self.update_route_table_if_needed().await?;

        do_delete(self, key).await
    }

    pub async fn list_members(&self) -> Result<Vec<Member>, Error> {
        do_list_members(self).await
    }

    fn route(&self, key: &str) -> Result<Url, Error> {
        if let Some(route_table) = &*self.route_table.read().unwrap()
            && let Some((_, addr)) = route_table.lookup(key)
        {
            return Url::parse(addr).map_err(make_opaque_error);
        }

        Url::parse(self.addr.as_ref()).map_err(make_opaque_error)
    }

    fn route_table_enabled(&self) -> bool {
        self.peer_addr.is_some()
    }

    const UPDATE_ROUTE_TABLE_INTERVAL: Duration = Duration::from_secs(10);

    async fn update_route_table_if_needed(&self) -> Result<(), Error> {
        if !self.route_table_enabled() {
            return Ok(());
        }

        if self.last_updated.read().unwrap().elapsed() > Self::UPDATE_ROUTE_TABLE_INTERVAL {
            let members = self.list_members().await?;
            let mut route_table = RouteTable::new();
            for member in &members {
                for vnode in &member.vnodes {
                    route_table.insert(*vnode, member.node_id, member.advertise_addr.clone());
                }
            }
            *self.route_table.write().unwrap() = Some(route_table);
            *self.last_updated.write().unwrap() = std::time::Instant::now();
        }
        Ok(())
    }
}

async fn do_get(client: &Client, key: &str) -> Result<Option<Vec<u8>>, Error> {
    let url = client.route(key)?.join(key).map_err(make_opaque_error)?;

    let resp = client
        .client
        .get(url)
        .send()
        .await
        .map_err(make_opaque_error)?;

    match resp.status() {
        StatusCode::NOT_FOUND => Ok(None),
        StatusCode::OK => {
            let body = resp.bytes().await.map_err(make_opaque_error)?;
            Ok(Some(body.to_vec()))
        }
        StatusCode::TOO_MANY_REQUESTS => Err(Error::TooManyRequests),
        _ => Err(make_opaque_error(resp.status())),
    }
}

async fn do_put(client: &Client, key: &str, value: &[u8]) -> Result<(), Error> {
    let url = client.route(key)?.join(key).map_err(make_opaque_error)?;

    let resp = client
        .client
        .put(url)
        .body(value.to_vec())
        .send()
        .await
        .map_err(make_opaque_error)?;

    match resp.status() {
        StatusCode::OK | StatusCode::CREATED => Ok(()),
        StatusCode::TOO_MANY_REQUESTS => Err(Error::TooManyRequests),
        status => Err(make_opaque_error(status)),
    }
}

async fn do_delete(client: &Client, key: &str) -> Result<(), Error> {
    let url = client.route(key)?.join(key).map_err(make_opaque_error)?;

    let resp = client
        .client
        .delete(url)
        .send()
        .await
        .map_err(make_opaque_error)?;

    match resp.status() {
        StatusCode::OK | StatusCode::NO_CONTENT => Ok(()),
        StatusCode::TOO_MANY_REQUESTS => Err(Error::TooManyRequests),
        status => Err(make_opaque_error(status)),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Member {
    node_id: Uuid,
    cluster_id: String,
    advertise_addr: String,
    advertise_peer_addr: String,
    incarnation: u64,
    status: String,
    heartbeat: String,
    vnodes: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
struct ListMembersResponse {
    members: Vec<Member>,
}

async fn do_list_members(client: &Client) -> Result<Vec<Member>, Error> {
    let Some(peer_addr) = &client.peer_addr else {
        return Err(Error::Opaque(
            "cannot list members with peer_addr not set".to_string(),
        ));
    };
    let url = peer_addr.join("members").map_err(make_opaque_error)?;

    let resp = client
        .client
        .get(url)
        .send()
        .await
        .map_err(make_opaque_error)?;

    match resp.status() {
        StatusCode::OK => {
            let members = resp
                .json::<ListMembersResponse>()
                .await
                .map_err(make_opaque_error)?
                .members;
            Ok(members)
        }
        StatusCode::TOO_MANY_REQUESTS => Err(Error::TooManyRequests),
        status => Err(make_opaque_error(status)),
    }
}
