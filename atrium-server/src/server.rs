use std::sync::Arc;

use atrium_core::Config;
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

pub async fn start_server(config: &Config, ctx: Arc<Context>) -> Result<(), std::io::Error> {
    let route = Route::new()
        .at("/:key", poem::get(get).put(put).delete(delete))
        .data(ctx)
        .with(LoggerMiddleware);

    log::info!("listening on {}", config.listen_addr);

    poem::Server::new(TcpListener::bind(&config.listen_addr))
        .run(route)
        .await
}

#[derive(Deserialize)]
struct GetParams {
    key: String,
}

#[handler]
pub async fn get(Data(ctx): Data<&Arc<Context>>, Query(params): Query<GetParams>) -> Response {
    let key = params.key.as_bytes().to_vec();
    let value = ctx.engine.get(&key).await;

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
    let key = params.key.as_bytes().to_vec();
    let put_result = body
        .into_bytes()
        .await
        .map(|bytes| ctx.engine.put(&key, &bytes));

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
    let key = params.key.as_bytes();
    ctx.engine.delete(key);
    Response::builder().status(StatusCode::NO_CONTENT).body("")
}
