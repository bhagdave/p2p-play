use crate::errors::FetchError;


pub trait ContentFetcher: Send + Sync {
    async fn fetch(&self, cid: &str) -> Result<Vec<u8>, FetchError>;
    async fn resolve_ipns(&self, name: &str) -> Result<String, FetchError>;
}

pub struct GatewayFetcher {
    gateway_url: String,
    client: reqwest::Client,
}

impl GatewayFetcher {
    pub fn new() -> Self {
        Self {
            gateway_url: "https://dweb.link".to_string(),
            client: reqwest::Client::new(),
        }
    }
}

impl ContentFetcher for GatewayFetcher {
    async fn fetch(&self, cid: &str) -> Result<Vec<u8>, FetchError> {
        let url = format!("{}/ipfs/{}", self.gateway_url, cid);
        let response = self.client.get(&url).send().await?;
        Ok(response.bytes().await?.to_vec())
    }
    
    async fn resolve_ipns(&self, name: &str) -> Result<String, FetchError> {
        // Gateways support IPNS resolution via redirect
        let url = format!("{}/ipns/{}", self.gateway_url, name);
        let response = self.client.head(&url).send().await?;
        // Extract final CID from redirect chain or response headers
        // ...
        // PLaceholder result to keep compiler happy
        Ok("resolved-cid-placeholder".to_string())
    }
}
