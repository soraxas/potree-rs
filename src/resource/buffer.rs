use super::{ResourceClient, ResourceError};
use async_trait::async_trait;
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

#[derive(Clone, Debug, Default)]
pub struct BufferClient {
    store: Arc<HashMap<String, Arc<Vec<u8>>>>,
}

impl BufferClient {
    pub fn new() -> Self {
        Self {
            store: Arc::new(HashMap::new()),
        }
    }

    pub fn from_entries<I, K>(entries: I) -> Self
    where
        I: IntoIterator<Item = (K, Vec<u8>)>,
        K: Into<String>,
    {
        let mut store = HashMap::new();
        for (key, data) in entries {
            store.insert(normalize_key(&key.into()), Arc::new(data));
        }
        Self {
            store: Arc::new(store),
        }
    }

    pub fn insert(&mut self, url: impl Into<String>, data: Vec<u8>) {
        let key = normalize_key(&url.into());
        let map = Arc::make_mut(&mut self.store);
        map.insert(key, Arc::new(data));
    }

    pub fn contains(&self, url: &str) -> bool {
        let key = normalize_key(url);
        self.store.contains_key(&key)
    }

    fn resolve(&self, url: &str) -> Option<Arc<Vec<u8>>> {
        let key = normalize_key(url);
        self.store.get(&key).cloned()
    }
}

fn normalize_key(url: &str) -> String {
    if let Some(stripped) = url.strip_prefix("buffer://") {
        stripped.trim_start_matches('/').to_string()
    } else {
        url.to_string()
    }
}

#[async_trait]
    impl ResourceClient for BufferClient {
    async fn get(
        &self,
        url: &str,
        _headers: Option<BTreeMap<String, String>>,
    ) -> Result<Vec<u8>, ResourceError> {
        let data = self
            .resolve(url)
            .ok_or_else(|| ResourceError::Other(format!("Memory resource missing: {url}")))?;
        Ok(data.to_vec())
    }

    async fn get_range(
        &self,
        url: &str,
        offset: u64,
        length: usize,
        _headers: Option<BTreeMap<String, String>>,
    ) -> Result<Vec<u8>, ResourceError> {
        let data = self
            .resolve(url)
            .ok_or_else(|| ResourceError::Other(format!("Memory resource missing: {url}")))?;

        let start = offset as usize;
        if start > data.len() {
            return Err(ResourceError::Other(format!(
                "Offset {offset} out of bounds for {url}"
            )));
        }

        let end = (offset as usize)
            .saturating_add(length)
            .min(data.len());

        Ok(data[start..end].to_vec())
    }
}
