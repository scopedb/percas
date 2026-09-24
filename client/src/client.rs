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
use std::sync::Mutex;
use std::time::Duration;
use std::time::Instant;

use fastrace_reqwest::traceparent_headers;
use reqwest::Body;
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
    control_peers: Vec<String>,
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
            control_peers: Vec::new(),
        }
    }

    /// Adds a control endpoint to try when refreshing cluster membership.
    pub fn control_peer(mut self, url: impl Into<String>) -> Self {
        self.control_peers.push(url.into());
        self
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
            control_peers,
        } = self;

        let data_url = parse_endpoint(&data_url)?;
        let ctrl_url = parse_endpoint(&ctrl_url)?;
        let client = match client {
            Some(client) => client,
            None => reqwest::ClientBuilder::new()
                .connect_timeout(Duration::from_secs(1))
                .timeout(Duration::from_secs(5))
                .no_proxy()
                .redirect(Policy::limited(2))
                .build()
                .map_err(make_opaque_error)?,
        };

        let mut peers = vec![ctrl_url.clone()];
        for peer in control_peers {
            let peer = parse_endpoint(&peer)?;
            if !peers.contains(&peer) {
                peers.push(peer);
            }
        }
        Ok(Client {
            client,
            data_url,
            ctrl_url,
            routes: Arc::new(Mutex::new(RouteState {
                next_refresh: Instant::now(),
                refreshing: false,
                table: RouteTable::default(),
                seeds: peers.clone(),
                peers,
                cursor: 0,
            })),
        })
    }
}

/// A client for interacting with a Percas cluster.
pub struct Client {
    client: reqwest::Client,
    data_url: Url,
    ctrl_url: Url,
    routes: Arc<Mutex<RouteState>>,
}

impl Client {
    /// Get the value associated with the given key.
    pub async fn get(&self, key: &str) -> Result<Option<Vec<u8>>, Error> {
        self.refresh_routes_in_background();

        let url = key_url(self.route(key), key);

        let resp = self
            .client
            .get(url)
            .headers(traceparent_headers())
            .timeout(Duration::from_secs(5))
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
        self.refresh_routes_in_background();

        let url = key_url(self.route(key), key);

        let resp = self
            .client
            .put(url)
            .headers(traceparent_headers())
            .body(value.to_vec())
            .timeout(Duration::from_secs(5))
            .send()
            .await
            .map_err(make_opaque_error)?;

        match resp.status() {
            StatusCode::OK | StatusCode::CREATED => Ok(()),
            StatusCode::TOO_MANY_REQUESTS => Err(Error::TooManyRequests),
            status => Err(make_opaque_error(status)),
        }
    }

    /// Set the value associated with the given key.
    ///
    /// This method exists to avoid an extra copy when the caller has ownership of the data:
    ///
    /// * `&'static str`
    /// * `Vec<u8>`
    /// * `bytes::Bytes`
    /// * `reqwest::Body`
    pub async fn put_owned<T: Into<Body>>(&self, key: &str, value: T) -> Result<(), Error> {
        self.refresh_routes_in_background();

        let url = key_url(self.route(key), key);

        let resp = self
            .client
            .put(url)
            .headers(traceparent_headers())
            .body(value)
            .timeout(Duration::from_secs(5))
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
        self.refresh_routes_in_background();

        let url = key_url(self.route(key), key);

        let resp = self
            .client
            .delete(url)
            .headers(traceparent_headers())
            .timeout(Duration::from_secs(5))
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
            .timeout(Duration::from_secs(5))
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

// Reset admission even if the runtime cancels a refresh task.
struct RefreshGuard(Arc<Mutex<RouteState>>);
impl Drop for RefreshGuard {
    fn drop(&mut self) {
        self.0.lock().unwrap().refreshing = false;
    }
}

struct RouteState {
    next_refresh: Instant,
    refreshing: bool,
    table: RouteTable,
    seeds: Vec<Url>,
    peers: Vec<Url>,
    cursor: usize,
}

#[derive(Deserialize)]
struct Member {
    node_id: Uuid,
    advertise_data_url: Url,
    advertise_ctrl_url: Url,
    status: String,
    vnodes: Vec<u32>,
}

#[derive(Deserialize)]
struct Members {
    members: Vec<Member>,
}

fn parse_endpoint(endpoint: &str) -> Result<Url, Error> {
    let url = Url::parse(endpoint).map_err(make_opaque_error)?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(make_opaque_error(
            "expected an HTTP(S) endpoint without credentials",
        ));
    }
    Ok(url)
}

// Keep keys in a query field: URL path normalization must never change a key.
fn key_url(mut base: Url, key: &str) -> Url {
    base.set_path("/v1/cache");
    base.set_query(None);
    base.set_fragment(None);
    base.query_pairs_mut().append_pair("key", key);
    base
}

impl Client {
    fn route(&self, key: &str) -> Url {
        self.routes
            .lock()
            .unwrap()
            .table
            .lookup(key)
            .map(|(_, url)| url.clone())
            .unwrap_or_else(|| self.data_url.clone())
    }

    fn refresh_routes_in_background(&self) {
        let mut state = self.routes.lock().unwrap();
        if state.refreshing || Instant::now() < state.next_refresh {
            return;
        }
        let Ok(runtime) = tokio::runtime::Handle::try_current() else {
            return;
        };
        state.refreshing = true;
        let mut peers = state.peers.clone();
        if !peers.is_empty() {
            let cursor = state.cursor % peers.len();
            peers.rotate_left(cursor);
            state.cursor = cursor + 1;
        }
        let routes = self.routes.clone();
        let client = self.client.clone();
        drop(state);
        let refresh = RefreshGuard(routes.clone());
        runtime.spawn(async move {
            let _refresh = refresh;
            // A single bounded refresh serves all concurrent callers. A failed
            // refresh retains the last usable table and never blocks a cache call.
            let updated = tokio::time::timeout(Duration::from_secs(3), async {
                for mut peer in peers {
                    peer.set_path("/members");
                    peer.set_query(None);
                    peer.set_fragment(None);
                    let response = client
                        .get(peer)
                        .timeout(Duration::from_millis(500))
                        .send()
                        .await;
                    let Ok(response) = response else {
                        continue;
                    };
                    if !response.status().is_success() {
                        continue;
                    }
                    let Ok(members) = response.json::<Members>().await else {
                        continue;
                    };
                    let mut table = RouteTable::default();
                    let mut learned = Vec::new();
                    for member in members.members {
                        if !matches!(member.status.as_str(), "alive" | "suspect") {
                            continue;
                        }
                        if parse_endpoint(member.advertise_data_url.as_str()).is_err()
                            || parse_endpoint(member.advertise_ctrl_url.as_str()).is_err()
                        {
                            continue;
                        }
                        for vnode in member.vnodes {
                            table.insert(vnode, member.node_id, member.advertise_data_url.clone());
                        }
                        if !learned.contains(&member.advertise_ctrl_url) {
                            learned.push(member.advertise_ctrl_url);
                        }
                    }
                    if table.lookup("").is_some() {
                        return Some((table, learned));
                    }
                }
                None
            })
            .await
            .ok()
            .flatten();
            let mut state = routes.lock().unwrap();
            let delay = if let Some((table, mut learned)) = updated {
                state.table = table;
                for seed in &state.seeds {
                    if !learned.contains(seed) {
                        learned.push(seed.clone());
                    }
                }
                state.peers = learned;
                UPDATE_ROUTE_TABLE_INTERVAL
            } else {
                Duration::from_secs(1)
            };
            state.next_refresh = Instant::now() + delay;
        });
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicUsize;
    use std::sync::atomic::Ordering;

    use tokio::io::AsyncReadExt;
    use tokio::io::AsyncWriteExt;

    use super::*;

    async fn http_server(body: String, status: u16) -> (Url, Arc<AtomicUsize>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = Url::parse(&format!("http://{}/", listener.local_addr().unwrap())).unwrap();
        let count = Arc::new(AtomicUsize::new(0));
        let seen = count.clone();
        tokio::spawn(async move {
            loop {
                let (mut stream, _) = listener.accept().await.unwrap();
                seen.fetch_add(1, Ordering::SeqCst);
                let response = format!(
                    "HTTP/1.1 {status} OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                tokio::spawn(async move {
                    let mut request = vec![0; 16384];
                    let _ = stream.read(&mut request).await;
                    let _ = stream.write_all(response.as_bytes()).await;
                });
            }
        });
        (url, count)
    }

    async fn refresh_done(client: &Client) {
        tokio::time::timeout(Duration::from_secs(4), async {
            while client.routes.lock().unwrap().refreshing {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
    }

    #[test]
    fn keys_cannot_change_the_endpoint_or_be_normalized() {
        for key in [
            "",
            "..",
            "a/../b",
            "?q=1#fragment",
            "http://other.test/key",
            "//other.test/",
            "a%2Fb",
            "中文 / +",
        ] {
            let url = key_url(
                Url::parse("http://cache.test/old?ignored=yes#fragment").unwrap(),
                key,
            );
            assert_eq!(url.host_str(), Some("cache.test"));
            assert_eq!(url.path(), "/v1/cache");
            assert_eq!(url.fragment(), None);
            assert_eq!(
                url.query_pairs().collect::<Vec<_>>(),
                vec![("key".into(), key.into())]
            );
        }
    }

    #[tokio::test]
    async fn unavailable_control_does_not_block_data_or_duplicate_refreshes() {
        let (data, _) = http_server("value".into(), 200).await;
        let stalled = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let client = ClientBuilder::new(
            data.to_string(),
            format!("http://{}", stalled.local_addr().unwrap()),
        )
        .build()
        .unwrap();
        for _ in 0..100 {
            client.refresh_routes_in_background();
        }
        let (connection, _) = stalled.accept().await.unwrap();
        assert!(
            tokio::time::timeout(Duration::from_millis(50), stalled.accept())
                .await
                .is_err()
        );
        assert_eq!(
            tokio::time::timeout(Duration::from_millis(300), client.get("key"))
                .await
                .unwrap()
                .unwrap(),
            Some(b"value".to_vec())
        );
        drop(connection);
    }

    #[tokio::test]
    async fn refresh_fails_over_and_excludes_dead_members() {
        let (data, _) = http_server("value".into(), 200).await;
        let body = serde_json::json!({"members": [
            {"node_id": Uuid::now_v7(), "status": "dead", "advertise_data_url": "http://dead.test/", "advertise_ctrl_url": "http://dead.test/", "vnodes": [0]},
            {"node_id": Uuid::now_v7(), "status": "alive", "advertise_data_url": data, "advertise_ctrl_url": "http://127.0.0.1:9/", "vnodes": [1]}
        ]});
        let (control, _) = http_server(body.to_string(), 200).await;
        let (failed, _) = http_server("failure".into(), 503).await;
        let client = ClientBuilder::new("http://bootstrap.test", failed.to_string())
            .control_peer(control.to_string())
            .build()
            .unwrap();
        client.refresh_routes_in_background();
        refresh_done(&client).await;
        assert_eq!(client.route("key"), data);
        {
            let mut state = client.routes.lock().unwrap();
            state.peers = vec![failed];
            state.next_refresh = Instant::now();
        }
        client.refresh_routes_in_background();
        refresh_done(&client).await;
        assert_eq!(client.route("key"), data);
        assert_eq!(client.get("key").await.unwrap(), Some(b"value".to_vec()));
    }
}
