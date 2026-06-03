#[cfg(feature = "blocking_reqwest")]
pub mod http;

#[cfg(feature = "blocking_fs")]
pub mod fs;

#[cfg(any(feature = "blocking_fs", feature = "blocking_reqwest"))]
pub mod url;

use bytes::Bytes;

use crate::metadata::Metadata;

pub trait BlockingPotreeAsset: Sync + Send {
    type Error: std::error::Error + Sync + Send + 'static;

    fn read_metadata(&self) -> Result<Metadata, Self::Error>;

    fn read_hierarchy(&self, offset: u64, length: usize) -> Result<Bytes, Self::Error>;

    fn read_octree(&self, offset: u64, length: usize) -> Result<Bytes, Self::Error>;
}
