use std::io;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use atrium_core::Config;
use mea::latch::Latch;
use mea::waitgroup::WaitGroup;
use poem::Body;
use poem::Endpoint;
use poem::EndpointExt;
use poem::IntoResponse;
use poem::Middleware;
use poem::Request;
use poem::Response;
use poem::Route;
use poem::handler;
use poem::http::StatusCode;
use poem::listener::Acceptor;
use poem::listener::Listener;
use poem::listener::TcpListener;
use poem::web::Data;
use poem::web::Query;
use poem::web::headers::ContentType;
use serde::Deserialize;
use serde::Serialize;

use crate::Context;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub port: u16,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self { port: 7654 }
    }
}

struct LoggerMiddleware;

impl<E> Middleware<E> for LoggerMiddleware
where
    E: Endpoint,
    E::Output: IntoResponse,
{
    type Output = LoggerEndpoint<E>;

    fn transform(&self, endpoint: E) -> Self::Output {
        LoggerEndpoint(endpoint)
    }
}

struct LoggerEndpoint<E>(E);

impl<E> Endpoint for LoggerEndpoint<E>
where
    E: Endpoint,
    E::Output: IntoResponse,
{
    type Output = Response;

    async fn call(&self, req: Request) -> Result<Self::Output, poem::Error> {
        log::debug!("{} {}", req.method(), req.uri());
        let resp = self.0.call(req).await.inspect_err(|err| {
            log::error!("{}", err);
        })?;
        let resp = resp.into_response();
        log::debug!("{}", resp.status());
        Ok(resp)
    }
}

pub(crate) type ServerFuture<T> = tokio::task::JoinHandle<Result<T, io::Error>>;

#[derive(Debug)]
pub struct ServerState {
    server_advertise_addr: SocketAddr,
    server_fut: ServerFuture<()>,
    shutdown: Arc<Latch>,
}

impl ServerState {
    pub fn server_advertise_addr(&self) -> SocketAddr {
        self.server_advertise_addr
    }

    pub fn shutdown_handle(&self) -> impl Fn() {
        let shutdown = self.shutdown.clone();
        move || shutdown.count_down()
    }

    pub fn shutdown(&self) {
        self.shutdown_handle()();
    }

    pub async fn await_shutdown(self) {
        self.shutdown.wait().await;

        match self.server_fut.await {
            Ok(_) => log::info!("Atrium server stopped."),
            Err(err) => log::error!(err:?; "Atrium server failed."),
        }
    }
}

pub async fn start_server(config: &Config, ctx: Arc<Context>) -> Result<ServerState, io::Error> {
    let shutdown = Arc::new(Latch::new(1));
    let wg = WaitGroup::new();

    log::info!("listening on {}", config.listen_addr);

    let acceptor = TcpListener::bind(&config.listen_addr)
        .into_acceptor()
        .await?;
    let listen_addr = acceptor.local_addr()[0]
        .as_socket_addr()
        .cloned()
        .ok_or_else(|| io::Error::other("failed to get local address of server"))?;
    let server_advertise_addr =
        resolve_advertise_addr(listen_addr, config.advertise_addr.as_deref())?;

    let server_fut = {
        let shutdown_clone = shutdown.clone();
        let wg_clone = wg.clone();

        let route = Route::new()
            .at("/", poem::get(get).put(put).delete(delete))
            .data(ctx)
            .with(LoggerMiddleware);
        let signal = async move {
            log::info!("Server has started on [{listen_addr}]");
            drop(wg_clone);

            shutdown_clone.wait().await;
            log::info!("Server is closing");
        };

        tokio::spawn(async move {
            poem::Server::new_with_acceptor(acceptor)
                .run_with_graceful_shutdown(route, signal, Some(Duration::from_secs(30)))
                .await
        })
    };

    wg.await;
    Ok(ServerState {
        server_advertise_addr,
        server_fut,
        shutdown,
    })
}

fn resolve_advertise_addr(
    listen_addr: SocketAddr,
    advertise_addr: Option<&str>,
) -> Result<SocketAddr, io::Error> {
    match advertise_addr {
        None => {
            if listen_addr.ip().is_unspecified() {
                let ip = local_ip_address::local_ip().map_err(io::Error::other)?;
                let port = listen_addr.port();
                Ok(SocketAddr::new(ip, port))
            } else {
                Ok(listen_addr)
            }
        }
        Some(advertise_addr) => {
            let advertise_addr = advertise_addr
                .parse::<SocketAddr>()
                .map_err(io::Error::other)?;
            assert!(
                advertise_addr.ip().is_global(),
                "ip = {}",
                advertise_addr.ip()
            );
            Ok(advertise_addr)
        }
    }
}

#[derive(Deserialize)]
struct GetParams {
    key: String,
}

#[handler]
pub async fn get(Data(ctx): Data<&Arc<Context>>, Query(params): Query<GetParams>) -> Response {
    let Ok(key) = urlencoding::decode(&params.key) else {
        return Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .body("Bad request");
    };
    let value = ctx.engine.get(key.as_bytes()).await;

    match value {
        Some(value) => Response::builder()
            .status(StatusCode::OK)
            .typed_header(ContentType::octet_stream())
            .body(value),
        None => Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body("Not found"),
    }
}

#[derive(Deserialize)]
struct PutParams {
    key: String,
}

#[handler]
pub async fn put(
    Data(ctx): Data<&Arc<Context>>,
    Query(params): Query<PutParams>,
    body: Body,
) -> Response {
    let Ok(key) = urlencoding::decode(&params.key) else {
        return Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .body("Bad request");
    };
    let put_result = body
        .into_bytes()
        .await
        .map(|bytes| ctx.engine.put(key.as_bytes(), &bytes));

    match put_result {
        Ok(_) => Response::builder()
            .status(StatusCode::CREATED)
            .body("Created"),
        Err(_) => Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .body("Bad request"),
    }
}

#[derive(Deserialize)]
struct DeleteParams {
    key: String,
}

#[handler]
pub async fn delete(
    Data(ctx): Data<&Arc<Context>>,
    Query(params): Query<DeleteParams>,
) -> Response {
    let Ok(key) = urlencoding::decode(&params.key) else {
        return Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .body("Bad request");
    };
    ctx.engine.delete(key.as_bytes());
    Response::builder().status(StatusCode::NO_CONTENT).body("")
}
