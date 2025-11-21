use crate::errors::FetchError;
use reqwest::Client;
use std::time::Duration;


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
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("Failed to build reqwest client");

        Self {
            gateway_url: "https://dweb.link".to_string(),
            client: reqwest::Client::new(),
        }
    }

    pub fn with_gateway(gateway_url: &str) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("Failed to build reqwest client");

        Self {
            gateway_url: gateway_url.to_string(),
            client,
        }
    }
}

impl ContentFetcher for GatewayFetcher {
    async fn fetch(&self, cid: &str) -> Result<Vec<u8>, FetchError> {
        let url = format!("{}/ipfs/{}", self.gateway_url, cid);
        let response = self.client.get(&url).send().await?;

        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(FetchError::NotFound(cid.to_string()));
        }

        let bytes = response.bytes().await?.to_vec();

        Ok(bytes)
    }
    
    async fn resolve_ipns(&self, name: &str) -> Result<String, FetchError> {
        // Gateways support IPNS resolution via redirect
        let url = format!("{}/ipns/{}", self.gateway_url, name);
        let response = self.client.head(&url).send().await?;
        
        let final_url = response.url().path();
        let cid = final_url.strip_prefix("/ipfs/").ok_or_else(|| FetchError::InvalidCid(name.to_string()))?;
        
        
        Ok(cid.to_string())
    }
}
