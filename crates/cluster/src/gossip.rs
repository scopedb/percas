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

use std::random::random;
use std::sync::Arc;
use std::sync::RwLock;
use std::time::Duration;

use backon::ConstantBuilder;
use backon::Retryable;
use error_stack::Result;
use error_stack::ResultExt;
use error_stack::bail;
use fastimer::MakeDelayExt;
use jiff::Timestamp;
use percas_core::JoinHandle;
use percas_core::Runtime;
use percas_core::make_runtime;
use percas_core::timer;
use poem::IntoResponse;
use poem::Response;
use poem::Route;
use poem::handler;
use poem::listener::TcpListener;
use poem::post;
use poem::web::Data;
use poem::web::Json;
use reqwest::Client;
use reqwest::ClientBuilder;
use reqwest::Url;
use serde::Deserialize;
use serde::Serialize;
use uuid::Uuid;

use crate::ClusterError;
use crate::member::MemberState;
use crate::member::MemberStatus;
use crate::member::Membership;
use crate::member::NodeInfo;
use crate::ring::HashRing;

const DEFAULT_PING_INTERVAL: Duration = Duration::from_secs(1);
const DEFAULT_SYNC_INTERVAL: Duration = Duration::from_secs(5);

const DEFAULT_RETRY_INTERVAL: Duration = Duration::from_secs(1);
const DEFAULT_RETRIES: usize = 3;

const DEFAULT_REBUILD_RING_INTERVAL: Duration = Duration::from_secs(10);

pub struct GossipState {
    initial_peers: Vec<String>,
    current_node: NodeInfo,
    transport: Transport,

    membership: RwLock<Membership>,
    ring: RwLock<Arc<HashRing<Uuid>>>,
}

impl GossipState {
    pub fn new(current_node: NodeInfo, initial_peers: Vec<String>) -> Self {
        let members = RwLock::new(Membership::default());
        let transport = Transport::new();
        let ring = RwLock::new(Arc::new(HashRing::default()));
        Self {
            initial_peers,
            current_node,
            membership: members,
            transport,
            ring,
        }
    }

    pub fn current(&self) -> &NodeInfo {
        &self.current_node
    }

    pub fn membership(&self) -> Membership {
        self.membership.read().unwrap().clone()
    }

    pub fn ring(&self) -> Arc<HashRing<Uuid>> {
        self.ring.read().unwrap().clone()
    }

    pub async fn start(
        self: Arc<Self>,
    ) -> Result<JoinHandle<std::result::Result<(), std::io::Error>>, ClusterError> {
        let rt = make_runtime("gossip", "gossip", 1);
        let route = Route::new().at("/gossip", post(gossip));

        // Listen on the peer address
        let addr = self.current_node.peer_addr.clone();
        let server_fut =
            rt.spawn(async move { poem::Server::new(TcpListener::bind(&addr)).run(route).await });

        // Start the gossip protocol
        let state = self.clone();
        drive_gossip(state, &rt).await?;

        Ok(server_fut)
    }

    fn handle_message(&self, message: Message) -> Option<Message> {
        match message {
            Message::Ping(info) => {
                self.membership.write().unwrap().update_member(MemberState {
                    info: info.clone(),
                    status: MemberStatus::Alive,
                    heartbeat: Timestamp::now(),
                });

                // Respond with an ack
                Some(Message::Ack(self.current_node.clone()))
            }
            Message::Ack(info) => {
                self.membership.write().unwrap().update_member(MemberState {
                    info,
                    status: MemberStatus::Alive,
                    heartbeat: Timestamp::now(),
                });
                None
            }
            Message::Sync { members } => {
                let snapshot = self.membership.read().unwrap().members().clone();
                for member in members {
                    if let Some(current) = snapshot.get(&member.info.id) {
                        // Update the member state
                        if current.heartbeat < member.heartbeat {
                            self.membership.write().unwrap().update_member(member);
                        }
                    } else {
                        // Add the new member
                        self.membership.write().unwrap().update_member(member);
                    }
                }

                // Ensure the current node is alive
                self.membership.write().unwrap().update_member(MemberState {
                    info: self.current_node.clone(),
                    status: MemberStatus::Alive,
                    heartbeat: Timestamp::now(),
                });

                // Respond with the current membership
                let members = self.membership.read().unwrap().members().clone();
                Some(Message::Sync {
                    members: members.values().cloned().collect(),
                })
            }
        }
    }

    async fn ping(&self, peer: NodeInfo) {
        let message = Message::Ping(self.current_node.clone());
        let do_send = || async { self.transport.send(&peer.peer_addr, &message).await };
        let with_retry = do_send.retry(
            ConstantBuilder::new()
                .with_delay(DEFAULT_RETRY_INTERVAL)
                .with_max_times(DEFAULT_RETRIES),
        );

        match with_retry.await {
            Ok(msg @ Message::Ack(_)) => {
                self.handle_message(msg);
            }

            _ => {
                self.mark_dead(&peer);
            }
        }
    }

    async fn sync(&self, peer: NodeInfo) {
        let message = Message::Sync {
            members: self.membership().members().values().cloned().collect(),
        };
        let do_send = || async { self.transport.send(&peer.peer_addr, &message).await };
        let with_retry = do_send.retry(
            ConstantBuilder::new()
                .with_delay(DEFAULT_RETRY_INTERVAL)
                .with_max_times(DEFAULT_RETRIES),
        );
        match with_retry.await {
            Ok(msg @ Message::Sync { .. }) => {
                self.handle_message(msg);
            }

            _ => {
                self.mark_dead(&peer);
            }
        }
    }

    async fn fast_bootstrap(&self) {
        for peer in &self.initial_peers {
            let message = Message::Ping(self.current_node.clone());
            let do_send = || async { self.transport.send(peer, &message).await };
            let with_retry = do_send.retry(
                ConstantBuilder::new()
                    .with_delay(DEFAULT_RETRY_INTERVAL)
                    .with_max_times(DEFAULT_RETRIES),
            );
            if let Ok(msg @ Message::Ack(_)) = with_retry.await {
                self.handle_message(msg);
            }
        }

        for peer in &self.initial_peers {
            let message = Message::Sync {
                members: self.membership().members().values().cloned().collect(),
            };
            let do_send = || async { self.transport.send(peer, &message).await };
            let with_retry = do_send.retry(
                ConstantBuilder::new()
                    .with_delay(DEFAULT_RETRY_INTERVAL)
                    .with_max_times(DEFAULT_RETRIES),
            );
            if let Ok(msg @ Message::Sync { .. }) = with_retry.await {
                self.handle_message(msg);
            }
        }

        self.rebuild_ring();
    }

    fn rebuild_ring(&self) {
        let members = self.membership.read().unwrap();

        *self.ring.write().unwrap() = Arc::new(HashRing::from(members.members().keys().cloned()));
    }

    fn mark_dead(&self, peer: &NodeInfo) {
        let mut members = self.membership.write().unwrap();
        if let Some(last_seen) = members.members().get(&peer.id).map(|m| m.heartbeat) {
            let member = MemberState {
                info: peer.clone(),
                status: MemberStatus::Dead,
                heartbeat: last_seen,
            };
            members.update_member(member);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
enum Message {
    Ping(NodeInfo),
    Ack(NodeInfo),

    Sync { members: Vec<MemberState> },
}

struct Transport {
    client: Client,
}

impl Transport {
    pub fn new() -> Self {
        let client = ClientBuilder::new()
            .http2_prior_knowledge()
            .build()
            .unwrap();
        Transport { client }
    }

    pub async fn send(&self, endpoint: &str, message: &Message) -> Result<Message, ClusterError> {
        let make_error =
            || ClusterError::Transport(format!("failed to send message to {endpoint}"));
        let url = Url::parse(endpoint).change_context_lazy(make_error)?;

        let resp = self
            .client
            .post(url)
            .json(message)
            .send()
            .await
            .change_context_lazy(make_error)?;

        if resp.status().is_success() {
            resp.json().await.change_context_lazy(make_error)
        } else {
            bail!(make_error())
        }
    }
}

async fn drive_gossip(state: Arc<GossipState>, runtime: &Runtime) -> Result<(), ClusterError> {
    // Fast bootstrap
    state
        .membership
        .write()
        .unwrap()
        .update_member(MemberState {
            info: state.current_node.clone(),
            status: MemberStatus::Alive,
            heartbeat: Timestamp::now(),
        });
    let state_clone = state.clone();
    runtime
        .spawn(async move {
            state_clone.fast_bootstrap().await;
        })
        .await;

    if state.membership().members().is_empty() {
        bail!(ClusterError::Internal(
            "failed to bootstrap the cluster, no initial peer available".to_string(),
        ))
    }

    // Ping
    let state_clone = state.clone();
    runtime.spawn(async move {
        let state = state_clone;
        let mut ticker = timer().interval(DEFAULT_PING_INTERVAL);
        loop {
            ticker.tick().await;

            let membership = state.membership();
            if let Some((_, member)) = membership
                .members()
                .iter()
                .nth(random::<usize>() % membership.members().len())
            {
                state.ping(member.info.clone()).await;
            } else {
                log::error!("no members found in the cluster");
                state.fast_bootstrap().await;
            }
        }
    });

    // Anti-entropy
    let state_clone = state.clone();
    runtime.spawn(async move {
        let state = state_clone;
        let mut ticker = timer().interval(DEFAULT_SYNC_INTERVAL);
        loop {
            ticker.tick().await;

            let membership = state.membership();
            if let Some((_, member)) = membership
                .members()
                .iter()
                .nth(random::<usize>() % membership.members().len())
            {
                state.sync(member.info.clone()).await;
            } else {
                log::error!("no members found in the cluster");
                state.fast_bootstrap().await;
            }
        }
    });

    // Rebuild ring
    let state_clone = state.clone();
    runtime.spawn(async move {
        let state = state_clone;
        let mut ticker = timer().interval(DEFAULT_REBUILD_RING_INTERVAL);
        loop {
            ticker.tick().await;
            state.rebuild_ring();
        }
    });

    Ok(())
}

#[handler]
async fn gossip(Json(msg): Json<Message>, Data(state): Data<&Arc<GossipState>>) -> Response {
    log::debug!("received message: {:?}", msg);

    if let Some(response) = state.handle_message(msg) {
        Json(response).into_response()
    } else {
        ().into_response()
    }
}
