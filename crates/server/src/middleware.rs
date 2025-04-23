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

use mea::semaphore::Semaphore;
use percas_client::ClientBuilder;
use percas_cluster::Proxy;
use percas_cluster::RouteDest;
use percas_core::num_cpus;
use poem::Endpoint;
use poem::IntoResponse;
use poem::Middleware;
use poem::Request;
use poem::Response;
use poem::http::Method;
use poem::http::StatusCode;

use crate::server::delete_success;
use crate::server::get_not_found;
use crate::server::get_success;
use crate::server::put_success;

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
    proxy: Option<Proxy>,
}

impl ClusterProxyMiddleware {
    pub fn new(proxy: Option<Proxy>) -> Self {
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
    proxy: Option<Proxy>,
    endpoint: E,
}

impl<E> Endpoint for ClusterProxyEndpoint<E>
where
    E: Endpoint,
    E::Output: IntoResponse,
{
    type Output = Response;

    async fn call(&self, mut req: Request) -> Result<Self::Output, poem::Error> {
        let key = req.path_params::<String>()?;

        if let Some(proxy) = &self.proxy {
            match proxy.route(&key) {
                RouteDest::Local => self
                    .endpoint
                    .call(req)
                    .await
                    .map(IntoResponse::into_response),
                RouteDest::RemoteAddr(addr) => {
                    let client = ClientBuilder::new(format!("http://{addr}"))
                        .build()
                        .unwrap();
                    match *req.method() {
                        Method::GET => {
                            let resp = client.get(&key).await;
                            match resp {
                                Ok(resp) => {
                                    if let Some(value) = resp {
                                        Ok(get_success(value))
                                    } else {
                                        Ok(get_not_found())
                                    }
                                }
                                Err(err) => {
                                    log::error!("failed to get from remote: {err}");
                                    self.endpoint
                                        .call(req)
                                        .await
                                        .map(IntoResponse::into_response)
                                }
                            }
                        }
                        Method::PUT => {
                            let body = req.take_body().into_bytes().await?;
                            let resp = client.put(&key, &body).await;
                            match resp {
                                Ok(()) => Ok(put_success()),
                                Err(err) => {
                                    log::error!("failed to put from remote: {err}");
                                    req.set_body(body);
                                    self.endpoint
                                        .call(req)
                                        .await
                                        .map(IntoResponse::into_response)
                                }
                            }
                        }
                        Method::DELETE => {
                            let resp = client.delete(&key).await;
                            match resp {
                                Ok(()) => Ok(delete_success()),
                                Err(err) => {
                                    log::error!("failed to delete from remote: {err}");
                                    self.endpoint
                                        .call(req)
                                        .await
                                        .map(IntoResponse::into_response)
                                }
                            }
                        }

                        _ => self
                            .endpoint
                            .call(req)
                            .await
                            .map(IntoResponse::into_response),
                    }
                }
            }
        } else {
            self.endpoint
                .call(req)
                .await
                .map(IntoResponse::into_response)
        }
    }
}

pub struct RateLimitMiddleware {
    wait_permit: Arc<Semaphore>,
    run_permit: Arc<Semaphore>,
}

impl RateLimitMiddleware {
    pub fn new() -> Self {
        let run_limit = num_cpus().get() * 100;
        let wait_limit = run_limit * 5;

        Self {
            wait_permit: Arc::new(Semaphore::new(wait_limit)),
            run_permit: Arc::new(Semaphore::new(run_limit)),
        }
    }
}

impl<E> Middleware<E> for RateLimitMiddleware
where
    E: Endpoint,
    E::Output: IntoResponse,
{
    type Output = RateLimitEndpoint<E>;

    fn transform(&self, endpoint: E) -> Self::Output {
        RateLimitEndpoint {
            wait_permit: self.wait_permit.clone(),
            run_permit: self.run_permit.clone(),
            endpoint,
        }
    }
}

pub struct RateLimitEndpoint<E> {
    wait_permit: Arc<Semaphore>,
    run_permit: Arc<Semaphore>,
    endpoint: E,
}

impl<E> Endpoint for RateLimitEndpoint<E>
where
    E: Endpoint,
    E::Output: IntoResponse,
{
    type Output = Response;

    async fn call(&self, req: Request) -> Result<Self::Output, poem::Error> {
        let Some(_wait_permit) = self.wait_permit.try_acquire(1) else {
            return Ok(Response::builder()
                .status(StatusCode::TOO_MANY_REQUESTS)
                .body(StatusCode::TOO_MANY_REQUESTS.to_string()));
        };
        let _run_permit = self.run_permit.acquire(1).await;

        self.endpoint
            .call(req)
            .await
            .map(IntoResponse::into_response)
    }
}
