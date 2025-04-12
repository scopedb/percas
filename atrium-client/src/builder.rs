use crate::client::Client;

pub struct ClientBuilder {
    endpoint: String,
}

impl ClientBuilder {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }

    pub fn build(self) -> Client {
        let builder = reqwest::ClientBuilder::new().no_proxy();
        // FIXME(tisonkun): fallible over unwrap
        Client::new(self.endpoint, builder).unwrap()
    }
}
