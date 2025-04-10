use crate::client::Client;

pub struct ClientBuilder {
    endpoint: String,
}

impl ClientBuilder {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }

    pub fn build(self) -> Client {
        Client {
            endpoint: self.endpoint,
        }
    }
}
