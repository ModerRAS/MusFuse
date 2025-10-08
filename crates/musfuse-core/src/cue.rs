use std::path::{Path, PathBuf};

use crate::error::Result;

#[derive(Debug, Clone, PartialEq)]
pub struct CueSheet {
    pub album_title: Option<String>,
    pub album_performer: Option<String>,
    pub files: Vec<CueFile>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CueFile {
    pub path: PathBuf,
    pub tracks: Vec<CueTrack>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CueTrack {
    pub number: u32,
    pub title: Option<String>,
    pub performer: Option<String>,
    pub index_01_frames: u64,
}

impl CueTrack {
    pub fn start_ms(&self) -> u64 {
        frames_to_ms(self.index_01_frames)
    }
}

pub struct CueParser;

impl CueParser {
    pub fn parse_str(&self, content: &str, base_dir: &Path) -> Result<CueSheet> {
        parse_cue(content, base_dir)
    }

    pub async fn parse_file(&self, path: &Path) -> Result<CueSheet> {
        let base_dir = path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));
        let content = tokio::fs::read_to_string(path).await?;
        parse_cue(&content, &base_dir)
    }
}

fn parse_cue(content: &str, base_dir: &Path) -> Result<CueSheet> {
    let mut sheet = CueSheet {
        album_title: None,
        album_performer: None,
        files: Vec::new(),
    };

    let mut current_file: Option<CueFile> = None;
    let mut current_track: Option<CueTrack> = None;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("REM") {
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("FILE") {
            if let Some(file) = current_file.take() {
                sheet.files.push(file);
            }
            let parts = rest.trim().splitn(2, ' ').collect::<Vec<_>>();
            let name = parts
                .first()
                .map(|s| s.trim_matches('"'))
                .filter(|s| !s.is_empty())
                .map(|s| base_dir.join(s))
                .ok_or_else(|| crate::error::MusFuseError::Mount("invalid FILE entry".into()))?;
            current_file = Some(CueFile {
                path: name,
                tracks: Vec::new(),
            });
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("TRACK") {
            if let Some(track) = current_track.take() {
                if let Some(file) = &mut current_file {
                    file.tracks.push(track);
                }
            }
            let mut parts = rest.trim().split_whitespace();
            let number = parts
                .next()
                .ok_or_else(|| crate::error::MusFuseError::Mount("missing track number".into()))?
                .parse::<u32>()
                .map_err(|_| crate::error::MusFuseError::Mount("invalid track number".into()))?;
            current_track = Some(CueTrack {
                number,
                title: None,
                performer: None,
                index_01_frames: 0,
            });
            continue;
        }

        if trimmed.starts_with("TITLE") {
            let value = extract_quoted(trimmed).unwrap_or_default();
            if let Some(track) = &mut current_track {
                track.title = Some(value.to_string());
            } else {
                sheet.album_title = Some(value.to_string());
            }
            continue;
        }

        if trimmed.starts_with("PERFORMER") {
            let value = extract_quoted(trimmed).unwrap_or_default();
            if let Some(track) = &mut current_track {
                track.performer = Some(value.to_string());
            } else {
                sheet.album_performer = Some(value.to_string());
            }
            continue;
        }

        if trimmed.starts_with("INDEX 01") {
            let timestamp = trimmed
                .split_whitespace()
                .last()
                .ok_or_else(|| crate::error::MusFuseError::Mount("missing index timestamp".into()))?;
            if let Some(track) = &mut current_track {
                track.index_01_frames = timestamp_to_frames(timestamp)?;
            }
            continue;
        }
    }

    if let Some(track) = current_track.take() {
        if let Some(file) = &mut current_file {
            file.tracks.push(track);
        }
    }
    if let Some(file) = current_file.take() {
        sheet.files.push(file);
    }

    Ok(sheet)
}

fn extract_quoted(line: &str) -> Option<&str> {
    let start = line.find('"')? + 1;
    let end = line[start..].find('"')? + start;
    Some(&line[start..end])
}

fn timestamp_to_frames(value: &str) -> Result<u64> {
    let parts: Vec<_> = value.split(':').collect();
    if parts.len() != 3 {
        return Err(crate::error::MusFuseError::Mount("invalid timestamp".into()));
    }
    let minutes: u64 = parts[0].parse().map_err(|_| crate::error::MusFuseError::Mount("invalid minutes".into()))?;
    let seconds: u64 = parts[1].parse().map_err(|_| crate::error::MusFuseError::Mount("invalid seconds".into()))?;
    let frames: u64 = parts[2].parse().map_err(|_| crate::error::MusFuseError::Mount("invalid frames".into()))?;
    Ok(minutes * 60 * 75 + seconds * 75 + frames)
}

pub fn frames_to_ms(frames: u64) -> u64 {
    (frames * 1000) / 75
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn parse_simple_cue() {
        let cue = r#"
        TITLE "Album"
        PERFORMER "Artist"
        FILE "disc.flac" WAVE
          TRACK 01 AUDIO
            TITLE "Intro"
            PERFORMER "Artist"
            INDEX 01 00:00:00
          TRACK 02 AUDIO
            TITLE "Song"
            INDEX 01 03:15:00
        "#;

        let parser = CueParser;
        let sheet = parser.parse_str(cue, Path::new("/music")).unwrap();
        assert_eq!(sheet.files.len(), 1);
        let file = &sheet.files[0];
        assert_eq!(file.path, Path::new("/music/disc.flac"));
        assert_eq!(file.tracks.len(), 2);
        assert_eq!(file.tracks[1].index_01_frames, 3 * 60 * 75 + 15 * 75);
    }
}
