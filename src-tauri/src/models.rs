//! the data types that travel between the database, the commands and the ui

use serde::{Deserialize, Serialize};

/// how a release is classified. drives the grouping on the artist page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReleaseType {
    Single,
    Ep,
    Album,
}

impl ReleaseType {
    pub fn as_str(self) -> &'static str {
        match self {
            ReleaseType::Single => "single",
            ReleaseType::Ep => "ep",
            ReleaseType::Album => "album",
        }
    }

    pub fn parse(value: &str) -> ReleaseType {
        match value.trim().to_ascii_lowercase().as_str() {
            "single" => ReleaseType::Single,
            "ep" => ReleaseType::Ep,
            _ => ReleaseType::Album,
        }
    }

    /// fallback classification when no online data is available: track count
    pub fn from_track_count(count: i64) -> ReleaseType {
        match count {
            0..=2 => ReleaseType::Single,
            3..=6 => ReleaseType::Ep,
            _ => ReleaseType::Album,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Artist {
    pub id: i64,
    pub name: String,
    pub sort_name: String,
    pub mbid: Option<String>,
    pub track_count: i64,
    pub release_count: i64,
    /// whether a profile image is stored. it is served over `robify://…/cover/artist/<id>`.
    pub has_image: bool,
    pub bio: Option<String>,
    /// where the details came from, the genius page for instance
    pub source_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Album {
    pub id: i64,
    pub title: String,
    pub artist_id: i64,
    pub artist_name: String,
    pub release_type: ReleaseType,
    pub year: Option<i64>,
    pub mbid: Option<String>,
    pub has_cover: bool,
    pub track_count: i64,
    pub duration_ms: i64,
}

/// an artist's involvement in a track
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackArtist {
    pub id: i64,
    pub name: String,
    /// "main" for the lead artist, "feature" for guest contributions
    pub role: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Track {
    pub id: i64,
    pub path: String,
    pub title: String,
    /// lead artist, drives sorting and grouping
    pub artist_id: i64,
    pub artist_name: String,
    /// everyone involved, lead artist first
    pub artists: Vec<TrackArtist>,
    pub album_id: i64,
    pub album_title: String,
    pub release_type: ReleaseType,
    pub track_no: Option<i64>,
    pub disc_no: Option<i64>,
    pub duration_ms: i64,
    pub genre: Option<String>,
    pub year: Option<i64>,
    pub format: String,
    pub has_cover: bool,
    pub has_lyrics: bool,
    pub added_at: i64,
    pub play_count: i64,
    pub favorite: bool,
    pub source: Option<String>,
    /// removed from the library. still visible in the yearly review, but no
    /// longer playable.
    pub deleted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Playlist {
    pub id: i64,
    pub name: String,
    pub description: Option<String>,
    pub created_at: i64,
    pub track_count: i64,
    pub duration_ms: i64,
    /// covers of the first tracks, for the mosaic preview in the frontend
    pub cover_album_ids: Vec<i64>,
    /// an own image is stored. it takes the place of the mosaic then.
    pub has_cover: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Lyrics {
    pub track_id: i64,
    /// time-synced lines in lrc format, where available
    pub synced: Option<String>,
    pub plain: Option<String>,
    pub source: Option<String>,
    pub updated_at: i64,
}

/// editable metadata, used by the tag editor and the downloader alike
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackMetadata {
    pub title: String,
    /// lead artists, several of them separated by semicolons
    pub artist: String,
    /// guest artists, separated by semicolons as well
    #[serde(default)]
    pub featured_artists: Option<String>,
    pub album: String,
    pub album_artist: Option<String>,
    pub release_type: Option<String>,
    pub year: Option<i64>,
    pub track_no: Option<i64>,
    pub disc_no: Option<i64>,
    pub genre: Option<String>,
    /// cover image in base64, without the data-url prefix
    pub cover_base64: Option<String>,
    pub cover_mime: Option<String>,
    pub lyrics_synced: Option<String>,
    pub lyrics_plain: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryStats {
    pub track_count: i64,
    pub artist_count: i64,
    pub album_count: i64,
    pub playlist_count: i64,
    pub total_duration_ms: i64,
    pub total_listened_ms: i64,
}
