use error_stack::Result;
use error_stack::ResultExt;
use error_stack::bail;
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

pub struct Client {
    pub(crate) endpoint: String,
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
}

async fn do_get(client: &Client, key: &str) -> Result<Option<Vec<u8>>, Error> {
    let make_error = || Error::Other("failed to get".to_string());
    let mut url = Url::parse(&client.endpoint).change_context_lazy(make_error)?;

    let encoded_key = urlencoding::encode(key);

    url = url.join(&encoded_key).change_context_lazy(make_error)?;

    let resp = reqwest::get(url).await.change_context_lazy(make_error)?;

    match resp.status() {
        StatusCode::OK => {
            let body = resp.bytes().await.change_context_lazy(make_error)?;
            Ok(Some(body.to_vec()))
        }

        StatusCode::NOT_FOUND => Ok(None),

        _ => bail!(Error::Other(resp.status().to_string())),
    }
}

async fn do_put(client: &Client, key: &str, value: &[u8]) -> Result<(), Error> {
    let make_error = || Error::Other("failed to get".to_string());
    let mut url = Url::parse(&client.endpoint).change_context_lazy(make_error)?;

    let encoded_key = urlencoding::encode(key);

    url = url.join(&encoded_key).change_context_lazy(make_error)?;

    let resp = reqwest::Client::new()
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
    let make_error = || Error::Other("failed to get".to_string());
    let mut url = Url::parse(&client.endpoint).change_context_lazy(make_error)?;

    let encoded_key = urlencoding::encode(key);

    url = url.join(&encoded_key).change_context_lazy(make_error)?;

    let resp = reqwest::Client::new()
        .delete(url)
        .send()
        .await
        .change_context_lazy(make_error)?;

    match resp.status() {
        StatusCode::OK | StatusCode::NO_CONTENT => Ok(()),

        _ => bail!(Error::Other(resp.status().to_string())),
    }
}
