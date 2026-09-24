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

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;
use std::time::Instant;

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
use rand::RngExt;
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

const SUSPICION_GRACE: Duration = Duration::from_secs(5);
const PROBE_TIMEOUT: Duration = Duration::from_secs(2);

const DEFAULT_MEMBER_DEADLINE: Duration = Duration::from_secs(30);

pub type GossipFuture = JoinHandle<Result<(), GossipError>>;

#[derive(Debug)]
pub struct GossipState {
    dir: PathBuf,
    initial_peers: Vec<Url>,
    current_node: ArcSwap<NodeInfo>,
    transport: Transport,

    view: ArcSwap<ClusterView>,
    writer: Mutex<BTreeMap<Uuid, Instant>>,
}

#[derive(Debug)]
pub(crate) struct ClusterView {
    pub membership: Arc<Membership>,
    pub ring: Arc<HashRing<Uuid>>,
}

impl GossipState {
    pub fn new(current_node: NodeInfo, initial_peers: Vec<Url>, dir: PathBuf) -> Self {
        Self {
            dir,
            initial_peers,
            current_node: ArcSwap::new(Arc::new(current_node)),
            view: ArcSwap::from_pointee(ClusterView {
                membership: Arc::new(Membership::default()),
                ring: Arc::new(HashRing::default()),
            }),
            writer: Mutex::new(BTreeMap::new()),
            transport: Transport::new(),
        }
    }

    pub fn current(&self) -> NodeInfo {
        (**self.current_node.load()).clone()
    }

    pub fn membership(&self) -> Arc<Membership> {
        self.view.load().membership.clone()
    }

    pub fn ring(&self) -> Arc<HashRing<Uuid>> {
        self.view.load().ring.clone()
    }

    pub(crate) fn snapshot(&self) -> Arc<ClusterView> {
        self.view.load_full()
    }

    // Call only while holding writer. Readers see one coherent membership/ring.
    fn publish(&self, membership: Membership) {
        let previous = self.view.load();
        let ring = if previous
            .membership
            .members()
            .keys()
            .eq(membership.members().keys())
        {
            previous.ring.clone()
        } else {
            Arc::new(HashRing::from(membership.members().keys().copied()))
        };
        self.view.store(Arc::new(ClusterView {
            membership: Arc::new(membership),
            ring,
        }));
    }

    /// Start the gossip protocol.
    pub async fn start(
        self: Arc<Self>,
        rt: &Runtime,
        shutdown_rx: ShutdownRecv,
    ) -> Result<Vec<GossipFuture>, GossipError> {
        let mut gossip_futs = vec![];

        // Fast bootstrap
        {
            let _writer = self.writer.lock().unwrap();
            let mut membership = (*self.membership()).clone();
            membership.update_member(MemberState {
                info: self.current(),
                status: MemberStatus::Alive,
                heartbeat: Timestamp::now(),
            });
            self.publish(membership);
        }

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
        let mut rng = rand::rngs::StdRng::from_rng(&mut rand::rng());
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
                        .nth(rng.random_range(0..membership.members().len().max(1)))
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
        let mut rng = rand::rngs::StdRng::from_rng(&mut rand::rng());
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
                        .nth(rng.random_range(0..membership.members().len().max(1)))
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
        let mut writer = self.writer.lock().unwrap();
        let cluster_id = self.current().cluster_id;
        match &message {
            GossipMessage::Ping(info) | GossipMessage::Ack(info)
                if info.cluster_id != cluster_id =>
            {
                return None;
            }
            _ => {}
        }
        let result = match message {
            GossipMessage::Ping(info) => {
                let mut membership = (*self.membership()).clone();
                membership.update_member(MemberState {
                    info: info.clone(),
                    status: MemberStatus::Alive,
                    heartbeat: Timestamp::now(),
                });
                self.publish(membership);

                // Respond with an ack
                Some(GossipMessage::Ack(self.current()))
            }
            GossipMessage::Ack(info) => {
                let mut membership = (*self.membership()).clone();
                membership.update_member(MemberState {
                    info: info.clone(),
                    status: MemberStatus::Alive,
                    heartbeat: Timestamp::now(),
                });
                self.publish(membership);

                None
            }
            GossipMessage::Sync { members } => {
                let mut membership = (*self.membership()).clone();
                for member in members {
                    if member.info.cluster_id != cluster_id {
                        continue;
                    }
                    membership.update_member(member);
                }

                // Ensure the current node is alive
                membership.update_member(MemberState {
                    info: self.current(),
                    status: MemberStatus::Alive,
                    heartbeat: Timestamp::now(),
                });

                self.publish(membership.clone());

                // Respond with the current membership
                Some(GossipMessage::Sync {
                    members: membership.into_members().into_values().collect(),
                })
            }
        };

        if self
            .membership()
            .members()
            .get(&self.current().node_id)
            .is_some_and(|member| member.status != MemberStatus::Alive)
        {
            log::info!("current node is marked as dead; advancing incarnation");
            self.advance_incarnation();
            let mut membership = (*self.membership()).clone();
            membership.update_member(MemberState {
                info: self.current(),
                status: MemberStatus::Alive,
                heartbeat: Timestamp::now(),
            });
            self.publish(membership);
        }

        writer.retain(|id, _| {
            self.membership()
                .members()
                .get(id)
                .is_some_and(|member| member.status == MemberStatus::Suspect)
        });
        result
    }

    fn advance_incarnation(&self) {
        let mut current = self.current();
        current.advance_incarnation();
        current.persist(&node_file_path(&self.dir));
        self.current_node.store(Arc::new(current));
    }

    fn remove_dead_members(&self) -> Vec<NodeInfo> {
        let mut writer = self.writer.lock().unwrap();
        let mut members = (*self.membership()).clone();
        let dead_members: Vec<NodeInfo> = members
            .members()
            .values()
            .filter_map(|member| {
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
            writer.remove(&dead_member.node_id);
        }

        self.publish(members);

        dead_members
    }

    async fn ping(&self, peer: NodeInfo) {
        let observed = self
            .membership()
            .members()
            .get(&peer.node_id)
            .map(|member| member.heartbeat);
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
            self.mark_dead(&peer, observed);
        }
    }

    async fn sync(&self, peer: NodeInfo) {
        let observed = self
            .membership()
            .members()
            .get(&peer.node_id)
            .map(|member| member.heartbeat);
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
            self.mark_dead(&peer, observed);
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
        let _writer = self.writer.lock().unwrap();
        // Ensure the current node is alive
        let mut membership = (*self.membership()).clone();
        membership.update_member(MemberState {
            info: self.current(),
            status: MemberStatus::Alive,
            heartbeat: Timestamp::now(),
        });

        self.publish(membership);
    }

    fn mark_dead(&self, peer: &NodeInfo, observed: Option<Timestamp>) {
        let mut writer = self.writer.lock().unwrap();
        if peer.node_id == self.current().node_id {
            return;
        }
        let mut members = (*self.membership()).clone();
        if let Some(current) = members.members().get(&peer.node_id).cloned() {
            // Do not apply a failed probe from an older node incarnation.
            if current.info.incarnation != peer.incarnation || Some(current.heartbeat) != observed {
                return;
            }
            let since = writer.entry(peer.node_id).or_insert_with(Instant::now);
            let status = if since.elapsed() >= SUSPICION_GRACE {
                MemberStatus::Dead
            } else {
                MemberStatus::Suspect
            };
            members.update_member(MemberState { status, ..current });
            self.publish(members);
        }
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
            client: Client::builder()
                .no_proxy()
                .connect_timeout(Duration::from_millis(500))
                .timeout(PROBE_TIMEOUT)
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .expect("failed to build gossip HTTP client"),
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

#[cfg(test)]
mod tests {
    use super::*;

    fn node() -> NodeInfo {
        NodeInfo::new(
            Uuid::now_v7(),
            "cluster".into(),
            "http://127.0.0.1:7654".parse().unwrap(),
            "http://127.0.0.1:7655".parse().unwrap(),
        )
    }

    #[test]
    fn concurrent_updates_preserve_every_member() {
        let dir = tempfile::tempdir().unwrap();
        let state = Arc::new(GossipState::new(node(), vec![], dir.path().to_path_buf()));
        let barrier = Arc::new(std::sync::Barrier::new(32));
        let threads: Vec<_> = (0..32)
            .map(|_| {
                let state = state.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    state.handle_message(GossipMessage::Ping(node()));
                })
            })
            .collect();
        for thread in threads {
            thread.join().unwrap();
        }
        let view = state.snapshot();
        assert_eq!(view.membership.members().len(), 32);
        assert!(
            view.membership
                .members()
                .contains_key(&view.ring.lookup("key").unwrap())
        );
    }

    #[test]
    fn failure_requires_a_grace_period_and_recovery_clears_suspicion() {
        let dir = tempfile::tempdir().unwrap();
        let state = GossipState::new(node(), vec![], dir.path().to_path_buf());
        let peer = node();
        state.handle_message(GossipMessage::Ping(peer.clone()));
        state.mark_dead(
            &peer,
            state
                .membership()
                .members()
                .get(&peer.node_id)
                .map(|member| member.heartbeat),
        );
        assert_eq!(
            state.membership().members()[&peer.node_id].status,
            MemberStatus::Suspect
        );
        state.handle_message(GossipMessage::Ack(peer.clone()));
        assert_eq!(
            state.membership().members()[&peer.node_id].status,
            MemberStatus::Alive
        );
        assert!(!state.writer.lock().unwrap().contains_key(&peer.node_id));
        state.mark_dead(
            &peer,
            state
                .membership()
                .members()
                .get(&peer.node_id)
                .map(|member| member.heartbeat),
        );
        state
            .writer
            .lock()
            .unwrap()
            .insert(peer.node_id, Instant::now() - SUSPICION_GRACE);
        state.mark_dead(
            &peer,
            state
                .membership()
                .members()
                .get(&peer.node_id)
                .map(|member| member.heartbeat),
        );
        assert_eq!(
            state.membership().members()[&peer.node_id].status,
            MemberStatus::Dead
        );
    }

    #[test]
    fn stale_probe_failure_cannot_override_successful_contact() {
        let dir = tempfile::tempdir().unwrap();
        let state = GossipState::new(node(), vec![], dir.path().to_path_buf());
        let peer = node();
        state.handle_message(GossipMessage::Ping(peer.clone()));
        let old = state.membership().members()[&peer.node_id].heartbeat;
        {
            let _writer = state.writer.lock().unwrap();
            let mut membership = (*state.membership()).clone();
            membership.update_member(MemberState {
                info: peer.clone(),
                status: MemberStatus::Alive,
                heartbeat: old + Duration::from_secs(1),
            });
            state.publish(membership);
        }
        state.mark_dead(&peer, Some(old));
        assert_eq!(
            state.membership().members()[&peer.node_id].status,
            MemberStatus::Alive
        );
    }

    #[tokio::test]
    async fn stalled_peer_has_a_bounded_probe() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap())
            .parse()
            .unwrap();
        let transport = Transport::new();
        let result = tokio::time::timeout(
            PROBE_TIMEOUT + Duration::from_secs(1),
            transport.send(&url, &GossipMessage::Ping(node())),
        )
        .await;
        assert!(result.unwrap().is_err());
    }

    #[test]
    fn foreign_cluster_cannot_join() {
        let dir = tempfile::tempdir().unwrap();
        let state = GossipState::new(node(), vec![], dir.path().to_path_buf());
        let mut peer = node();
        peer.cluster_id = "other".into();
        assert!(state.handle_message(GossipMessage::Ping(peer)).is_none());
        assert!(state.membership().members().is_empty());
    }
}
