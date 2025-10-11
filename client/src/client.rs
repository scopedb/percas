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

use fastrace_reqwest::traceparent_headers;
use reqwest::StatusCode;
use reqwest::Url;
use reqwest::redirect::Policy;
use serde::Deserialize;
use uuid::Uuid;

use crate::Error;
use crate::protos::Version;
use crate::route::RouteTable;

const UPDATE_ROUTE_TABLE_INTERVAL: Duration = Duration::from_secs(10);

fn make_opaque_error(msg: impl ToString) -> Error {
    Error::Opaque(msg.to_string())
}

/// A builder for creating a `Client`.
#[derive(Debug, Clone)]
pub struct ClientBuilder {
    data_url: String,
    ctrl_url: String,
    client: Option<reqwest::Client>,
}

impl ClientBuilder {
    /// Create a new client builder with the given data server url and control server url.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use percas_client::ClientBuilder;
    ///
    /// let builder = ClientBuilder::new("http://percas-data:8080", "http://percas-ctrl:8081");
    /// let client = builder.build().unwrap();
    /// let _ = client; // use client
    /// ```
    pub fn new(data_url: impl Into<String>, ctrl_url: impl Into<String>) -> Self {
        Self {
            data_url: data_url.into(),
            ctrl_url: ctrl_url.into(),
            client: None,
        }
    }

    /// Set a custom HTTP client. If not set, a default client will be used.
    pub fn http_client(mut self, client: reqwest::Client) -> Self {
        self.client = Some(client);
        self
    }

    /// Build the client.
    pub fn build(self) -> Result<Client, Error> {
        let Self {
            data_url,
            ctrl_url,
            client,
        } = self;

        let data_url = Url::parse(&data_url).map_err(make_opaque_error)?;
        let ctrl_url = Url::parse(&ctrl_url).map_err(make_opaque_error)?;
        let client = match client {
            Some(client) => client,
            None => reqwest::ClientBuilder::new()
                .no_proxy()
                .redirect(Policy::limited(2))
                .build()
                .map_err(make_opaque_error)?,
        };

        // force an initial route table update on first use
        let last_updated = Instant::now() - UPDATE_ROUTE_TABLE_INTERVAL - Duration::from_secs(1);
        Ok(Client {
            client,
            data_url,
            ctrl_url,
            last_updated: RwLock::new(last_updated),
            route_table: RwLock::new(None),
        })
    }
}

/// A client for interacting with a Percas cluster.
pub struct Client {
    client: reqwest::Client,
    data_url: Url,
    ctrl_url: Url,
    last_updated: RwLock<Instant>,
    route_table: RwLock<Option<RouteTable>>,
}

impl Client {
    /// Get the value associated with the given key.
    pub async fn get(&self, key: &str) -> Result<Option<Vec<u8>>, Error> {
        self.update_route_table_if_needed().await?;

        let url = self.route(key).join(key).map_err(make_opaque_error)?;

        let resp = self
            .client
            .get(url)
            .headers(traceparent_headers())
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

    /// Set the value associated with the given key.
    pub async fn put(&self, key: &str, value: &[u8]) -> Result<(), Error> {
        self.update_route_table_if_needed().await?;

        let url = self.route(key).join(key).map_err(make_opaque_error)?;

        let resp = self
            .client
            .put(url)
            .headers(traceparent_headers())
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

    /// Delete the value associated with the given key.
    pub async fn delete(&self, key: &str) -> Result<(), Error> {
        self.update_route_table_if_needed().await?;

        let url = self.route(key).join(key).map_err(make_opaque_error)?;

        let resp = self
            .client
            .delete(url)
            .headers(traceparent_headers())
            .send()
            .await
            .map_err(make_opaque_error)?;

        match resp.status() {
            StatusCode::OK | StatusCode::NO_CONTENT => Ok(()),
            StatusCode::TOO_MANY_REQUESTS => Err(Error::TooManyRequests),
            status => Err(make_opaque_error(status)),
        }
    }

    /// Get the version of the Percas server.
    pub async fn version(&self) -> Result<Version, Error> {
        let url = self.ctrl_url.join("version").map_err(make_opaque_error)?;

        let resp = self
            .client
            .get(url)
            .headers(traceparent_headers())
            .send()
            .await
            .map_err(make_opaque_error)?;

        match resp.status() {
            StatusCode::OK => resp.json::<Version>().await.map_err(make_opaque_error),
            StatusCode::TOO_MANY_REQUESTS => Err(Error::TooManyRequests),
            status => Err(make_opaque_error(status)),
        }
    }
}

impl Client {
    fn route(&self, key: &str) -> Url {
        if let Some(route_table) = &*self.route_table.read().unwrap()
            && let Some((_, url)) = route_table.lookup(key)
        {
            url.clone()
        } else {
            self.data_url.clone()
        }
    }

    async fn update_route_table_if_needed(&self) -> Result<(), Error> {
        let url = self.ctrl_url.join("members").map_err(make_opaque_error)?;

        if self.last_updated.read().unwrap().elapsed() > UPDATE_ROUTE_TABLE_INTERVAL {
            #[derive(Deserialize)]
            #[expect(dead_code)] // some fields may be unused
            struct Member {
                node_id: Uuid,
                advertise_data_url: Url,
                advertise_ctrl_url: Url,
                incarnation: u64,
                vnodes: Vec<u32>,
            }

            #[derive(Deserialize)]
            struct ListMembersResponse {
                members: Vec<Member>,
            }

            let resp = self
                .client
                .get(url)
                .headers(traceparent_headers())
                .send()
                .await
                .map_err(make_opaque_error)?;

            let members = match resp.status() {
                StatusCode::OK => {
                    resp.json::<ListMembersResponse>()
                        .await
                        .map_err(make_opaque_error)?
                        .members
                }
                StatusCode::TOO_MANY_REQUESTS => return Err(Error::TooManyRequests),
                status => return Err(make_opaque_error(status)),
            };

            let mut route_table = RouteTable::default();
            for member in members {
                for vnode in member.vnodes {
                    route_table.insert(vnode, member.node_id, member.advertise_data_url.clone());
                }
            }
            *self.route_table.write().unwrap() = Some(route_table);
            *self.last_updated.write().unwrap() = Instant::now();
        }
        Ok(())
    }
}
