use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;

use crate::error::Result;
use crate::kv::{KvBackend, KvKey, KvNamespace, KvStore};
use crate::metadata::{TagDelta, TrackId, TrackMetadata};

#[async_trait]
pub trait TagReader: Send + Sync {
    async fn read_from_file(&self, track: &TrackId, path: &Path) -> Result<TrackMetadata>;
}

#[async_trait]
pub trait TagPersistence: Send + Sync {
    async fn load_delta(&self, track: &TrackId) -> Result<Option<TagDelta>>;
    async fn save_delta(&self, track: &TrackId, delta: &TagDelta) -> Result<()>;
    async fn delete_delta(&self, track: &TrackId) -> Result<()>;
}

pub struct KvTagPersistence<B: KvBackend> {
    store: KvStore<B>,
}

impl<B: KvBackend> KvTagPersistence<B> {
    pub fn new(store: KvStore<B>) -> Self {
        Self { store }
    }

    fn key(track: &TrackId) -> KvKey {
        KvKey::new(KvNamespace::Track, format!("{}:tag", track))
    }
}

#[async_trait]
impl<B: KvBackend> TagPersistence for KvTagPersistence<B> {
    async fn load_delta(&self, track: &TrackId) -> Result<Option<TagDelta>> {
        self.store.load(&Self::key(track)).await
    }

    async fn save_delta(&self, track: &TrackId, delta: &TagDelta) -> Result<()> {
        self.store.store(&Self::key(track), delta).await
    }

    async fn delete_delta(&self, track: &TrackId) -> Result<()> {
        self.store.remove(&Self::key(track)).await
    }
}

#[async_trait]
pub trait TagOverlayService: Send + Sync {
    async fn read(&self, track: &TrackId, source: &Path) -> Result<TrackMetadata>;
    async fn apply(
        &self,
        track: &TrackId,
        source: &Path,
        delta: &TagDelta,
    ) -> Result<TrackMetadata>;
    async fn remove(&self, track: &TrackId) -> Result<()>;
}

pub struct TagOverlay<R: TagReader, P: TagPersistence> {
    reader: Arc<R>,
    persistence: Arc<P>,
}

impl<R: TagReader, P: TagPersistence> TagOverlay<R, P> {
    pub fn new(reader: Arc<R>, persistence: Arc<P>) -> Self {
        Self {
            reader,
            persistence,
        }
    }

    fn apply_delta(meta: &mut TrackMetadata, delta: &TagDelta) {
        for key in &delta.remove {
            meta.tags.0.remove(key);
        }
        for (key, value) in &delta.set {
            meta.tags.insert(key.clone(), value.clone());
        }
    }
}

#[async_trait]
impl<R, P> TagOverlayService for TagOverlay<R, P>
where
    R: TagReader,
    P: TagPersistence,
{
    async fn read(&self, track: &TrackId, source: &Path) -> Result<TrackMetadata> {
        let mut metadata = self.reader.read_from_file(track, source).await?;
        if let Some(delta) = self.persistence.load_delta(track).await? {
            Self::apply_delta(&mut metadata, &delta);
        }
        Ok(metadata)
    }

    async fn apply(
        &self,
        track: &TrackId,
        source: &Path,
        delta: &TagDelta,
    ) -> Result<TrackMetadata> {
        let mut merged = self.reader.read_from_file(track, source).await?;
        Self::apply_delta(&mut merged, delta);
        self.persistence.save_delta(track, delta).await?;
        Ok(merged)
    }

    async fn remove(&self, track: &TrackId) -> Result<()> {
        self.persistence.delete_delta(track).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mockall::{mock, predicate::always};
    use std::collections::HashMap;
    use tempfile::tempdir;

    use crate::kv::SledBackend;
    use crate::metadata::{AlbumId, TagMap, TagValue};

    mock! {
        pub Reader {}

        #[async_trait]
        impl TagReader for Reader {
            async fn read_from_file(&self, track: &TrackId, path: &Path) -> Result<TrackMetadata>;
        }
    }

    fn sample_track() -> TrackMetadata {
        TrackMetadata {
            id: TrackId {
                album: AlbumId("album".into()),
                disc: 1,
                index: 1,
            },
            title: "Track".into(),
            artist: "Artist".into(),
            album_artist: None,
            duration_ms: 1000,
            tags: TagMap::default(),
            artwork: None,
        }
    }

    #[tokio::test]
    async fn overlay_merges_delta() {
        let mut reader = MockReader::new();
        reader
            .expect_read_from_file()
            .with(always(), always())
            .returning(|_, _| Ok(sample_track()));

        let dir = tempdir().unwrap();
        let backend = SledBackend::open(dir.path()).unwrap();
        let store = KvStore::new(Arc::new(backend));
        let persistence = Arc::new(KvTagPersistence::new(store));
        let overlay = TagOverlay::new(Arc::new(reader), persistence.clone());

        let track_id = TrackId {
            album: AlbumId("album".into()),
            disc: 1,
            index: 1,
        };

        let delta = TagDelta {
            set: HashMap::from([(String::from("RATING"), TagValue::Number(5))]),
            remove: vec![String::from("COMMENT")],
        };

        let merged = overlay
            .apply(&track_id, Path::new("track.flac"), &delta)
            .await
            .unwrap();
        assert_eq!(merged.tags.get("RATING"), Some(&TagValue::Number(5)));

        let reloaded = overlay
            .read(&track_id, Path::new("dummy.flac"))
            .await
            .unwrap();
        assert_eq!(reloaded.tags.get("RATING"), Some(&TagValue::Number(5)));
    }
}
