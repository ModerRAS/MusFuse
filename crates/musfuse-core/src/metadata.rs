use std::collections::{BTreeMap, HashMap};
use std::fmt;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Ord, PartialOrd)]
pub struct AlbumId(pub String);

impl fmt::Display for AlbumId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Ord, PartialOrd)]
pub struct TrackId {
    pub album: AlbumId,
    pub disc: u8,
    pub index: u32,
}

impl fmt::Display for TrackId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}-{:02}-{:02}", self.album, self.disc, self.index)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct TagMap(pub BTreeMap<String, TagValue>);

impl TagMap {
    pub fn get(&self, key: &str) -> Option<&TagValue> {
        self.0.get(key)
    }

    pub fn insert(&mut self, key: impl Into<String>, value: TagValue) {
        self.0.insert(key.into(), value);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TagValue {
    Text(String),
    Number(i64),
    Float(f64),
    Bool(bool),
    List(Vec<TagValue>),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TrackMetadata {
    pub id: TrackId,
    pub title: String,
    pub artist: String,
    pub album_artist: Option<String>,
    pub duration_ms: u64,
    pub tags: TagMap,
    pub artwork: Option<ArtworkRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AlbumMetadata {
    pub id: AlbumId,
    pub title: String,
    pub album_artist: Option<String>,
    pub year: Option<u32>,
    pub tracks: Vec<TrackId>,
    pub artwork: Option<ArtworkRef>,
    pub tags: TagMap,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ArtworkRef {
    pub hash: String,
    pub mime: String,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TagDelta {
    pub set: HashMap<String, TagValue>,
    pub remove: Vec<String>,
}

impl TagDelta {
    pub fn is_empty(&self) -> bool {
        self.set.is_empty() && self.remove.is_empty()
    }
}
