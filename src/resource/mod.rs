#[cfg(feature = "ehttp")]
mod ehttp;

#[cfg(feature = "fs")]
mod file;

pub mod buffer;

#[cfg(all(feature = "reqwest", not(any(feature = "wasm", feature = "ehttp"))))]
mod reqwest;

#[cfg(feature = "ehttp_local")]
mod ehttp_local;

use async_trait::async_trait;
use serde::de::DeserializeOwned;
use std::collections::BTreeMap;
use std::marker::PhantomData;
use std::sync::Arc;
use url::Url;

pub use buffer::BufferClient;

#[derive(Clone, Debug)]
pub struct ResourceLoader {
    #[cfg(feature = "fs")]
    file: file::FileClient,

    #[cfg(all(feature = "reqwest", not(any(feature = "wasm", feature = "ehttp"))))]
    http: reqwest::ReqwestClient,

    #[cfg(all(feature = "ehttp", not(feature = "ehttp_local")))]
    http: ehttp::EhttpClient,

    #[cfg(feature = "ehttp_local")]
    http: ehttp_local::EhttpClientLocal,

    buffer: Option<buffer::BufferClient>,
}

impl Default for ResourceLoader {
    fn default() -> Self {
        Self::new()
    }
}

impl ResourceLoader {
    pub fn new() -> Self {
        Self {
            #[cfg(feature = "fs")]
            file: file::FileClient,
            #[cfg(all(feature = "reqwest", not(any(feature = "wasm", feature = "ehttp"))))]
            http: reqwest::ReqwestClient::new(),

            #[cfg(all(feature = "ehttp", not(feature = "ehttp_local")))]
            http: ehttp::EhttpClient,

            #[cfg(feature = "ehttp_local")]
            http: ehttp_local::EhttpClientLocal::new(),
            buffer: None,
        }
    }

    /// Add a buffer client to the resource loader.
    pub fn with_buffer(mut self, buffer: buffer::BufferClient) -> Self {
        self.buffer = Some(buffer);
        self
    }

    fn get_delegate(&'_ self, url: &str) -> Result<ErasedResourceClient<'_>, ResourceError> {
        if url.contains("://") {
            let parsed_url = Url::parse(url)?;
            let scheme = parsed_url.scheme();

            match scheme {
                #[cfg(any(feature = "ehttp", feature = "reqwest"))]
                "http" | "https" => Ok(ErasedResourceClient::Http(&self.http)),
                #[cfg(feature = "fs")]
                "file" => Ok(ErasedResourceClient::File(&self.file)),
                "buffer" => {
                    let buffer = self.buffer.as_ref().ok_or_else(|| {
                        ResourceError::Unsupported("Memory backend not configured".to_string())
                    })?;
                    Ok(ErasedResourceClient::Memory(buffer))
                }
                _ => Err(ResourceError::Unsupported(format!(
                    "Unknown scheme {scheme}"
                ))),
            }
        } else {
            if let Some(buffer) = &self.buffer {
                if buffer.contains(url) {
                    return Ok(ErasedResourceClient::Memory(buffer));
                }
            }
            #[cfg(feature = "fs")]
            {
                Ok(ErasedResourceClient::File(&self.file))
            }

            #[cfg(all(
                not(feature = "fs"),
                any(feature = "reqwest", feature = "ehttp", feature = "ehttp_local")
            ))]
            {
                Ok(ErasedResourceClient::Http(&self.http))
            }

            #[cfg(all(
                not(feature = "fs"),
                not(feature = "reqwest"),
                not(feature = "ehttp"),
                not(feature = "ehttp_local")
            ))]
            Err(ResourceError::Unsupported(
                "Scheme not supported".to_string(),
            ))
        }
    }
    pub async fn get(
        &self,
        url: &str,
        headers: Option<BTreeMap<String, String>>,
    ) -> Result<Vec<u8>, ResourceError> {
        self.get_delegate(url)?.get(url, headers).await
    }

    pub async fn get_range(
        &self,
        url: &str,
        offset: u64,
        length: usize,
        headers: Option<BTreeMap<String, String>>,
    ) -> Result<Vec<u8>, ResourceError> {
        self.get_delegate(url)?
            .get_range(url, offset, length, headers)
            .await
    }

    pub async fn get_json<T: DeserializeOwned + Send>(
        &self,
        url: &str,
        headers: Option<BTreeMap<String, String>>,
    ) -> Result<T, ResourceError> {
        self.get_delegate(url)?.get_json(url, headers).await
    }
}

#[async_trait]
trait ResourceClient: Send + Sync + Clone {
    async fn get(
        &self,
        url: &str,
        headers: Option<BTreeMap<String, String>>,
    ) -> Result<Vec<u8>, ResourceError>;

    async fn get_range(
        &self,
        url: &str,
        offset: u64,
        length: usize,
        headers: Option<BTreeMap<String, String>>,
    ) -> Result<Vec<u8>, ResourceError> {
        // Compute the Range header
        let end = offset
            .checked_add(length as u64)
            .map(|v| v - 1)
            .ok_or_else(|| ResourceError::Other("Range overflow".into()))?;
        let range_value = format!("bytes={}-{}", offset, end);

        // Merge headers
        let mut all_headers = headers.unwrap_or_default();
        all_headers.insert("Range".to_string(), range_value);

        // Call get() with Range header
        self.get(url, Some(all_headers)).await
    }

    async fn get_json<T: DeserializeOwned + Send>(
        &self,
        url: &str,
        headers: Option<BTreeMap<String, String>>,
    ) -> Result<T, ResourceError> {
        let bytes = self.get(url, headers).await?;
        Ok(serde_json::from_slice(&bytes)?)
    }
}

#[derive(Clone, Debug)]
pub enum ErasedResourceClient<'a> {
    #[cfg(feature = "fs")]
    File(&'a file::FileClient),
    #[cfg(all(feature = "reqwest", not(any(feature = "wasm", feature = "ehttp"))))]
    Http(&'a reqwest::ReqwestClient),
    #[cfg(all(feature = "ehttp", not(feature = "ehttp_local")))]
    Http(&'a ehttp::EhttpClient),
    #[cfg(feature = "ehttp_local")]
    Http(&'a ehttp_local::EhttpClientLocal),
    Memory(&'a buffer::BufferClient),
    Phantom(&'a PhantomData<String>),
}

#[async_trait]
impl ResourceClient for ErasedResourceClient<'_> {
    #[allow(unused_variables)]
    async fn get(
        &self,
        url: &str,
        headers: Option<BTreeMap<String, String>>,
    ) -> Result<Vec<u8>, ResourceError> {
        match self {
            #[cfg(feature = "fs")]
            ErasedResourceClient::File(delegate) => delegate.get(url, headers).await,
            #[cfg(any(feature = "reqwest", feature = "ehttp", feature = "ehttp_local"))]
            ErasedResourceClient::Http(delegate) => delegate.get(url, headers).await,
            ErasedResourceClient::Memory(delegate) => delegate.get(url, headers).await,
            _ => Err(ResourceError::Unsupported(
                "Scheme not supported".to_string(),
            )),
        }
    }

    #[allow(unused_variables)]
    async fn get_range(
        &self,
        url: &str,
        offset: u64,
        length: usize,
        headers: Option<BTreeMap<String, String>>,
    ) -> Result<Vec<u8>, ResourceError> {
        match self {
            #[cfg(feature = "fs")]
            ErasedResourceClient::File(delegate) => {
                delegate.get_range(url, offset, length, headers).await
            }
            #[cfg(any(feature = "reqwest", feature = "ehttp", feature = "ehttp_local"))]
            ErasedResourceClient::Http(delegate) => {
                delegate.get_range(url, offset, length, headers).await
            }
            ErasedResourceClient::Memory(delegate) => {
                delegate.get_range(url, offset, length, headers).await
            }
            _ => Err(ResourceError::Unsupported(
                "Scheme not supported".to_string(),
            )),
        }
    }

    #[allow(unused_variables)]
    async fn get_json<T: DeserializeOwned + Send>(
        &self,
        url: &str,
        headers: Option<BTreeMap<String, String>>,
    ) -> Result<T, ResourceError> {
        match self {
            #[cfg(feature = "fs")]
            ErasedResourceClient::File(delegate) => delegate.get_json(url, headers).await,
            #[cfg(any(feature = "reqwest", feature = "ehttp", feature = "ehttp_local"))]
            ErasedResourceClient::Http(delegate) => delegate.get_json(url, headers).await,
            ErasedResourceClient::Memory(delegate) => delegate.get_json(url, headers).await,
            _ => Err(ResourceError::Unsupported(
                "Scheme not supported".to_string(),
            )),
        }
    }
}

#[async_trait]
impl<C: ResourceClient> ResourceClient for Arc<C> {
    async fn get(
        &self,
        url: &str,
        headers: Option<BTreeMap<String, String>>,
    ) -> Result<Vec<u8>, ResourceError> {
        (**self).get(url, headers).await
    }

    async fn get_range(
        &self,
        url: &str,
        offset: u64,
        length: usize,
        headers: Option<BTreeMap<String, String>>,
    ) -> Result<Vec<u8>, ResourceError> {
        (**self).get_range(url, offset, length, headers).await
    }

    async fn get_json<T: DeserializeOwned + Send>(
        &self,
        url: &str,
        headers: Option<BTreeMap<String, String>>,
    ) -> Result<T, ResourceError> {
        (**self).get_json(url, headers).await
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ResourceError {
    #[error("Network error: {0}")]
    Network(String),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Unexpected HTTP status code: {0}")]
    Status(u16),

    #[error("File error: {0}")]
    File(#[from] std::io::Error),

    #[error("Unable to parse url: {0}")]
    Url(#[from] url::ParseError),

    #[error("{0}")]
    Other(String),

    #[error("Unsupported scheme: {0}")]
    Unsupported(String),
}
