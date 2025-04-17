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

use error_stack::Result;
use error_stack::ResultExt;
use error_stack::bail;
use reqwest::IntoUrl;
use reqwest::StatusCode;
use reqwest::Url;

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub enum Error {
    #[from(std::io::Error)]
    IO(std::io::Error),
    #[from(reqwest::Error)]
    Http(reqwest::Error),

    Other(String),
}

pub struct ClientBuilder {
    endpoint: String,
}

impl ClientBuilder {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }

    pub fn build(self) -> Result<Client, Error> {
        let builder = reqwest::ClientBuilder::new().no_proxy();
        Client::new(self.endpoint, builder)
    }
}

pub struct Client {
    client: reqwest::Client,
    base_url_data: Url,
    // perhaps for debugging
    #[allow(dead_code)]
    base_url: Url,
}

impl Client {
    pub async fn get(&self, key: &str) -> Result<Option<Vec<u8>>, Error> {
        do_get(self, key).await
    }

    pub async fn put(&self, key: &str, value: &[u8]) -> Result<(), Error> {
        do_put(self, key, value).await
    }

    pub async fn delete(&self, key: &str) -> Result<(), Error> {
        do_delete(self, key).await
    }

    fn new(base_url: impl IntoUrl, builder: reqwest::ClientBuilder) -> Result<Self, Error> {
        let client = builder.build().map_err(Error::Http)?;

        let make_error = || Error::Other("failed to prepare base URLs".to_string());
        let base_url = base_url.into_url().change_context_lazy(make_error)?;
        let base_url_data = base_url.join("/data/").change_context_lazy(make_error)?;

        Ok(Client {
            client,
            base_url,
            base_url_data,
        })
    }
}

async fn do_get(client: &Client, key: &str) -> Result<Option<Vec<u8>>, Error> {
    let make_error = || Error::Other("failed to get".to_string());

    let url = client
        .base_url_data
        .join(key)
        .change_context_lazy(make_error)?;

    let resp = client
        .client
        .get(url)
        .send()
        .await
        .change_context_lazy(make_error)?;

    match resp.status() {
        StatusCode::NOT_FOUND => Ok(None),
        StatusCode::OK => {
            let body = resp.bytes().await.change_context_lazy(make_error)?;
            Ok(Some(body.to_vec()))
        }
        _ => bail!(Error::Other(resp.status().to_string())),
    }
}

async fn do_put(client: &Client, key: &str, value: &[u8]) -> Result<(), Error> {
    let make_error = || Error::Other("failed to put".to_string());

    let url = client
        .base_url_data
        .join(key)
        .change_context_lazy(make_error)?;

    let resp = client
        .client
        .put(url)
        .body(value.to_vec())
        .send()
        .await
        .change_context_lazy(make_error)?;

    match resp.status() {
        StatusCode::OK | StatusCode::CREATED => Ok(()),
        _ => bail!(Error::Other(resp.status().to_string())),
    }
}

async fn do_delete(client: &Client, key: &str) -> Result<(), Error> {
    let make_error = || Error::Other("failed to delete".to_string());

    let url = client
        .base_url_data
        .join(key)
        .change_context_lazy(make_error)?;

    let resp = client
        .client
        .delete(url)
        .send()
        .await
        .change_context_lazy(make_error)?;

    match resp.status() {
        StatusCode::OK | StatusCode::NO_CONTENT => Ok(()),
        _ => bail!(Error::Other(resp.status().to_string())),
    }
}
