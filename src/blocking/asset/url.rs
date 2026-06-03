use bytes::Bytes;
use thiserror::Error;
use url::Url;

#[cfg(feature = "blocking_fs")]
use crate::blocking::asset::fs::BlockingPotreeFsAsset;
#[cfg(feature = "blocking_reqwest")]
use crate::blocking::asset::http::BlockingPotreeHttpAsset;
use crate::{blocking::asset::BlockingPotreeAsset, metadata::Metadata};

pub struct BlockingPotreeUrlAsset {
    inner: InnerAsset,
}

pub enum InnerAsset {
    #[cfg(feature = "blocking_reqwest")]
    Http(BlockingPotreeHttpAsset),
    #[cfg(feature = "blocking_fs")]
    Fs(BlockingPotreeFsAsset),
}

impl BlockingPotreeUrlAsset {
    pub fn from_url(url: &str) -> Result<Self, BlockingPotreeUrlAssetError> {
        if url.contains("://") {
            let parsed_url = Url::parse(url)?;
            let scheme = parsed_url.scheme();

            match scheme {
                #[cfg(feature = "blocking_reqwest")]
                "http" | "https" => Ok(Self {
                    inner: InnerAsset::Http(BlockingPotreeHttpAsset::from_url(url)),
                }),
                #[cfg(feature = "blocking_fs")]
                "file" => Ok(Self {
                    inner: InnerAsset::Fs(BlockingPotreeFsAsset::from_path(parsed_url.path())),
                }),
                _ => Err(BlockingPotreeUrlAssetError::Unsupported(format!(
                    "Unknown scheme {}",
                    scheme
                ))),
            }
        } else {
            #[cfg(feature = "blocking_fs")]
            {
                Ok(Self {
                    inner: InnerAsset::Fs(BlockingPotreeFsAsset::from_path(url)),
                })
            }

            #[cfg(all(not(feature = "blocking_fs"), feature = "blocking_reqwest"))]
            {
                Ok(Self {
                    inner: InnerAsset::Http(BlockingPotreeHttpAsset::from_url(url)),
                })
            }

            #[cfg(all(not(feature = "blocking_fs"), not(feature = "blocking_reqwest"),))]
            Err(BlockingPotreeUrlAssetError::Unsupported(
                "Relative urls are not supported in this configuration.".to_string(),
            ))
        }
    }
}

impl BlockingPotreeAsset for BlockingPotreeUrlAsset {
    type Error = BlockingPotreeUrlAssetError;

    fn read_metadata(&self) -> Result<Metadata, Self::Error> {
        match &self.inner {
            #[cfg(feature = "blocking_reqwest")]
            InnerAsset::Http(potree_http_asset) => Ok(potree_http_asset
                .read_metadata()
                .map_err(|err| Self::Error::Read(err.to_string()))?),
            #[cfg(feature = "blocking_fs")]
            InnerAsset::Fs(potree_fs_asset) => Ok(potree_fs_asset
                .read_metadata()
                .map_err(|err| Self::Error::Read(err.to_string()))?),
        }
    }

    fn read_hierarchy(&self, offset: u64, length: usize) -> Result<Bytes, Self::Error> {
        match &self.inner {
            #[cfg(feature = "blocking_reqwest")]
            InnerAsset::Http(potree_http_asset) => Ok(potree_http_asset
                .read_hierarchy(offset, length)
                .map_err(|err| Self::Error::Read(err.to_string()))?),
            #[cfg(feature = "blocking_fs")]
            InnerAsset::Fs(potree_fs_asset) => {
                Ok(potree_fs_asset
                    .read_hierarchy(offset, length)
                    .map_err(|err| Self::Error::Read(err.to_string()))?)
            }
        }
    }

    fn read_octree(&self, offset: u64, length: usize) -> Result<Bytes, Self::Error> {
        match &self.inner {
            #[cfg(feature = "blocking_reqwest")]
            InnerAsset::Http(potree_http_asset) => Ok(potree_http_asset
                .read_octree(offset, length)
                .map_err(|err| Self::Error::Read(err.to_string()))?),
            #[cfg(feature = "blocking_fs")]
            InnerAsset::Fs(potree_fs_asset) => Ok(potree_fs_asset
                .read_octree(offset, length)
                .map_err(|err| Self::Error::Read(err.to_string()))?),
        }
    }
}

#[derive(Debug, Error)]
pub enum BlockingPotreeUrlAssetError {
    #[error("Read error: {0}")]
    Read(String),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Unable to parse url: {0}")]
    Url(#[from] url::ParseError),

    #[error("{0}")]
    Other(String),

    #[error("Unsupported scheme: {0}")]
    Unsupported(String),
}
