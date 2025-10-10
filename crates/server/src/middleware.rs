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
use percas_core::num_cpus;
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
        let key = req.path_params::<String>()?;
        match self.proxy.route(&key) {
            RouteDest::Local => self
                .endpoint
                .call(req)
                .await
                .map(IntoResponse::into_response),
            RouteDest::RemoteAddr(addr) => {
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

                let location = format!("http://{addr}{}", req.uri().path());
                Ok(temporary_redirect(&location))
            }
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
            return Ok(too_many_requests());
        };
        let _run_permit = self.run_permit.acquire(1).await;

        self.endpoint
            .call(req)
            .await
            .map(IntoResponse::into_response)
    }
}
