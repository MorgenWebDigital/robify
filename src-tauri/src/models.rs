use serde::{Deserialize, Serialize};

/// Wie eine Veröffentlichung eingeordnet wird. Bestimmt die Gruppierung
/// auf der Künstlerseite.
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

    /// Fallback-Einordnung, wenn keine Online-Daten vorliegen: Anzahl der Titel.
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
    /// Profilbild hinterlegt? Ausgeliefert wird es über `robify://…/cover/artist/<id>`.
    pub has_image: bool,
    pub bio: Option<String>,
    /// Herkunft der Angaben, z. B. die Genius-Seite.
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

/// Beteiligung eines Künstlers an einem Titel.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackArtist {
    pub id: i64,
    pub name: String,
    /// "main" für Hauptkünstler, "feature" für Gastbeiträge.
    pub role: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Track {
    pub id: i64,
    pub path: String,
    pub title: String,
    /// Hauptkünstler, bestimmt Sortierung und Gruppierung.
    pub artist_id: i64,
    pub artist_name: String,
    /// Alle Beteiligten, Hauptkünstler zuerst.
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
    /// Aus der Bibliothek entfernt. Im Rückblick weiterhin sichtbar, aber
    /// nicht mehr abspielbar.
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
    /// Cover der ersten Titel, für die Mosaik-Vorschau im Frontend.
    pub cover_album_ids: Vec<i64>,
    /// Eigenes Bild hinterlegt. Dann tritt es an die Stelle des Mosaiks.
    pub has_cover: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Lyrics {
    pub track_id: i64,
    /// Zeitsynchrone Zeilen im LRC-Format, falls verfügbar.
    pub synced: Option<String>,
    pub plain: Option<String>,
    pub source: Option<String>,
    pub updated_at: i64,
}

/// Editierbare Metadaten, wird sowohl vom Tag-Editor als auch vom
/// Downloader benutzt.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackMetadata {
    pub title: String,
    /// Hauptkünstler; mehrere werden mit Semikolon getrennt.
    pub artist: String,
    /// Gastkünstler, ebenfalls mit Semikolon getrennt.
    #[serde(default)]
    pub featured_artists: Option<String>,
    pub album: String,
    pub album_artist: Option<String>,
    pub release_type: Option<String>,
    pub year: Option<i64>,
    pub track_no: Option<i64>,
    pub disc_no: Option<i64>,
    pub genre: Option<String>,
    /// Base64-kodiertes Coverbild (ohne Data-URL-Präfix).
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
