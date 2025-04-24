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
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use fastimer::schedule::SimpleActionExt;
use mea::shutdown::ShutdownRecv;
use mea::shutdown::ShutdownSend;
use mea::waitgroup::WaitGroup;
use percas_cluster::GossipFuture;
use percas_cluster::Proxy;
use percas_core::Runtime;
use percas_core::timer;
use percas_metrics::GlobalMetrics;
use percas_metrics::OperationMetrics;
use poem::Body;
use poem::EndpointExt;
use poem::Response;
use poem::Route;
use poem::handler;
use poem::http::StatusCode;
use poem::listener::Acceptor;
use poem::listener::Listener;
use poem::listener::TcpAcceptor;
use poem::listener::TcpListener;
use poem::web::Data;
use poem::web::Path;
use poem::web::headers::ContentType;

use crate::PercasContext;
use crate::middleware::ClusterProxyMiddleware;
use crate::middleware::LoggerMiddleware;
use crate::middleware::RateLimitMiddleware;
use crate::scheduled::ReportMetricsAction;

pub(crate) type ServerFuture<T> = tokio::task::JoinHandle<Result<T, io::Error>>;

#[derive(Debug)]
pub struct ServerState {
    advertise_addr: SocketAddr,
    server_fut: ServerFuture<()>,
    gossip_futs: Vec<GossipFuture>,

    shutdown_rx_server: ShutdownRecv,
    shutdown_tx_actions: Vec<ShutdownSend>,
}

impl ServerState {
    pub fn advertise_addr(&self) -> SocketAddr {
        self.advertise_addr
    }

    pub async fn await_shutdown(self) {
        self.shutdown_rx_server.is_shutdown().await;

        log::info!("percas server is shutting down");

        for shutdown in self.shutdown_tx_actions.iter() {
            shutdown.shutdown();
        }
        for shutdown in self.shutdown_tx_actions {
            shutdown.await_shutdown().await;
        }
        log::info!("percas actions shutdown");

        match self.server_fut.await {
            Ok(_) => log::info!("percas server stopped."),
            Err(err) => log::error!(err:?; "percas server failed."),
        }

        match futures_util::future::try_join_all(self.gossip_futs).await {
            Ok(_) => log::info!("percas gossip stopped."),
            Err(err) => log::error!(err:?; "percas gossip failed."),
        }
    }
}

pub async fn make_acceptor_and_advertise_addr(
    listen_addr: &str,
    advertise_addr: Option<&str>,
) -> Result<(TcpAcceptor, SocketAddr), io::Error> {
    log::info!("listening on {}", listen_addr);

    let acceptor = TcpListener::bind(&listen_addr).into_acceptor().await?;
    let listen_addr = acceptor.local_addr()[0]
        .as_socket_addr()
        .cloned()
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::AddrNotAvailable,
                "failed to get local listen addr",
            )
        })?;

    let advertise_addr = match advertise_addr {
        None => {
            if listen_addr.ip().is_unspecified() {
                let ip = local_ip_address::local_ip().map_err(io::Error::other)?;
                let port = listen_addr.port();
                SocketAddr::new(ip, port)
            } else {
                listen_addr
            }
        }
        Some(advertise_addr) => advertise_addr
            .parse::<SocketAddr>()
            .map_err(io::Error::other)?,
    };

    Ok((acceptor, advertise_addr))
}

pub async fn start_server(
    rt: &Runtime,
    shutdown_rx: ShutdownRecv,
    ctx: Arc<PercasContext>,
    acceptor: TcpAcceptor,
    advertise_addr: SocketAddr,
    cluster_proxy: Option<Proxy>,
    gossip_futs: Vec<GossipFuture>,
) -> Result<ServerState, io::Error> {
    let wg = WaitGroup::new();
    let shutdown_rx_server = shutdown_rx;

    let server_fut = {
        let shutdown_clone = shutdown_rx_server.clone();
        let wg_clone = wg.clone();

        let route = Route::new()
            .at(
                "/*key",
                poem::get(get)
                    .put(put)
                    .delete(delete)
                    .with(ClusterProxyMiddleware::new(cluster_proxy)),
            )
            .data(ctx.clone())
            .with(RateLimitMiddleware::new())
            .with(LoggerMiddleware);
        let listen_addr = acceptor.local_addr()[0].clone();
        let signal = async move {
            log::info!("server has started on [{listen_addr}]");
            drop(wg_clone);

            shutdown_clone.is_shutdown().await;
            log::info!("server is closing");
        };

        tokio::spawn(async move {
            poem::Server::new_with_acceptor(acceptor)
                .run_with_graceful_shutdown(route, signal, Some(Duration::from_secs(10)))
                .await
        })
    };

    wg.await;

    // Scheduled actions
    let mut shutdown_tx_actions = vec![];
    let (shutdown_tx, shutdown_rx) = mea::shutdown::new_pair();
    ReportMetricsAction::new(ctx.clone()).schedule_with_fixed_delay(
        async move { shutdown_rx.is_shutdown().await },
        rt,
        timer(),
        None,
        Duration::from_secs(60),
    );
    shutdown_tx_actions.push(shutdown_tx);

    Ok(ServerState {
        advertise_addr,
        server_fut,
        gossip_futs,
        shutdown_rx_server,
        shutdown_tx_actions,
    })
}

pub fn get_success(body: impl Into<Body>) -> Response {
    Response::builder()
        .status(StatusCode::OK)
        .typed_header(ContentType::octet_stream())
        .body(body)
}

pub fn get_not_found() -> Response {
    Response::builder()
        .status(StatusCode::NOT_FOUND)
        .typed_header(ContentType::text())
        .body(StatusCode::NOT_FOUND.to_string())
}

#[handler]
pub async fn get(Data(ctx): Data<&Arc<PercasContext>>, key: Path<String>) -> Response {
    let metrics = &GlobalMetrics::get().operation;
    let start = std::time::Instant::now();

    match ctx.engine.get(key.as_bytes()).await {
        Some(value) => {
            let labels = OperationMetrics::operation_labels(
                OperationMetrics::OPERATION_GET,
                OperationMetrics::STATUS_SUCCESS,
            );
            metrics.count.add(1, &labels);
            metrics.bytes.add(value.len() as u64, &labels);
            metrics
                .duration
                .record(start.elapsed().as_secs_f64(), &labels);

            get_success(value)
        }
        None => {
            let labels = OperationMetrics::operation_labels(
                OperationMetrics::OPERATION_GET,
                OperationMetrics::STATUS_NOT_FOUND,
            );
            metrics.count.add(1, &labels);
            metrics
                .duration
                .record(start.elapsed().as_secs_f64(), &labels);

            Response::builder()
                .status(StatusCode::NOT_FOUND)
                .typed_header(ContentType::text())
                .body(StatusCode::NOT_FOUND.to_string())
        }
    }
}

pub fn put_success() -> Response {
    Response::builder()
        .status(StatusCode::CREATED)
        .typed_header(ContentType::text())
        .body(StatusCode::CREATED.to_string())
}

pub fn put_bad_request() -> Response {
    Response::builder()
        .status(StatusCode::BAD_REQUEST)
        .typed_header(ContentType::text())
        .body(StatusCode::BAD_REQUEST.to_string())
}

#[handler]
pub async fn put(Data(ctx): Data<&Arc<PercasContext>>, key: Path<String>, body: Body) -> Response {
    let metrics = &GlobalMetrics::get().operation;
    let start = std::time::Instant::now();

    match body.into_bytes().await.map(|bytes| {
        ctx.engine.put(key.as_bytes(), &bytes);
        bytes.len()
    }) {
        Ok(len) => {
            let labels = OperationMetrics::operation_labels(
                OperationMetrics::OPERATION_PUT,
                OperationMetrics::STATUS_SUCCESS,
            );
            metrics.count.add(1, &labels);
            metrics.bytes.add(len as u64, &labels);
            metrics
                .duration
                .record(start.elapsed().as_secs_f64(), &labels);

            put_success()
        }
        Err(_) => {
            let labels = OperationMetrics::operation_labels(
                OperationMetrics::OPERATION_PUT,
                OperationMetrics::STATUS_FAILURE,
            );
            metrics.count.add(1, &labels);
            metrics
                .duration
                .record(start.elapsed().as_secs_f64(), &labels);

            put_bad_request()
        }
    }
}

pub fn delete_success() -> Response {
    Response::builder().status(StatusCode::NO_CONTENT).finish()
}

#[handler]
pub async fn delete(Data(ctx): Data<&Arc<PercasContext>>, key: Path<String>) -> Response {
    let metrics = &GlobalMetrics::get().operation;
    let start = std::time::Instant::now();
    ctx.engine.delete(key.as_bytes());

    let labels = OperationMetrics::operation_labels(
        OperationMetrics::OPERATION_DELETE,
        OperationMetrics::STATUS_SUCCESS,
    );
    metrics.count.add(1, &labels);
    metrics
        .duration
        .record(start.elapsed().as_secs_f64(), &labels);

    delete_success()
}
