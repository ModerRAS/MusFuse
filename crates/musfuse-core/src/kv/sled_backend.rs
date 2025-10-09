use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::task::spawn_blocking;

use crate::error::{MusFuseError, Result};

use super::{KvBackend, KvKey, KvNamespace, NamespaceCache};

pub struct SledBackend {
    db: Arc<sled::Db>,
    cache: Arc<NamespaceCache>,
}

impl SledBackend {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let db = sled::open(path)
            .map_err(|err| MusFuseError::Kv(format!("unable to open sled db: {err}")))?;
        Ok(Self {
            db: Arc::new(db),
            cache: Arc::new(NamespaceCache::new()),
        })
    }

    pub fn from_db(db: sled::Db) -> Self {
        Self {
            db: Arc::new(db),
            cache: Arc::new(NamespaceCache::new()),
        }
    }

    async fn tree(&self, namespace: KvNamespace) -> Result<sled::Tree> {
        let db = self.db.clone();
        let cache = self.cache.clone();
        spawn_blocking(move || cache.get_or_insert(&db, namespace))
            .await
            .map_err(|err| MusFuseError::Kv(format!("task join error: {err}")))?
    }
}

#[async_trait]
impl KvBackend for SledBackend {
    async fn get(&self, key: &KvKey) -> Result<Option<Vec<u8>>> {
        let tree = self.tree(key.namespace).await?;
        let key_bytes = key.key.clone();
        spawn_blocking(move || {
            tree.get(key_bytes.as_bytes())
                .map(|opt| opt.map(|ivec| ivec.as_ref().to_vec()))
                .map_err(|err| MusFuseError::Kv(err.to_string()))
        })
        .await
        .map_err(|err| MusFuseError::Kv(format!("task join error: {err}")))?
    }

    async fn put(&self, key: &KvKey, value: Vec<u8>) -> Result<()> {
        let tree = self.tree(key.namespace).await?;
        let key_bytes = key.key.clone();
        spawn_blocking(move || {
            tree.insert(key_bytes.as_bytes(), value)
                .map(|_| ())
                .map_err(|err| MusFuseError::Kv(err.to_string()))
        })
        .await
        .map_err(|err| MusFuseError::Kv(format!("task join error: {err}")))?
    }

    async fn delete(&self, key: &KvKey) -> Result<()> {
        let tree = self.tree(key.namespace).await?;
        let key_bytes = key.key.clone();
        spawn_blocking(move || {
            tree.remove(key_bytes.as_bytes())
                .map(|_| ())
                .map_err(|err| MusFuseError::Kv(err.to_string()))
        })
        .await
        .map_err(|err| MusFuseError::Kv(format!("task join error: {err}")))?
    }

    async fn scan_prefix(
        &self,
        namespace: KvNamespace,
        prefix: &str,
    ) -> Result<Vec<(String, Vec<u8>)>> {
        let tree = self.tree(namespace).await?;
        let prefix = prefix.to_owned();
        spawn_blocking(move || {
            let mut results = Vec::new();
            for item in tree.scan_prefix(prefix.as_bytes()) {
                let (key, value) = item.map_err(|err| MusFuseError::Kv(err.to_string()))?;
                results.push((String::from_utf8_lossy(&key).into_owned(), value.to_vec()));
            }
            Ok(results)
        })
        .await
        .map_err(|err| MusFuseError::Kv(format!("task join error: {err}")))?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kv::{KvKey, KvNamespace, KvStore};
    use crate::metadata::{AlbumId, TagMap, TrackId, TrackMetadata};

    fn test_store(path: &Path) -> Result<KvStore<SledBackend>> {
        let backend = SledBackend::open(path)?;
        Ok(KvStore::new(Arc::new(backend)))
    }

    #[tokio::test]
    async fn put_and_get_roundtrip() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = test_store(dir.path()).expect("create store");
        let key = KvKey::new(KvNamespace::Track, "album1-01-01");

        let metadata = TrackMetadata {
            id: TrackId {
                album: AlbumId("album1".into()),
                disc: 1,
                index: 1,
            },
            title: "Intro".into(),
            artist: "Artist".into(),
            album_artist: None,
            duration_ms: 120_000,
            tags: TagMap::default(),
            artwork: None,
        };

        store.store(&key, &metadata).await.expect("store");
        let fetched = store.load::<TrackMetadata>(&key).await.expect("load");
        assert_eq!(fetched, Some(metadata));
    }

    #[tokio::test]
    async fn scan_prefix_returns_matches() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = test_store(dir.path()).expect("create store");

        for idx in 1..=3 {
            let key = KvKey::new(KvNamespace::Track, format!("album1-01-{idx:02}"));
            store.store(&key, &idx).await.expect("store");
        }

        let backend = store.backend().clone();
        let results = backend
            .scan_prefix(KvNamespace::Track, "album1-01")
            .await
            .expect("scan");
        assert_eq!(results.len(), 3);
    }
}
