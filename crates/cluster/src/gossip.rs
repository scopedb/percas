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

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::RwLock;
use std::time::Duration;

use backon::ConstantBuilder;
use backon::Retryable;
use exn::Result;
use exn::ResultExt;
use exn::bail;
use fastimer::MakeDelayExt;
use jiff::Timestamp;
use mea::shutdown::ShutdownRecv;
use mea::waitgroup::WaitGroup;
use percas_core::JoinHandle;
use percas_core::Runtime;
use percas_core::node_file_path;
use percas_core::timer;
use poem::EndpointExt;
use poem::IntoResponse;
use poem::Response;
use poem::Route;
use poem::handler;
use poem::listener::Acceptor;
use poem::listener::TcpAcceptor;
use poem::web::Data;
use poem::web::Json;
use rand::Rng;
use rand::SeedableRng;
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
use crate::node::NodeInfo;
use crate::ring::HashRing;

const DEFAULT_PING_INTERVAL: Duration = Duration::from_secs(1);
const DEFAULT_SYNC_INTERVAL: Duration = Duration::from_secs(5);

const DEFAULT_RETRY_INTERVAL: Duration = Duration::from_secs(1);
const DEFAULT_RETRIES: usize = 3;

const DEFAULT_REBUILD_RING_INTERVAL: Duration = Duration::from_secs(5);

const DEFAULT_MEMBER_DEADLINE: Duration = Duration::from_secs(30);

pub type GossipFuture = JoinHandle<Result<(), ClusterError>>;

#[derive(Debug)]
pub struct GossipState {
    dir: PathBuf,
    initial_peers: Vec<String>,
    current_node: RwLock<NodeInfo>,
    transport: Transport,

    membership: RwLock<Membership>,
    ring: RwLock<Arc<HashRing<Uuid>>>,
}

impl GossipState {
    pub fn new(current_node: NodeInfo, initial_peers: Vec<String>, dir: PathBuf) -> Self {
        let current_node = RwLock::new(current_node);
        let members = RwLock::new(Membership::default());
        let transport = Transport::new();
        let ring = RwLock::new(Arc::new(HashRing::default()));
        Self {
            dir,
            initial_peers,
            current_node,
            membership: members,
            transport,
            ring,
        }
    }

    pub fn current(&self) -> NodeInfo {
        self.current_node.read().unwrap().clone()
    }

    pub fn membership(&self) -> Membership {
        self.membership.read().unwrap().clone()
    }

    pub fn ring(&self) -> Arc<HashRing<Uuid>> {
        self.ring.read().unwrap().clone()
    }

    pub async fn start(
        self: Arc<Self>,
        rt: &Runtime,
        shutdown_rx: ShutdownRecv,
        acceptor: TcpAcceptor,
    ) -> Result<Vec<GossipFuture>, ClusterError> {
        let wg = WaitGroup::new();
        let route = Route::new()
            .at("/gossip", poem::post(gossip).data(self.clone()))
            .at("/members", poem::get(list_members).data(self.clone()));

        let mut gossip_futs = vec![];

        // Listen on the peer address
        let server_fut = {
            let wg = wg.clone();
            let shutdown = shutdown_rx.clone();
            rt.spawn(async move {
                let listen_addr = acceptor.local_addr()[0].clone();
                let signal = async move {
                    log::info!("gossip proxy has started on [{listen_addr}]");
                    drop(wg);

                    shutdown.is_shutdown().await;
                    log::info!("gossip proxy is closing");
                };
                poem::Server::new_with_acceptor(acceptor)
                    .run_with_graceful_shutdown(route, signal, Some(Duration::from_secs(10)))
                    .await
                    .or_raise(|| ClusterError("failed to run gossip proxy".to_string()))
            })
        };
        wg.await;
        gossip_futs.push(server_fut);

        // Start the gossip protocol
        drive_gossip(rt, shutdown_rx, &mut gossip_futs, self).await?;

        Ok(gossip_futs)
    }

    fn handle_message(&self, message: Message) -> Option<Message> {
        log::debug!("received message: {message:?}");
        let result = match message {
            Message::Ping(info) => {
                self.membership.write().unwrap().update_member(MemberState {
                    info: info.clone(),
                    status: MemberStatus::Alive,
                    heartbeat: Timestamp::now(),
                });

                // Respond with an ack
                Some(Message::Ack(self.current()))
            }
            Message::Ack(info) => {
                self.membership.write().unwrap().update_member(MemberState {
                    info: info.clone(),
                    status: MemberStatus::Alive,
                    heartbeat: Timestamp::now(),
                });

                None
            }
            Message::Sync { members } => {
                for member in members {
                    self.membership.write().unwrap().update_member(member);
                }

                // Ensure the current node is alive
                self.membership.write().unwrap().update_member(MemberState {
                    info: self.current(),
                    status: MemberStatus::Alive,
                    heartbeat: Timestamp::now(),
                });

                // Respond with the current membership
                let members = self.membership.read().unwrap().members().clone();
                Some(Message::Sync {
                    members: members.values().cloned().collect(),
                })
            }
        };

        if self
            .membership
            .read()
            .unwrap()
            .is_dead(self.current().node_id)
        {
            log::info!("current node is marked as dead; advancing incarnation");
            self.advance_incarnation();
        }

        result
    }

    fn advance_incarnation(&self) {
        let mut current = self.current_node.write().unwrap();
        current.advance_incarnation();
        current.persist(&node_file_path(&self.dir));
    }

    fn remove_dead_members(&self) -> Vec<NodeInfo> {
        let mut members = self.membership.write().unwrap();
        let dead_members: Vec<NodeInfo> = members
            .members()
            .iter()
            .filter_map(|(_, member)| {
                if member.status == MemberStatus::Dead
                    && member.heartbeat + DEFAULT_MEMBER_DEADLINE < Timestamp::now()
                {
                    Some(member.info.clone())
                } else {
                    None
                }
            })
            .collect();

        for dead_member in &dead_members {
            members.remove_member(dead_member.node_id);
        }

        dead_members
    }

    async fn ping(&self, peer: NodeInfo) {
        let message = Message::Ping(self.current());
        let do_send = || async {
            self.transport
                .send(&peer.advertise_peer_addr, &message)
                .await
                .inspect_err(|e| log::error!("failed to send ping message: {e:?}"))
        };
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
        let do_send = || async {
            self.transport
                .send(&peer.advertise_peer_addr, &message)
                .await
                .inspect_err(|e| log::error!("failed to send sync message: {e:?}"))
        };
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
            let message = Message::Ping(self.current());
            let do_send = || async {
                self.transport
                    .send(peer, &message)
                    .await
                    .inspect_err(|e| log::error!("failed to send ping message: {e:?}"))
            };
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
            let do_send = || async {
                self.transport
                    .send(peer, &message)
                    .await
                    .inspect_err(|e| log::error!("failed to send sync message: {e:?}"))
            };
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
        // Ensure the current node is alive
        let mut membership = self.membership.write().unwrap();
        membership.update_member(MemberState {
            info: self.current(),
            status: MemberStatus::Alive,
            heartbeat: Timestamp::now(),
        });

        *self.ring.write().unwrap() =
            Arc::new(HashRing::from(membership.members().keys().cloned()));
    }

    fn mark_dead(&self, peer: &NodeInfo) {
        let mut members = self.membership.write().unwrap();
        if let Some(last_seen) = members.members().get(&peer.node_id).map(|m| m.heartbeat) {
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

#[derive(Debug)]
struct Transport {
    client: Client,
}

impl Transport {
    pub fn new() -> Self {
        let client = ClientBuilder::new().build().unwrap();
        Transport { client }
    }

    pub async fn send(&self, endpoint: &str, message: &Message) -> Result<Message, ClusterError> {
        let make_error = || ClusterError(format!("failed to send message to {endpoint}"));

        let url = Url::parse(&format!("http://{endpoint}"))
            .and_then(|url| url.join("gossip"))
            .or_raise(make_error)?;

        let resp = self
            .client
            .post(url)
            .json(message)
            .send()
            .await
            .or_raise(make_error)?;

        if resp.status().is_success() {
            resp.json().await.or_raise(make_error)
        } else {
            bail!(make_error())
        }
    }
}

async fn drive_gossip(
    rt: &Runtime,
    shutdown_rx: ShutdownRecv,
    gossip_futs: &mut Vec<GossipFuture>,
    state: Arc<GossipState>,
) -> Result<(), ClusterError> {
    // Fast bootstrap
    state
        .membership
        .write()
        .unwrap()
        .update_member(MemberState {
            info: state.current(),
            status: MemberStatus::Alive,
            heartbeat: Timestamp::now(),
        });

    let state_clone = state.clone();
    rt.spawn(async move {
        state_clone.fast_bootstrap().await;
    })
    .await;

    if state.membership().members().is_empty() {
        bail!(ClusterError(
            "failed to bootstrap the cluster: no initial peer available".to_string(),
        ))
    }

    // Ping
    let state_clone = state.clone();
    let shutdown_rx_clone = shutdown_rx.clone();
    let mut rng = rand::rngs::StdRng::from_os_rng();
    let ping_fut = rt.spawn(async move {
        let fut = async move {
            let state = state_clone;
            let mut ticker = timer().interval(DEFAULT_PING_INTERVAL);
            loop {
                ticker.tick().await;

                let membership = state.membership();
                if let Some((_, member)) = membership
                    .members()
                    .iter()
                    .nth(rng.random_range(0..membership.members().len()))
                {
                    if member.status == MemberStatus::Dead {
                        log::debug!("skipping dead member: {member:?}");
                        continue;
                    }
                    log::debug!("pinging member: {member:?}");
                    state.ping(member.info.clone()).await;
                } else {
                    log::error!("no members found in the cluster");
                    state.fast_bootstrap().await;
                }
            }
        };

        tokio::select! {
            _ = fut => Ok(()),
            _ = shutdown_rx_clone.is_shutdown() => {
                log::info!("gossip ping task is shutting down");
                Ok(())
            }
        }
    });
    gossip_futs.push(ping_fut);

    // Anti-entropy
    let state_clone = state.clone();
    let shutdown_rx_clone = shutdown_rx.clone();
    let mut rng = rand::rngs::StdRng::from_os_rng();
    let anti_entropy_fut = rt.spawn(async move {
        let fut = async move {
            let state = state_clone;
            let mut ticker = timer().interval(DEFAULT_SYNC_INTERVAL);
            loop {
                ticker.tick().await;
                let membership = state.membership();
                if let Some((_, member)) = membership
                    .members()
                    .iter()
                    .nth(rng.random_range(0..membership.members().len()))
                {
                    if member.status == MemberStatus::Dead {
                        log::debug!("skipping dead member: {member:?}");
                        continue;
                    }
                    log::debug!("syncing member: {member:?}");
                    state.sync(member.info.clone()).await;
                } else {
                    log::error!("no members found in the cluster");
                    state.fast_bootstrap().await;
                }
            }
        };

        tokio::select! {
            _ = fut => Ok(()),
            _ = shutdown_rx_clone.is_shutdown() => {
                log::info!("gossip anti-entropy task is shutting down");
                Ok(())
            }
        }
    });
    gossip_futs.push(anti_entropy_fut);

    // Rebuild ring
    let state_clone = state.clone();
    let shutdown_rx_clone = shutdown_rx.clone();
    let rebuild_ring_fut = rt.spawn(async move {
        let fut = async move {
            let state = state_clone;
            let mut ticker = timer().interval(DEFAULT_REBUILD_RING_INTERVAL);
            loop {
                ticker.tick().await;
                state.rebuild_ring();
            }
        };

        tokio::select! {
            _ = fut => Ok(()),
            _ = shutdown_rx_clone.is_shutdown() => {
                log::info!("gossip rebuild ring task is shutting down");
                Ok(())
            }
        }
    });
    gossip_futs.push(rebuild_ring_fut);

    // Remove dead members
    let state_clone = state.clone();
    let shutdown_rx_clone = shutdown_rx.clone();
    let remove_dead_members_fut = rt.spawn(async move {
        let fut = async move {
            let state = state_clone;
            let mut ticker = timer().interval(DEFAULT_MEMBER_DEADLINE);
            loop {
                ticker.tick().await;
                let dead_members = state.remove_dead_members();
                if !dead_members.is_empty() {
                    log::info!("removed dead members: {dead_members:?}");
                    state.rebuild_ring();
                }
            }
        };

        tokio::select! {
            _ = fut => Ok(()),
            _ = shutdown_rx_clone.is_shutdown() => {
                log::info!("gossip remove dead members task is shutting down");
                Ok(())
            }
        }
    });
    gossip_futs.push(remove_dead_members_fut);

    Ok(())
}

#[handler]
async fn gossip(Json(msg): Json<Message>, Data(state): Data<&Arc<GossipState>>) -> Response {
    log::debug!("received message: {msg:?}");

    if let Some(response) = state.handle_message(msg) {
        Json(response).into_response()
    } else {
        ().into_response()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
struct Member {
    node_id: Uuid,
    cluster_id: String,
    advertise_addr: String,
    advertise_peer_addr: String,
    incarnation: u64,
    status: MemberStatus,
    heartbeat: Timestamp,
    vnodes: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
struct ListMembersResponse {
    members: Vec<Member>,
}

#[handler]
async fn list_members(Data(state): Data<&Arc<GossipState>>) -> Response {
    let resp = ListMembersResponse {
        members: state
            .membership()
            .members()
            .values()
            .map(|m| Member {
                node_id: m.info.node_id,
                cluster_id: m.info.cluster_id.clone(),
                advertise_addr: m.info.advertise_addr.clone(),
                advertise_peer_addr: m.info.advertise_peer_addr.clone(),
                incarnation: m.info.incarnation,
                status: m.status,
                heartbeat: m.heartbeat,
                vnodes: state.ring().list_vnodes(&m.info.node_id),
            })
            .collect(),
    };
    Json(resp).into_response()
}

#[cfg(test)]
mod tests {
    use insta::assert_json_snapshot;
    use jiff::Timestamp;
    use uuid::Uuid;

    use crate::gossip::ListMembersResponse;
    use crate::gossip::Member;
    use crate::member::MemberStatus;

    #[test]
    fn test_list_members_serde() {
        let resp = ListMembersResponse {
            members: vec![Member {
                node_id: Uuid::nil(),
                cluster_id: "cluster".to_string(),
                advertise_addr: "127.0.0.1:7654".to_string(),
                advertise_peer_addr: "127.0.0.1:7655".to_string(),
                incarnation: 2,
                status: MemberStatus::Alive,
                heartbeat: Timestamp::constant(123, 456),
                vnodes: vec![1, 2, 3],
            }],
        };
        assert_json_snapshot!(
            resp,
            @r#"
            {
              "members": [
                {
                  "node_id": "00000000-0000-0000-0000-000000000000",
                  "cluster_id": "cluster",
                  "advertise_addr": "127.0.0.1:7654",
                  "advertise_peer_addr": "127.0.0.1:7655",
                  "incarnation": 2,
                  "status": "alive",
                  "heartbeat": "1970-01-01T00:02:03.000000456Z",
                  "vnodes": [
                    1,
                    2,
                    3
                  ]
                }
              ]
            }
            "#
        );
    }
}
