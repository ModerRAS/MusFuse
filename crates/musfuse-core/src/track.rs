use std::path::PathBuf;

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::cue::CueSheet;
use crate::metadata::{AlbumId, TagMap, TrackId, TrackMetadata};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SourceTrack {
    pub id: TrackId,
    pub path: PathBuf,
    pub cue_path: Option<PathBuf>,
    pub offset_frames: u64,
    pub length_frames: u64,
    pub sample_rate: u32,
    pub channels: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TrackCollection {
    pub album: AlbumId,
    pub tracks: Vec<SourceTrack>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TrackIndexEntry {
    pub id: TrackId,
    pub metadata: TrackMetadata,
    pub source: SourceTrack,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TrackIndex {
    pub entries: Vec<TrackIndexEntry>,
}

impl TrackIndex {
    pub fn by_id(&self, id: &TrackId) -> Option<&TrackIndexEntry> {
        self.entries.iter().find(|entry| &entry.id == id)
    }
}

pub struct TrackMapper;

impl TrackMapper {
    pub fn from_cue(sheet: &CueSheet, album_id: &AlbumId, cue_path: Option<&Path>) -> TrackIndex {
        let mut entries = Vec::new();
        for file in &sheet.files {
            let mut iter = file.tracks.iter().peekable();
            while let Some(track) = iter.next() {
                let next_start = iter
                    .peek()
                    .map(|next| next.index_01_frames)
                    .unwrap_or(track.index_01_frames);
                let length_frames = if next_start > track.index_01_frames {
                    next_start - track.index_01_frames
                } else {
                    0
                };

                let track_id = TrackId {
                    album: album_id.clone(),
                    disc: 1,
                    index: track.number,
                };

                let metadata = TrackMetadata {
                    id: track_id.clone(),
                    title: track
                        .title
                        .clone()
                        .unwrap_or_else(|| format!("Track {:02}", track.number)),
                    artist: track
                        .performer
                        .clone()
                        .or_else(|| sheet.album_performer.clone())
                        .unwrap_or_else(|| "Unknown Artist".into()),
                    album_artist: sheet.album_performer.clone(),
                    duration_ms: crate::cue::frames_to_ms(length_frames),
                    tags: TagMap::default(),
                    artwork: None,
                };

                let source = SourceTrack {
                    id: track_id.clone(),
                    path: file.path.clone(),
                    cue_path: cue_path.map(|p| p.to_path_buf()),
                    offset_frames: track.index_01_frames,
                    length_frames,
                    sample_rate: 44_100,
                    channels: 2,
                };

                entries.push(TrackIndexEntry {
                    id: track_id,
                    metadata,
                    source,
                });
            }
        }
        TrackIndex { entries }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cue::{CueFile, CueSheet, CueTrack};

    #[test]
    fn map_cue_to_track_index() {
        let sheet = CueSheet {
            album_title: Some("Album".into()),
            album_performer: Some("Artist".into()),
            files: vec![CueFile {
                path: Path::new("/music/disc.flac").to_path_buf(),
                tracks: vec![
                    CueTrack {
                        number: 1,
                        title: Some("Intro".into()),
                        performer: Some("Artist".into()),
                        index_01_frames: 0,
                    },
                    CueTrack {
                        number: 2,
                        title: Some("Song".into()),
                        performer: None,
                        index_01_frames: 75 * 120,
                    },
                ],
            }],
        };

        let album = AlbumId("album".into());
        let index = TrackMapper::from_cue(&sheet, &album, Some(Path::new("/music/disc.cue")));
        assert_eq!(index.entries.len(), 2);
        let second = &index.entries[1];
        assert_eq!(second.metadata.title, "Song");
        assert_eq!(second.metadata.artist, "Artist");
        assert_eq!(second.source.path, Path::new("/music/disc.flac"));
        assert_eq!(
            second.source.cue_path.as_deref(),
            Some(Path::new("/music/disc.cue"))
        );
    }
}
