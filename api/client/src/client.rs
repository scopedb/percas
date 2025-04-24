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

use reqwest::IntoUrl;
use reqwest::StatusCode;
use reqwest::Url;

use crate::Error;

#[derive(Debug, Clone)]
pub struct ClientFactory {
    client: reqwest::Client,
}

impl ClientFactory {
    pub fn new() -> Result<Self, Error> {
        let client = reqwest::ClientBuilder::new()
            .no_proxy()
            .build()
            .map_err(Error::Http)?;
        Ok(Self { client })
    }

    pub fn make_client(&self, endpoint: String) -> Result<Client, Error> {
        Client::new(endpoint, self.client.clone())
    }
}

pub struct Client {
    client: reqwest::Client,
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

    fn new(base_url: impl IntoUrl, client: reqwest::Client) -> Result<Self, Error> {
        let base_url = base_url.into_url().map_err(Error::Http)?;
        Ok(Client { client, base_url })
    }
}

async fn do_get(client: &Client, key: &str) -> Result<Option<Vec<u8>>, Error> {
    let url = client
        .base_url
        .join(key)
        .map_err(|e| Error::Other(e.to_string()))?;

    let resp = client.client.get(url).send().await.map_err(Error::Http)?;

    match resp.status() {
        StatusCode::NOT_FOUND => Ok(None),
        StatusCode::OK => {
            let body = resp.bytes().await.map_err(Error::Http)?;
            Ok(Some(body.to_vec()))
        }
        StatusCode::TOO_MANY_REQUESTS => Err(Error::TooManyRequests),
        _ => Err(Error::Other(resp.status().to_string())),
    }
}

async fn do_put(client: &Client, key: &str, value: &[u8]) -> Result<(), Error> {
    let url = client
        .base_url
        .join(key)
        .map_err(|e| Error::Other(e.to_string()))?;

    let resp = client
        .client
        .put(url)
        .body(value.to_vec())
        .send()
        .await
        .map_err(Error::Http)?;

    match resp.status() {
        StatusCode::OK | StatusCode::CREATED => Ok(()),
        StatusCode::TOO_MANY_REQUESTS => Err(Error::TooManyRequests),
        status => Err(Error::Other(status.to_string())),
    }
}

async fn do_delete(client: &Client, key: &str) -> Result<(), Error> {
    let url = client
        .base_url
        .join(key)
        .map_err(|e| Error::Other(e.to_string()))?;

    let resp = client
        .client
        .delete(url)
        .send()
        .await
        .map_err(Error::Http)?;

    match resp.status() {
        StatusCode::OK | StatusCode::NO_CONTENT => Ok(()),
        StatusCode::TOO_MANY_REQUESTS => Err(Error::TooManyRequests),
        status => Err(Error::Other(status.to_string())),
    }
}
