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

use arc_swap::ArcSwap;
use backon::ConstantBuilder;
use backon::Retryable;
use exn::Result;
use exn::ResultExt;
use exn::bail;
use exn::ensure;
use fastimer::MakeDelayExt;
use jiff::Timestamp;
use mea::shutdown::ShutdownRecv;
use percas_core::JoinHandle;
use percas_core::Runtime;
use percas_core::node_file_path;
use percas_core::timer;
use rand::Rng;
use rand::SeedableRng;
use reqwest::Client;
use reqwest::Url;
use serde::Deserialize;
use serde::Serialize;
use uuid::Uuid;

use crate::GossipError;
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

pub type GossipFuture = JoinHandle<Result<(), GossipError>>;

#[derive(Debug)]
pub struct GossipState {
    dir: PathBuf,
    initial_peers: Vec<Url>,
    current_node: RwLock<NodeInfo>,
    transport: Transport,

    membership: ArcSwap<Membership>,
    ring: ArcSwap<HashRing<Uuid>>,
}

impl GossipState {
    pub fn new(current_node: NodeInfo, initial_peers: Vec<Url>, dir: PathBuf) -> Self {
        Self {
            dir,
            initial_peers,
            current_node: RwLock::new(current_node),
            membership: ArcSwap::new(Arc::new(Membership::default())),
            transport: Transport::new(),
            ring: ArcSwap::new(Arc::new(HashRing::default())),
        }
    }

    pub fn current(&self) -> NodeInfo {
        self.current_node.read().unwrap().clone()
    }

    pub fn membership(&self) -> Arc<Membership> {
        self.membership.load_full()
    }

    pub fn ring(&self) -> Arc<HashRing<Uuid>> {
        self.ring.load_full()
    }

    /// Start the gossip protocol.
    pub async fn start(
        self: Arc<Self>,
        rt: &Runtime,
        shutdown_rx: ShutdownRecv,
    ) -> Result<Vec<GossipFuture>, GossipError> {
        let mut gossip_futs = vec![];

        // Fast bootstrap
        self.membership
            .store(Arc::new(Membership::from_iter([MemberState {
                info: self.current(),
                status: MemberStatus::Alive,
                heartbeat: Timestamp::now(),
            }])));

        let state_clone = self.clone();
        rt.spawn(async move {
            state_clone.fast_bootstrap().await;
        })
        .await;

        if self.membership().members().is_empty() {
            bail!(GossipError(
                "failed to bootstrap the cluster: no initial peer available".to_string(),
            ))
        }

        // Ping
        let state_clone = self.clone();
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
        let state_clone = self.clone();
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
        let state_clone = self.clone();
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
        let state_clone = self.clone();
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

        Ok(gossip_futs)
    }

    pub fn handle_message(&self, message: GossipMessage) -> Option<GossipMessage> {
        log::debug!("received message: {message:?}");
        let result = match message {
            GossipMessage::Ping(info) => {
                let mut membership = (**self.membership.load()).clone();
                membership.update_member(MemberState {
                    info: info.clone(),
                    status: MemberStatus::Alive,
                    heartbeat: Timestamp::now(),
                });
                self.membership.store(Arc::new(membership));

                // Respond with an ack
                Some(GossipMessage::Ack(self.current()))
            }
            GossipMessage::Ack(info) => {
                let mut membership = (**self.membership.load()).clone();
                membership.update_member(MemberState {
                    info: info.clone(),
                    status: MemberStatus::Alive,
                    heartbeat: Timestamp::now(),
                });
                self.membership.store(Arc::new(membership));

                None
            }
            GossipMessage::Sync { members } => {
                let mut membership = (**self.membership.load()).clone();
                for member in members {
                    membership.update_member(member);
                }

                // Ensure the current node is alive
                membership.update_member(MemberState {
                    info: self.current(),
                    status: MemberStatus::Alive,
                    heartbeat: Timestamp::now(),
                });

                self.membership.store(Arc::new(membership.clone()));

                // Respond with the current membership
                Some(GossipMessage::Sync {
                    members: membership.into_members().into_values().collect(),
                })
            }
        };

        if self.membership.load().is_dead(self.current().node_id) {
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
        let mut members = (**self.membership.load()).clone();
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

        self.membership.store(Arc::new(members));

        dead_members
    }

    async fn ping(&self, peer: NodeInfo) {
        let message = GossipMessage::Ping(self.current());
        let do_send = || async {
            self.transport
                .send(&peer.advertise_ctrl_url, &message)
                .await
                .inspect_err(|e| log::error!("failed to send ping message: {e:?}"))
        };
        let with_retry = do_send.retry(
            ConstantBuilder::new()
                .with_delay(DEFAULT_RETRY_INTERVAL)
                .with_max_times(DEFAULT_RETRIES),
        );
        if let Ok(msg @ GossipMessage::Ack(_)) = with_retry.await {
            self.handle_message(msg);
        } else {
            self.mark_dead(&peer);
        }
    }

    async fn sync(&self, peer: NodeInfo) {
        let message = GossipMessage::Sync {
            members: self.membership().members().values().cloned().collect(),
        };
        let do_send = || async {
            self.transport
                .send(&peer.advertise_ctrl_url, &message)
                .await
                .inspect_err(|e| log::error!("failed to send sync message: {e:?}"))
        };
        let with_retry = do_send.retry(
            ConstantBuilder::new()
                .with_delay(DEFAULT_RETRY_INTERVAL)
                .with_max_times(DEFAULT_RETRIES),
        );
        if let Ok(msg @ GossipMessage::Sync { .. }) = with_retry.await {
            self.handle_message(msg);
        } else {
            self.mark_dead(&peer);
        }
    }

    async fn fast_bootstrap(&self) {
        for peer in &self.initial_peers {
            let message = GossipMessage::Ping(self.current());
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
            if let Ok(msg @ GossipMessage::Ack(_)) = with_retry.await {
                self.handle_message(msg);
            }
        }

        for peer in &self.initial_peers {
            let message = GossipMessage::Sync {
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
            if let Ok(msg @ GossipMessage::Sync { .. }) = with_retry.await {
                self.handle_message(msg);
            }
        }

        self.rebuild_ring();
    }

    fn rebuild_ring(&self) {
        // Ensure the current node is alive
        let mut membership = (**self.membership.load()).clone();
        membership.update_member(MemberState {
            info: self.current(),
            status: MemberStatus::Alive,
            heartbeat: Timestamp::now(),
        });

        self.ring.store(Arc::new(HashRing::from(
            membership.members().keys().cloned(),
        )));
    }

    fn mark_dead(&self, peer: &NodeInfo) {
        let mut members = (**self.membership.load()).clone();
        if let Some(last_seen) = members.members().get(&peer.node_id).map(|m| m.heartbeat) {
            let member = MemberState {
                info: peer.clone(),
                status: MemberStatus::Dead,
                heartbeat: last_seen,
            };
            members.update_member(member);
        }
        self.membership.store(Arc::new(members));
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GossipMessage {
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
        Transport {
            client: Client::new(),
        }
    }

    pub async fn send(
        &self,
        url: &Url,
        message: &GossipMessage,
    ) -> Result<GossipMessage, GossipError> {
        let make_error = || GossipError(format!("failed to send message to {url}"));
        let url = url.join("gossip").or_raise(make_error)?;
        let resp = self
            .client
            .post(url)
            .json(message)
            .send()
            .await
            .or_raise(make_error)?;
        ensure!(resp.status().is_success(), make_error());
        resp.json().await.or_raise(make_error)
    }
}
