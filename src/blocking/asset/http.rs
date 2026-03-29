use bytes::Bytes;
use thiserror::Error;

use super::BlockingPotreeAsset;
use crate::metadata::Metadata;

pub struct BlockingPotreeHttpAsset {
    base_url: String,
    #[cfg(feature = "blocking_reqwest")]
    client: reqwest::blocking::Client,
}

impl BlockingPotreeHttpAsset {
    pub fn from_url(url: &str) -> Self {
        let base_url = if url.ends_with('/') {
            // remove leading /
            url.trim_end_matches('/').to_string()
        } else {
            match url.rfind('/') {
                // remove last part of the url if it ends with (metadata.json, hierarchy.bin or octree.bin)
                Some(index) => {
                    let (path, end) = url.split_at(index);
                    match &end[1..] {
                        "metadata.json" | "hierarchy.bin" | "octree.bin" => path.to_string(),
                        _ => url.to_string(),
                    }
                }
                None => url.to_string(),
            }
        };

        Self {
            base_url,
            client: reqwest::blocking::Client::new(),
        }
    }
}

impl BlockingPotreeAsset for BlockingPotreeHttpAsset {
    type Error = PotreeHttpAssetError;

    fn read_metadata(&self) -> Result<Metadata, Self::Error> {
        let metadata_url = format!("{}/metadata.json", self.base_url);

        Ok(self
            .client
            .get(metadata_url)
            .send()?
            .error_for_status()?
            .json()?)
    }

    fn read_hierarchy(&self, offset: u64, length: usize) -> Result<Bytes, Self::Error> {
        let end = offset
            .checked_add(length as u64)
            .map(|v| v - 1)
            .ok_or_else(|| Self::Error::Other("Range overflow".into()))?;
        let range_value = format!("bytes={}-{}", offset, end);

        let hierarchy_url = format!("{}/hierarchy.bin", self.base_url);

        Ok(self
            .client
            .get(hierarchy_url)
            .header("range", range_value)
            .send()?
            .error_for_status()?
            .bytes()?)
    }

    fn read_octree(&self, offset: u64, length: usize) -> Result<Bytes, Self::Error> {
        let end = offset
            .checked_add(length as u64)
            .map(|v| v - 1)
            .ok_or_else(|| Self::Error::Other("Range overflow".into()))?;
        let range_value = format!("bytes={}-{}", offset, end);

        let octree_url = format!("{}/octree.bin", self.base_url);

        Ok(self
            .client
            .get(octree_url)
            .header("range", range_value)
            .send()?
            .error_for_status()?
            .bytes()?)
    }
}

#[derive(Debug, Error)]
pub enum PotreeHttpAssetError {
    #[error("Reqwest error: {0}")]
    Reqwest(#[from] reqwest::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("{0}")]
    Other(String),

    #[error("Unsupported scheme: {0}")]
    Unsupported(String),
}
