use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use serde::{Serialize, de::DeserializeOwned};

use crate::error::Result;

mod sled_backend;
pub use sled_backend::SledBackend;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct KvKey {
    pub namespace: KvNamespace,
    pub key: String,
}

impl KvKey {
    pub fn new(namespace: KvNamespace, key: impl Into<String>) -> Self {
        Self {
            namespace,
            key: key.into(),
        }
    }

    pub fn as_str(&self) -> String {
        format!("{}:{}", self.namespace, self.key)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KvNamespace {
    Track,
    Album,
    Artwork,
    Cue,
    FileStat,
    Cache,
    Policy,
}

impl std::fmt::Display for KvNamespace {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use KvNamespace::*;
        let value = match self {
            Track => "track",
            Album => "album",
            Artwork => "artwork",
            Cue => "cue",
            FileStat => "file",
            Cache => "cache",
            Policy => "policy",
        };
        f.write_str(value)
    }
}

#[async_trait]
pub trait KvBackend: Send + Sync + 'static {
    async fn get(&self, key: &KvKey) -> Result<Option<Vec<u8>>>;
    async fn put(&self, key: &KvKey, value: Vec<u8>) -> Result<()>;
    async fn delete(&self, key: &KvKey) -> Result<()>;
    async fn scan_prefix(
        &self,
        namespace: KvNamespace,
        prefix: &str,
    ) -> Result<Vec<(String, Vec<u8>)>>;
}

pub trait KvCodec: Serialize + DeserializeOwned + Send + Sync + 'static {}

impl<T> KvCodec for T where T: Serialize + DeserializeOwned + Send + Sync + 'static {}

pub struct KvStore<B: KvBackend> {
    backend: Arc<B>,
}

impl<B: KvBackend> KvStore<B> {
    pub fn new(backend: Arc<B>) -> Self {
        Self { backend }
    }

    pub fn backend(&self) -> &Arc<B> {
        &self.backend
    }

    pub async fn load<T>(&self, key: &KvKey) -> Result<Option<T>>
    where
        T: KvCodec,
    {
        match self.backend.get(key).await? {
            Some(bytes) => {
                let value = serde_json::from_slice(&bytes)
                    .map_err(|err| crate::error::MusFuseError::Kv(err.to_string()))?;
                Ok(Some(value))
            }
            None => Ok(None),
        }
    }

    pub async fn store<T>(&self, key: &KvKey, value: &T) -> Result<()>
    where
        T: KvCodec,
    {
        let bytes = serde_json::to_vec(value)
            .map_err(|err| crate::error::MusFuseError::Kv(err.to_string()))?;
        self.backend.put(key, bytes).await
    }

    pub async fn remove(&self, key: &KvKey) -> Result<()> {
        self.backend.delete(key).await
    }
}

struct NamespaceCache {
    map: parking_lot::Mutex<HashMap<KvNamespace, sled::Tree>>,
}

impl NamespaceCache {
    fn new() -> Self {
        Self {
            map: parking_lot::Mutex::new(HashMap::new()),
        }
    }

    fn get_or_insert(
        &self,
        db: &sled::Db,
        namespace: KvNamespace,
    ) -> crate::error::Result<sled::Tree> {
        if let Some(tree) = self.map.lock().get(&namespace).cloned() {
            return Ok(tree);
        }

        let tree_name = namespace.to_string();
        let tree = db
            .open_tree(tree_name.as_bytes())
            .map_err(|err| crate::error::MusFuseError::Kv(err.to_string()))?;
        self.map.lock().insert(namespace, tree.clone());
        Ok(tree)
    }
}
