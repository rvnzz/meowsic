use serde::{Deserialize, Serialize};

use super::{img, val_to_id, YAlbum, YArtist, YTrack};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ArtistDto {
    pub id: String,
    pub name: String,
    pub cover_url: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct TrackDto {
    pub id: String,
    pub title: String,
    pub artists: Vec<ArtistDto>,
    pub album_id: Option<String>,
    pub album_title: Option<String>,
    pub cover_url: Option<String>,
    pub cover_url_large: Option<String>,
    pub duration_ms: Option<i64>,
    pub available: bool,
    pub explicit: bool,
}

impl TrackDto {
    pub fn artist_line(&self) -> String {
        if self.artists.is_empty() {
            "—".to_string()
        } else {
            self
                .artists
                .iter()
                .map(|a| a.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        }
    }

    pub fn display(&self) -> String {
        format!("{} — {}", self.artist_line(), self.title)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct AlbumBriefDto {
    pub id: String,
    pub title: String,
    pub cover_url: Option<String>,
    pub year: Option<i64>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct AlbumPageDto {
    pub id: String,
    pub title: String,
    pub cover_url: Option<String>,
    pub year: Option<i64>,
    pub genre: Option<String>,
    pub artists: Vec<ArtistDto>,
    pub tracks: Vec<TrackDto>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct ArtistPageDto {
    pub id: String,
    pub name: String,
    pub cover_url: Option<String>,
    pub description: Option<String>,
    pub listeners: Option<i64>,
    pub genres: Vec<String>,
    pub tracks: Vec<TrackDto>,
    pub albums: Vec<AlbumBriefDto>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct PlaylistDto {
    pub kind: i64,
    pub title: String,
    pub track_count: i64,
    pub cover_url: Option<String>,
    pub owner_uid: Option<i64>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct PlaylistFullDto {
    pub kind: i64,
    pub title: String,
    pub revision: i64,
    pub tracks: Vec<TrackDto>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct SearchDto {
    pub tracks: Vec<TrackDto>,
    pub artists: Vec<ArtistDto>,
    pub albums: Vec<AlbumBriefDto>,
    pub text: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct WheelItemDto {
    pub id: String,
    pub kind: String,
    pub name: String,
    pub description: Option<String>,
    pub cover_url: Option<String>,
    pub station: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct StationOptionDto {
    pub value: String,
    pub name: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct StationInfoDto {
    pub id: String,
    pub name: String,
    pub language: Option<String>,
    pub mood_energy: Option<String>,
    pub languages: Vec<StationOptionDto>,
    pub moods: Vec<StationOptionDto>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct WaveResponse {
    pub station_id: String,
    pub radio_session_id: Option<String>,
    pub batch_id: Option<String>,
    pub tracks: Vec<TrackDto>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct LyricsLineDto {
    pub time_ms: i64,
    pub text: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct LyricsDto {
    pub track_id: String,
    pub synced: bool,
    pub source: String,
    pub lines: Vec<LyricsLineDto>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ProfileDto {
    pub uid: i64,
    pub login: String,
    pub display_name: String,
    pub avatar_url: Option<String>,
    pub has_plus: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct StreamDto {
    pub track_id: String,
    pub url: String,
    pub codec: String,
    pub bitrate: i64,
}

pub(crate) fn map_artist(a: &YArtist) -> ArtistDto {
    let uri = a.cover.as_ref().and_then(|c| c.uri.clone()).or_else(|| a.og_image.clone());
    ArtistDto {
        id: val_to_id(&a.id),
        name: a.name.clone().unwrap_or_default(),
        cover_url: img(uri.as_deref(), "200x200"),
    }
}

pub(crate) fn map_track(t: &YTrack) -> TrackDto {
    let album = t.albums.first();
    let cover_uri = t
        .cover_uri
        .clone()
        .or_else(|| album.and_then(|a| a.cover_uri.clone()));
    TrackDto {
        id: val_to_id(&t.id),
        title: t.title.clone().unwrap_or_default(),
        artists: t.artists.iter().map(map_artist).collect(),
        album_id: album.map(|a| val_to_id(&a.id)),
        album_title: album.and_then(|a| a.title.clone()),
        cover_url: img(cover_uri.as_deref(), "200x200"),
        cover_url_large: img(cover_uri.as_deref(), "1000x1000"),
        duration_ms: t.duration_ms,
        available: t.available.unwrap_or(true),
        explicit: t.content_warning.is_some(),
    }
}

pub(crate) fn map_album_brief(a: &YAlbum) -> AlbumBriefDto {
    AlbumBriefDto {
        id: val_to_id(&a.id),
        title: a.title.clone().unwrap_or_default(),
        cover_url: img(a.cover_uri.as_deref(), "200x200"),
        year: a.year,
    }
}

pub(crate) fn map_tracks(items: &[serde_json::Value]) -> Vec<TrackDto> {
    items
        .iter()
        .filter_map(|raw| serde_json::from_value::<YTrack>(raw.clone()).ok())
        .map(|t| map_track(&t))
        .collect()
}
