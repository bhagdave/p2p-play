use crate::errors::FetchError;
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
            client,
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
        let cid = final_url
            .strip_prefix("/ipfs/")
            .ok_or_else(|| FetchError::InvalidCid(name.to_string()))?;

        Ok(cid.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wasm_executor::validate_wasm;

    /// Create a minimal valid WASM module for testing
    fn create_minimal_wasm() -> Vec<u8> {
        wat::parse_str(
            r#"
            (module
                (func $main)
                (export "_start" (func $main))
            )
            "#,
        )
        .expect("Failed to parse WAT")
    }

    #[test]
    fn test_validate_wasm_bytes() {
        let wasm = create_minimal_wasm();
        validate_wasm(&wasm).unwrap();
    }

    #[tokio::test]
    async fn test_gateway_fetcher_creation() {
        let fetcher = GatewayFetcher::new();
        assert_eq!(fetcher.gateway_url, "https://dweb.link");
    }

    #[tokio::test]
    async fn test_gateway_fetcher_with_custom_gateway() {
        let fetcher = GatewayFetcher::with_gateway("https://ipfs.io");
        assert_eq!(fetcher.gateway_url, "https://ipfs.io");
    }

    /// This test requires network access and a valid CID on IPFS.
    /// Run with: cargo test test_fetch_wasm_from_ipfs -- --ignored
    #[tokio::test]
    #[ignore]
    async fn test_fetch_wasm_from_ipfs() {
        let fetcher = GatewayFetcher::new();
        let cid = "QmSwfNM1vNQu3orSr2SrSyAZijYmHm57W4PU2XULPrNcjd";

        let bytes = fetcher.fetch(cid).await.unwrap();
        validate_wasm(&bytes).unwrap();

        println!("Fetched {} bytes", bytes.len());
    }
}
