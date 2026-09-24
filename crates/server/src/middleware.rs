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
use std::time::Duration;

use mea::semaphore::Semaphore;
use percas_core::RequestLimits;
use percas_gossip::Proxy;
use percas_gossip::RouteDest;
use percas_metrics::GlobalMetrics;
use percas_metrics::OperationMetrics;
use poem::Endpoint;
use poem::IntoResponse;
use poem::Middleware;
use poem::Request;
use poem::Response;
use poem::http::StatusCode;

use crate::server::temporary_redirect;
use crate::server::too_many_requests;

pub struct LoggerMiddleware;

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

pub struct LoggerEndpoint<E>(E);

impl<E> Endpoint for LoggerEndpoint<E>
where
    E: Endpoint,
    E::Output: IntoResponse,
{
    type Output = Response;

    async fn call(&self, req: Request) -> Result<Self::Output, poem::Error> {
        let method = req.method().clone();
        let uri = req.uri().clone();
        log::debug!("{method} {uri} called");
        let resp = self.0.call(req).await.inspect_err(|err| {
            if err.status() != StatusCode::NOT_FOUND {
                log::error!("{method} {uri} {}: {err}", err.status());
            }
        })?;
        let resp = resp.into_response();
        log::debug!("{method} {uri} returns {}", resp.status());
        Ok(resp)
    }
}

pub struct ClusterProxyMiddleware {
    proxy: Proxy,
}

impl ClusterProxyMiddleware {
    pub fn new(proxy: Proxy) -> Self {
        Self { proxy }
    }
}

impl<E> Middleware<E> for ClusterProxyMiddleware
where
    E: Endpoint,
    E::Output: IntoResponse,
{
    type Output = ClusterProxyEndpoint<E>;

    fn transform(&self, endpoint: E) -> Self::Output {
        ClusterProxyEndpoint {
            proxy: self.proxy.clone(),
            endpoint,
        }
    }
}

pub struct ClusterProxyEndpoint<E> {
    proxy: Proxy,
    endpoint: E,
}

impl<E> Endpoint for ClusterProxyEndpoint<E>
where
    E: Endpoint,
    E::Output: IntoResponse,
{
    type Output = Response;

    async fn call(&self, req: Request) -> Result<Self::Output, poem::Error> {
        let key = cache_key(&req)?;
        match self.proxy.route(&key) {
            RouteDest::Local => self
                .endpoint
                .call(req)
                .await
                .map(IntoResponse::into_response),
            RouteDest::RemoteAddr(mut url) => {
                let operation = match req.method().as_str() {
                    "GET" => OperationMetrics::OPERATION_GET,
                    "PUT" => OperationMetrics::OPERATION_PUT,
                    "DELETE" => OperationMetrics::OPERATION_DELETE,
                    _ => OperationMetrics::OPERATION_UNKNOWN,
                };

                GlobalMetrics::get().operation.count.add(
                    1,
                    &OperationMetrics::operation_labels(
                        operation,
                        OperationMetrics::STATUS_REDIRECT,
                    ),
                );

                url.set_path(req.uri().path());
                url.set_query(req.uri().query());
                Ok(temporary_redirect(url.as_ref()))
            }
        }
    }
}

/// Extract the exact same key for both routing and storage. The legacy path
/// API remains available; the v1 API carries opaque UTF-8 keys in a query field.
pub(crate) fn cache_key(req: &Request) -> Result<String, poem::Error> {
    if req.uri().path() == "/v1/cache" {
        let mut keys = url::form_urlencoded::parse(req.uri().query().unwrap_or("").as_bytes())
            .filter(|(name, _)| name == "key");
        let key = keys
            .next()
            .ok_or_else(|| poem::Error::from_status(StatusCode::BAD_REQUEST))?
            .1
            .into_owned();
        if keys.next().is_some() {
            return Err(poem::Error::from_status(StatusCode::BAD_REQUEST));
        }
        Ok(key)
    } else {
        Ok(req.path_params::<String>()?)
    }
}

#[derive(Clone)]
pub struct RateLimitMiddleware {
    limits: RequestLimits,
    requests: Arc<Semaphore>,
    upload_bytes: Arc<Semaphore>,
}

impl RateLimitMiddleware {
    pub fn new(limits: RequestLimits) -> Self {
        Self {
            requests: Arc::new(Semaphore::new(limits.max_concurrent_requests)),
            upload_bytes: Arc::new(Semaphore::new(limits.max_inflight_body_bytes)),
            limits,
        }
    }
}

impl<E: Endpoint> Middleware<E> for RateLimitMiddleware {
    type Output = RateLimitEndpoint<E>;
    fn transform(&self, endpoint: E) -> Self::Output {
        RateLimitEndpoint {
            budget: self.clone(),
            endpoint,
        }
    }
}

pub struct RateLimitEndpoint<E> {
    budget: RateLimitMiddleware,
    endpoint: E,
}

impl<E: Endpoint> Endpoint for RateLimitEndpoint<E> {
    type Output = Response;

    async fn call(&self, mut req: Request) -> Result<Response, poem::Error> {
        let Some(_request) = self.budget.requests.try_acquire(1) else {
            return Ok(too_many_requests());
        };
        let mut upload = None;
        if req.method() == poem::http::Method::PUT {
            let limits = &self.budget.limits;
            let declared = req
                .headers()
                .get(poem::http::header::CONTENT_LENGTH)
                .map(|value| {
                    value
                        .to_str()
                        .ok()
                        .and_then(|value| value.parse::<usize>().ok())
                });
            if declared == Some(None) {
                return Ok(Response::builder().status(StatusCode::BAD_REQUEST).finish());
            }
            let bound = declared.flatten().unwrap_or(limits.max_body_bytes);
            if bound > limits.max_body_bytes {
                return Ok(Response::builder()
                    .status(StatusCode::PAYLOAD_TOO_LARGE)
                    .finish());
            }
            upload = self.budget.upload_bytes.try_acquire(bound.max(1));
            if upload.is_none() {
                return Ok(too_many_requests());
            }
            let body = tokio::time::timeout(
                Duration::from_millis(limits.body_timeout_ms),
                req.take_body().into_bytes_limit(bound),
            )
            .await;
            let bytes = match body {
                Err(_) => {
                    return Ok(Response::builder()
                        .status(StatusCode::REQUEST_TIMEOUT)
                        .finish());
                }
                Ok(Err(poem::error::ReadBodyError::PayloadTooLarge)) => {
                    return Ok(Response::builder()
                        .status(StatusCode::PAYLOAD_TOO_LARGE)
                        .finish());
                }
                Ok(Err(_)) => {
                    return Ok(Response::builder().status(StatusCode::BAD_REQUEST).finish());
                }
                Ok(Ok(bytes)) => bytes,
            };
            req.set_body(bytes);
        }
        let result = self
            .endpoint
            .call(req)
            .await
            .map(IntoResponse::into_response);
        drop(upload);
        result
    }
}

#[cfg(test)]
mod tests {
    use poem::EndpointExt;
    use poem::http::Method;

    use super::*;

    fn limits() -> RequestLimits {
        RequestLimits {
            max_body_bytes: 8,
            max_inflight_body_bytes: 8,
            max_concurrent_requests: 2,
            body_timeout_ms: 50,
        }
    }

    fn put(body: impl Into<poem::Body>) -> Request {
        Request::builder().method(Method::PUT).body(body)
    }

    #[tokio::test]
    async fn oversized_bodies_are_rejected_with_or_without_content_length() {
        let endpoint = poem::endpoint::make_sync(|_| "ok").with(RateLimitMiddleware::new(limits()));
        for declared in [false, true] {
            let mut request = put("123456789");
            if declared {
                request
                    .headers_mut()
                    .insert("content-length", "9".parse().unwrap());
            }
            assert_eq!(
                endpoint.get_response(request).await.status(),
                StatusCode::PAYLOAD_TOO_LARGE
            );
        }
        assert_eq!(
            endpoint.get_response(put("12345678")).await.status(),
            StatusCode::OK
        );
    }

    #[tokio::test]
    async fn stalled_upload_reserves_budget_and_releases_it_after_timeout() {
        let endpoint = poem::endpoint::make_sync(|_| "ok").with(RateLimitMiddleware::new(limits()));
        let (_writer, reader) = tokio::io::duplex(64);
        let first = endpoint.get_response(put(poem::Body::from_async_read(reader)));
        tokio::pin!(first);
        // Poll the first request until it is waiting for bytes with its reservation held.
        assert!(
            tokio::time::timeout(Duration::from_millis(5), &mut first)
                .await
                .is_err()
        );
        assert_eq!(
            endpoint.get_response(put("x")).await.status(),
            StatusCode::TOO_MANY_REQUESTS
        );
        assert_eq!(first.await.status(), StatusCode::REQUEST_TIMEOUT);
        assert_eq!(
            endpoint.get_response(put("x")).await.status(),
            StatusCode::OK
        );
    }

    #[tokio::test]
    async fn cancelling_a_request_releases_concurrency_capacity() {
        let mut limits = limits();
        limits.max_concurrent_requests = 1;
        let endpoint = poem::endpoint::make(|request| async move {
            if request.uri().path() == "/slow" {
                std::future::pending::<()>().await;
            }
            "ok"
        })
        .with(RateLimitMiddleware::new(limits));
        let mut slow = Box::pin(
            endpoint.get_response(Request::builder().uri("/slow".parse().unwrap()).finish()),
        );
        assert!(
            tokio::time::timeout(Duration::from_millis(5), &mut slow)
                .await
                .is_err()
        );
        assert_eq!(
            endpoint.get_response(Request::default()).await.status(),
            StatusCode::TOO_MANY_REQUESTS
        );
        drop(slow);
        assert_eq!(
            endpoint.get_response(Request::default()).await.status(),
            StatusCode::OK
        );
    }

    #[tokio::test]
    async fn query_key_is_preserved_across_redirects() {
        let dir = std::path::PathBuf::from("unused");
        let local = percas_gossip::NodeInfo::new(
            uuid::Uuid::now_v7(),
            "cluster".into(),
            "http://local.test/".parse().unwrap(),
            "http://local.test:7655/".parse().unwrap(),
        );
        let state = Arc::new(percas_gossip::GossipState::new(local, vec![], dir));
        let remote = percas_gossip::NodeInfo::new(
            uuid::Uuid::now_v7(),
            "cluster".into(),
            "http://remote.test/".parse().unwrap(),
            "http://remote.test:7655/".parse().unwrap(),
        );
        state.handle_message(percas_gossip::GossipMessage::Ping(remote));
        let endpoint = poem::endpoint::make_sync(|_| "local")
            .with(ClusterProxyMiddleware::new(Proxy::new(state)));
        let uri = "/v1/cache?key=a%2F..%2Fb%3Fx%23y";
        let request = Request::builder().uri(uri.parse().unwrap()).finish();
        assert_eq!(cache_key(&request).unwrap(), "a/../b?x#y");
        let response = endpoint.get_response(request).await;
        assert_eq!(response.status(), StatusCode::TEMPORARY_REDIRECT);
        assert_eq!(
            response.headers()["location"],
            format!("http://remote.test{uri}")
        );
    }
}
