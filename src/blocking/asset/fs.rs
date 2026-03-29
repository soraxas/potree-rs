use std::{
    io::{Read, Seek, SeekFrom},
    path::PathBuf,
};

use super::BlockingPotreeAsset;
use crate::metadata::Metadata;
use bytes::Bytes;
use thiserror::Error;

pub struct BlockingPotreeFsAsset {
    base_path: PathBuf,
}

impl BlockingPotreeFsAsset {
    pub fn from_path(path: impl Into<PathBuf>) -> Self {
        Self {
            base_path: path.into(),
        }
    }
}

impl BlockingPotreeAsset for BlockingPotreeFsAsset {
    type Error = PotreeFsAssetError;

    fn read_metadata(&self) -> Result<Metadata, Self::Error> {
        let metadata_path = self.base_path.join("metadata.json");
        let buffer = std::fs::read(metadata_path)?;

        Ok(serde_json::from_slice(&buffer)?)
    }

    fn read_hierarchy(&self, offset: u64, length: usize) -> Result<Bytes, Self::Error> {
        let hierarchy_path = self.base_path.join("hierarchy.bin");

        let mut file = std::fs::File::open(hierarchy_path)?;
        file.seek(SeekFrom::Start(offset))?;
        let mut bytes = vec![0; length];
        file.read_exact(&mut bytes)?;

        Ok(bytes.into())
    }

    fn read_octree(&self, offset: u64, length: usize) -> Result<Bytes, Self::Error> {
        let octree_path = self.base_path.join("octree.bin");

        let mut file = std::fs::File::open(octree_path)?;
        file.seek(SeekFrom::Start(offset))?;
        let mut bytes = vec![0; length];
        file.read_exact(&mut bytes)?;

        Ok(bytes.into())
    }
}

#[derive(Debug, Error)]
pub enum PotreeFsAssetError {
    #[error("IO Error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}
