//! Anreicherung von Metadaten aus offenen Web-Diensten:
//! iTunes Search (Cover, Release-Art), MusicBrainz (IDs, Release-Gruppen)
//! und LRCLIB (Lyrics, auch zeitsynchron). Alle drei benötigen keinen Key.

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;
use std::time::Duration;
use crate::fehler;

const USER_AGENT: &str = concat!(
    "Robify/",
    env!("CARGO_PKG_VERSION"),
    " ( https://github.com/MorgenWebDigital/robify )"
);

pub fn client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .timeout(Duration::from_secs(20))
            .build()
            .expect("HTTP-Client konnte nicht erstellt werden")
    })
}

/// Ein Metadaten-Vorschlag, den der Nutzer übernehmen oder verwerfen kann.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetadataCandidate {
    pub source: String,
    pub title: String,
    /// Hauptkünstler; mehrere mit Semikolon getrennt.
    pub artist: String,
    /// Gastkünstler, ebenfalls mit Semikolon getrennt.
    pub featured_artists: Option<String>,
    pub album: String,
    pub album_artist: Option<String>,
    pub release_type: Option<String>,
    pub year: Option<i64>,
    pub track_no: Option<i64>,
    pub disc_no: Option<i64>,
    pub genre: Option<String>,
    pub cover_url: Option<String>,
    pub mbid: Option<String>,
    pub duration_ms: Option<i64>,
    /// Genius-Seite mit den Lyrics.
    #[serde(default)]
    pub lyrics_url: Option<String>,
    /// Genius-Kennungen, um Titelnummer und Release-Art nachzuladen.
    #[serde(default)]
    pub genius_song_id: Option<i64>,
    #[serde(default)]
    pub genius_album_id: Option<i64>,
}

// ------------------------------------------------------------------ Genius

/// Genius pflegt Haupt- und Gastkünstler getrennt und ist damit die beste
/// Quelle für Titel mit mehreren Beteiligten. Die öffentliche Web-API
/// braucht keinen Schlüssel.
async fn search_genius(query: &str, limit: usize) -> Result<Vec<MetadataCandidate>> {
    let url = format!(
        "https://genius.com/api/search/song?q={}",
        urlencoding::encode(query)
    );
    let body: serde_json::Value = client().get(url).send().await?.json().await?;

    // Je nach Endpunkt liegen die Treffer flach oder in Abschnitten.
    let hits = body["response"]["hits"]
        .as_array()
        .or_else(|| body["response"]["sections"][0]["hits"].as_array())
        .cloned()
        .unwrap_or_default();

    let ids: Vec<i64> = hits
        .iter()
        .filter_map(|hit| hit["result"]["id"].as_i64())
        .take(limit)
        .collect();

    // Album und Genre stehen erst in der Detailansicht. Die Abfragen laufen
    // bewusst nacheinander. Genius drosselt Anfrageflut.
    let details = run_sequentially(ids.iter().map(|id| genius_song(*id))).await;

    Ok(details.into_iter().flatten().collect())
}

/// Arbeitet die Abfragen der Reihe nach ab. Genius quittiert zu viele
/// gleichzeitige Anfragen mit Fehlern, deshalb bewusst nacheinander.
async fn run_sequentially<T>(
    futures: impl IntoIterator<Item = impl std::future::Future<Output = T>>,
) -> Vec<T> {
    let pending: Vec<_> = futures.into_iter().collect();
    let mut out = Vec::with_capacity(pending.len());
    for future in pending {
        out.push(future.await);
    }
    out
}

fn genius_names(value: &serde_json::Value) -> Vec<String> {
    value
        .as_array()
        .map(|list| {
            list.iter()
                .filter_map(|artist| artist["name"].as_str())
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

async fn genius_song(id: i64) -> Option<MetadataCandidate> {
    let url = format!("https://genius.com/api/songs/{id}");
    let body: serde_json::Value = client().get(url).send().await.ok()?.json().await.ok()?;
    let song = &body["response"]["song"];

    let title = song["title"].as_str()?.trim().to_string();
    let primary = genius_names(&song["primary_artists"]);
    let artist = if primary.is_empty() {
        song["primary_artist"]["name"].as_str()?.to_string()
    } else {
        primary.join("; ")
    };

    let featured = genius_names(&song["featured_artists"]);
    let album = &song["album"];

    Some(MetadataCandidate {
        source: "Genius".into(),
        title,
        artist,
        featured_artists: (!featured.is_empty()).then(|| featured.join("; ")),
        album: album["name"].as_str().unwrap_or("").to_string(),
        // Das Albumobjekt führt den Künstler als Klartextfeld.
        album_artist: album["primary_artist_names"]
            .as_str()
            .or_else(|| album["artist"]["name"].as_str())
            .map(str::to_string),
        release_type: None,
        year: song["release_date_components"]["year"]
            .as_i64()
            .or_else(|| album["release_date_components"]["year"].as_i64()),
        track_no: None,
        disc_no: None,
        genre: song["primary_tag"]["name"].as_str().map(str::to_string),
        // Für Titel eines Albums ist das Albumcover das passendere Bild.
        cover_url: album["cover_art_url"]
            .as_str()
            .or_else(|| song["song_art_image_url"].as_str())
            .or_else(|| song["header_image_url"].as_str())
            .map(str::to_string),
        mbid: None,
        duration_ms: None,
        lyrics_url: song["url"].as_str().map(str::to_string),
        genius_song_id: song["id"].as_i64(),
        genius_album_id: album["id"].as_i64(),
    })
}

// --------------------------------------------------------- Künstlerdaten

/// Vorschlag für einen Künstler. Bild und Beschreibung zum Übernehmen.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtistCandidate {
    pub source: String,
    pub name: String,
    pub image_url: Option<String>,
    pub bio: Option<String>,
    pub url: Option<String>,
    pub genius_id: Option<i64>,
}

/// Genius legt Beschreibungen als verschachtelten Baum ab.
fn flatten_dom(node: &serde_json::Value, out: &mut String) {
    if let Some(text) = node.as_str() {
        out.push_str(text);
        return;
    }
    if let Some(list) = node.as_array() {
        for child in list {
            flatten_dom(child, out);
        }
        return;
    }
    if let Some(object) = node.as_object() {
        let tag = object.get("tag").and_then(|t| t.as_str()).unwrap_or("");
        if matches!(tag, "p" | "br" | "div") && !out.is_empty() && !out.ends_with('\n') {
            out.push('\n');
        }
        if let Some(children) = object.get("children") {
            flatten_dom(children, out);
        }
        if tag == "p" {
            out.push('\n');
        }
    }
}

fn genius_description(artist: &serde_json::Value) -> Option<String> {
    let mut text = String::new();
    flatten_dom(&artist["description"]["dom"], &mut text);

    let cleaned = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");

    if cleaned.is_empty() {
        // Kurzfassung als Rückfall, falls der Baum leer ist.
        return artist["description_preview"]
            .as_str()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
    }
    Some(cleaned)
}

/// Genius nutzt für Künstler ohne Bild ein Platzhaltermotiv, das wollen wir
/// nicht als Profilbild speichern.
fn usable_image(url: Option<&str>) -> Option<String> {
    let url = url?;
    (!url.contains("default_cover_image") && !url.contains("default_avatar"))
        .then(|| url.to_string())
}

pub async fn search_artists(name: &str) -> Result<Vec<ArtistCandidate>> {
    let name = name.trim();
    if name.is_empty() {
        return Ok(Vec::new());
    }

    let url = format!(
        "https://genius.com/api/search/artist?q={}",
        urlencoding::encode(name)
    );
    let body: serde_json::Value = client().get(url).send().await?.json().await?;
    let hits = body["response"]["hits"]
        .as_array()
        .or_else(|| body["response"]["sections"][0]["hits"].as_array())
        .cloned()
        .unwrap_or_default();

    let ids: Vec<i64> = hits
        .iter()
        .filter_map(|hit| hit["result"]["id"].as_i64())
        .take(5)
        .collect();

    let details = run_sequentially(ids.iter().map(|id| genius_artist(*id))).await;
    let out: Vec<ArtistCandidate> = details.into_iter().flatten().collect();

    if out.is_empty() {
        return Err(anyhow!(fehler!("Zu „{0}“ wurde nichts gefunden.", name)));
    }
    Ok(out)
}

pub async fn genius_artist(id: i64) -> Option<ArtistCandidate> {
    let url = format!("https://genius.com/api/artists/{id}");
    let body: serde_json::Value = client().get(url).send().await.ok()?.json().await.ok()?;
    let artist = &body["response"]["artist"];

    Some(ArtistCandidate {
        source: "Genius".into(),
        name: artist["name"].as_str()?.trim().to_string(),
        image_url: usable_image(artist["image_url"].as_str())
            .or_else(|| usable_image(artist["header_image_url"].as_str())),
        bio: genius_description(artist),
        url: artist["url"].as_str().map(str::to_string),
        genius_id: artist["id"].as_i64(),
    })
}

// ------------------------------------------------- Genius: Lyrics und Album

/// Springt hinter das `<div>`, das an `open_pos` beginnt. Zählt dabei
/// Die bekanntesten Titel eines Künstlers.
///
/// Dient dem Abgleich bei Namensgleichheit: Zu „Julia“ führt Genius mehrere
/// Künstler, und ohne Prüfung landet das Bild der falschen im Profil. Wessen
/// Werk in der Bibliothek steht, ist der Gesuchte.
pub async fn artist_songs(genius_id: i64, limit: usize) -> Vec<String> {
    let url = format!(
        "https://genius.com/api/artists/{genius_id}/songs?per_page={}&sort=popularity",
        limit.clamp(1, 50)
    );
    let Ok(response) = client().get(url).send().await else {
        return Vec::new();
    };
    let Ok(body) = response.json::<serde_json::Value>().await else {
        return Vec::new();
    };

    body["response"]["songs"]
        .as_array()
        .map(|songs| {
            songs
                .iter()
                .filter_map(|song| song["title"].as_str())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// verschachtelte `div`s mit.
fn skip_div(doc: &str, open_pos: usize) -> usize {
    let mut cursor = match doc[open_pos..].find('>') {
        Some(index) => open_pos + index + 1,
        None => return doc.len(),
    };
    let mut depth = 1usize;

    while depth > 0 {
        let next_open = doc[cursor..].find("<div").map(|i| cursor + i);
        let next_close = doc[cursor..].find("</div>").map(|i| cursor + i);
        match (next_open, next_close) {
            (_, None) => return doc.len(),
            (Some(open), Some(close)) if open < close => {
                depth += 1;
                cursor = open + 4;
            }
            (_, Some(close)) => {
                depth -= 1;
                cursor = close + 6;
            }
        }
    }
    cursor
}

fn unescape_entities(value: &str) -> String {
    value
        .replace("&nbsp;", " ")
        .replace("&#x27;", "'")
        .replace("&#39;", "'")
        .replace("&quot;", "\"")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

fn strip_tags(fragment: &str) -> String {
    let with_breaks = fragment
        .replace("<br/>", "\n")
        .replace("<br />", "\n")
        .replace("<br>", "\n");

    let mut out = String::with_capacity(with_breaks.len());
    let mut inside_tag = false;
    for character in with_breaks.chars() {
        match character {
            '<' => inside_tag = true,
            '>' => inside_tag = false,
            c if !inside_tag => out.push(c),
            _ => {}
        }
    }
    unescape_entities(&out)
}

/// Holt die Lyrics von einer Genius-Songseite.
pub async fn genius_lyrics(url: &str) -> Result<String> {
    let doc = client()
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;

    let mut parts = Vec::new();
    let mut position = 0;
    while let Some(found) = doc[position..].find(r#"data-lyrics-container="true""#) {
        let marker = position + found;
        let content_start = match doc[marker..].find('>') {
            Some(index) => marker + index + 1,
            None => break,
        };
        let container_end = skip_div(&doc, marker);
        let mut fragment = doc[content_start..container_end.saturating_sub(6)].to_string();

        // Genius markiert Kopfzeilen und Hinweise selbst als nicht zugehörig.
        while let Some(excluded) = fragment.find(r#"data-exclude-from-selection="true""#) {
            let Some(open) = fragment[..excluded].rfind("<div") else {
                break;
            };
            let end = skip_div(&fragment, open);
            fragment = format!("{}{}", &fragment[..open], &fragment[end..]);
        }

        parts.push(strip_tags(&fragment));
        position = container_end;
    }

    let lyrics = parts.join("\n").trim().to_string();
    if lyrics.is_empty() {
        return Err(anyhow!(fehler!("Auf der Genius-Seite standen keine Lyrics")));
    }
    Ok(lyrics)
}

/// Anzahl der Albumtitel und die Nummer des gesuchten Titels.
async fn genius_album_tracks(album_id: i64, song_id: Option<i64>) -> Option<(i64, Option<i64>)> {
    let url = format!("https://genius.com/api/albums/{album_id}/tracks?per_page=50");
    let body: serde_json::Value = client().get(url).send().await.ok()?.json().await.ok()?;
    let tracks = body["response"]["tracks"].as_array()?;

    let track_no = song_id.and_then(|id| {
        tracks
            .iter()
            .find(|entry| entry["song"]["id"].as_i64() == Some(id))
            .and_then(|entry| entry["number"].as_i64())
    });
    Some((tracks.len() as i64, track_no))
}

// ------------------------------------------------------------------ iTunes

#[derive(Deserialize)]
struct ItunesResponse {
    results: Vec<ItunesTrack>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ItunesTrack {
    track_name: Option<String>,
    artist_name: Option<String>,
    collection_name: Option<String>,
    collection_artist_name: Option<String>,
    artwork_url100: Option<String>,
    release_date: Option<String>,
    primary_genre_name: Option<String>,
    track_number: Option<i64>,
    disc_number: Option<i64>,
    track_count: Option<i64>,
    track_time_millis: Option<i64>,
}

/// iTunes hängt die Release-Art an den Albumnamen ("… - Single", "… - EP").
fn itunes_release_type(collection: &str, track_count: Option<i64>) -> (String, String) {
    let lower = collection.to_lowercase();
    if let Some(stripped) = lower.strip_suffix(" - single") {
        return (collection[..stripped.len()].to_string(), "single".into());
    }
    if let Some(stripped) = lower.strip_suffix(" - ep") {
        return (collection[..stripped.len()].to_string(), "ep".into());
    }
    let kind = match track_count {
        Some(c) if c <= 2 => "single",
        Some(c) if c <= 6 => "ep",
        _ => "album",
    };
    (collection.to_string(), kind.into())
}

async fn search_itunes(query: &str, limit: usize) -> Result<Vec<MetadataCandidate>> {
    let url = format!(
        "https://itunes.apple.com/search?term={}&entity=song&limit={}",
        urlencoding::encode(query),
        limit
    );
    let response: ItunesResponse = client().get(url).send().await?.json().await?;

    Ok(response
        .results
        .into_iter()
        .filter_map(|t| {
            let title = t.track_name?;
            let artist = t.artist_name.unwrap_or_default();
            let collection = t.collection_name.unwrap_or_else(|| title.clone());
            let (album, release_type) = itunes_release_type(&collection, t.track_count);
            Some(MetadataCandidate {
                source: "iTunes".into(),
                title,
                artist,
                featured_artists: None,
                album,
                album_artist: t.collection_artist_name,
                release_type: Some(release_type),
                year: t
                    .release_date
                    .as_deref()
                    .and_then(|d| d.get(0..4))
                    .and_then(|y| y.parse().ok()),
                track_no: t.track_number,
                disc_no: t.disc_number,
                genre: t.primary_genre_name,
                // artworkUrl100 lässt sich auf beliebige Größen hochskalieren.
                cover_url: t
                    .artwork_url100
                    .map(|u| u.replace("100x100bb", "1000x1000bb")),
                mbid: None,
                duration_ms: t.track_time_millis,
                lyrics_url: None,
                genius_song_id: None,
                genius_album_id: None,
            })
        })
        .collect())
}

// ------------------------------------------------------------- MusicBrainz

#[derive(Deserialize)]
struct MbResponse {
    recordings: Vec<MbRecording>,
}

#[derive(Deserialize)]
struct MbRecording {
    id: String,
    title: String,
    length: Option<i64>,
    #[serde(rename = "artist-credit", default)]
    artist_credit: Vec<MbArtistCredit>,
    #[serde(default)]
    releases: Vec<MbRelease>,
}

#[derive(Deserialize)]
struct MbArtistCredit {
    name: String,
}

#[derive(Deserialize)]
struct MbRelease {
    id: String,
    title: String,
    date: Option<String>,
    #[serde(rename = "release-group")]
    release_group: Option<MbReleaseGroup>,
}

#[derive(Deserialize)]
struct MbReleaseGroup {
    #[serde(rename = "primary-type")]
    primary_type: Option<String>,
    #[serde(rename = "secondary-types", default)]
    secondary_types: Vec<String>,
}

/// MusicBrainz erlaubt laut eigener Richtlinie eine Anfrage pro Sekunde.
/// Bei einem Album mit dreißig Titeln liefe das sonst dreißigfach parallel,
/// und endete in einer Sperre.
async fn musicbrainz_ticket() {
    use std::sync::{Mutex, OnceLock};
    use std::time::{Duration, Instant};
    static LAST: OnceLock<Mutex<Option<Instant>>> = OnceLock::new();

    const MIN_GAP: Duration = Duration::from_secs(1);
    let warten = {
        let lock = LAST.get_or_init(|| Mutex::new(None));
        let mut zuletzt = lock.lock().unwrap_or_else(|vergiftet| vergiftet.into_inner());
        let jetzt = Instant::now();
        let rest = zuletzt
            .map(|vorher| MIN_GAP.saturating_sub(jetzt.duration_since(vorher)))
            .unwrap_or_default();
        // Gleich vormerken, damit sich Wartende einreihen statt zu drängeln.
        *zuletzt = Some(jetzt + rest);
        rest
    };
    if !warten.is_zero() {
        tokio::time::sleep(warten).await;
    }
}

async fn search_musicbrainz(query: &str, limit: usize) -> Result<Vec<MetadataCandidate>> {
    musicbrainz_ticket().await;
    let url = format!(
        "https://musicbrainz.org/ws/2/recording?query={}&fmt=json&limit={}",
        urlencoding::encode(query),
        limit
    );
    let response: MbResponse = client().get(url).send().await?.json().await?;

    Ok(response
        .recordings
        .into_iter()
        .map(|r| {
            let artist = r
                .artist_credit
                .iter()
                .map(|a| a.name.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            let release = r.releases.first();
            let release_type = release
                .and_then(|rel| rel.release_group.as_ref())
                .map(|group| {
                    if group
                        .secondary_types
                        .iter()
                        .any(|s| s.eq_ignore_ascii_case("Compilation"))
                    {
                        "album".to_string()
                    } else {
                        match group.primary_type.as_deref() {
                            Some("Single") => "single".into(),
                            Some("EP") => "ep".into(),
                            _ => "album".into(),
                        }
                    }
                });

            MetadataCandidate {
                source: "MusicBrainz".into(),
                title: r.title,
                artist,
                featured_artists: None,
                album: release.map(|rel| rel.title.clone()).unwrap_or_default(),
                album_artist: None,
                release_type,
                year: release
                    .and_then(|rel| rel.date.as_deref())
                    .and_then(|d| d.get(0..4))
                    .and_then(|y| y.parse().ok()),
                track_no: None,
                disc_no: None,
                genre: None,
                // Cover Art Archive liefert das Front-Cover zur Release-ID.
                cover_url: release
                    .map(|rel| format!("https://coverartarchive.org/release/{}/front-500", rel.id)),
                mbid: Some(r.id),
                duration_ms: r.length,
                lyrics_url: None,
                genius_song_id: None,
                genius_album_id: None,
            }
        })
        .collect())
}

/// Fragt alle Dienste parallel ab; ein Ausfall macht die Suche nicht kaputt.
/// Genius steht vorn, weil es Haupt- und Gastkünstler sauber trennt.
pub async fn search_metadata(query: &str) -> Result<Vec<MetadataCandidate>> {
    let query = query.trim();
    if query.is_empty() {
        return Ok(Vec::new());
    }
    let (genius, itunes, musicbrainz) = tokio::join!(
        search_genius(query, 5),
        search_itunes(query, 8),
        search_musicbrainz(query, 8)
    );

    let mut out = genius.unwrap_or_default();
    out.extend(itunes.unwrap_or_default());
    out.extend(musicbrainz.unwrap_or_default());
    if out.is_empty() {
        return Err(anyhow!(fehler!("Keine Metadaten gefunden für „{0}“", query)));
    }
    Ok(out)
}

// ------------------------------------------- Automatische Zuordnung
//
// Was yt-dlp aus einer Videobeschreibung zieht, ist oft ungenau: Zusätze wie
// „(Official Video)“ im Titel, alle Künstler in einem Feld, kein Album.
// Deshalb suchen wir den Titel nach dem Download online, aber nur, wenn der
// Treffer sicher derselbe ist.

/// Störwörter, die in Videotiteln stehen, aber nicht zum Songtitel gehören.
const TITLE_NOISE: [&str; 14] = [
    "official video",
    "official music video",
    "official audio",
    "official lyric video",
    "lyric video",
    "lyrics",
    "visualizer",
    "audio only",
    "full album",
    "remastered",
    "hq",
    "hd",
    "4k",
    "explicit",
];

/// Bringt einen Titel auf eine vergleichbare Form: ohne Klammerzusätze,
/// Störwörter und Satzzeichen.
pub fn normalize_for_match(value: &str) -> String {
    let mut text = value.to_lowercase();

    // Klammerinhalte entfernen, dort stehen fast immer nur Zusätze.
    for (open, close) in [('(', ')'), ('[', ']'), ('{', '}')] {
        while let Some(start) = text.find(open) {
            match text[start..].find(close) {
                Some(offset) => {
                    text.replace_range(start..start + offset + close.len_utf8(), " ");
                }
                None => break,
            }
        }
    }

    for noise in TITLE_NOISE {
        text = text.replace(noise, " ");
    }

    let cleaned: String = text
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect();
    cleaned.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Zerlegt einen Text in vergleichbare Wörter, **ohne** Klammerinhalte zu
/// entfernen, anders als [`normalize_for_match`].
///
/// Für Zusätze wie „(Remix)“ oder „[Edit]“ ist genau das nötig: Sie stehen
/// fast immer in Klammern und würden sonst unsichtbar.
pub fn normalize_words(value: &str) -> String {
    let cleaned: String = value
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect();
    cleaned.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Steckt `needle` als vollständige Wortfolge in `haystack`?
///
/// Ein reiner Teilstring-Vergleich reicht nicht: „A“ steckt in „PA69“,
/// gemeint ist aber ein anderer Künstler.
pub fn contains_word_sequence(haystack: &str, needle: &str) -> bool {
    let hay: Vec<&str> = haystack.split(' ').filter(|w| !w.is_empty()).collect();
    let seek: Vec<&str> = needle.split(' ').filter(|w| !w.is_empty()).collect();
    if seek.is_empty() || seek.len() > hay.len() {
        return false;
    }
    hay.windows(seek.len()).any(|window| window == seek.as_slice())
}

/// Sind zwei Bezeichnungen mit hoher Sicherheit dasselbe?
pub fn looks_like_same(a: &str, b: &str) -> bool {
    let (a, b) = (normalize_for_match(a), normalize_for_match(b));
    if a.is_empty() || b.is_empty() {
        return false;
    }
    if a == b {
        return true;
    }
    // Teilstring zählt nur, wenn der kürzere Text den längeren gut abdeckt.
    let (short, long) = if a.len() <= b.len() { (&a, &b) } else { (&b, &a) };
    long.contains(short.as_str()) && short.len() * 10 >= long.len() * 6
}

/// Sucht online nach dem Titel und übernimmt die Angaben, wenn der Treffer
/// eindeutig passt. Gibt `None` zurück, wenn nichts sicher zugeordnet werden
/// konnte, dann bleiben die Daten aus der Datei unangetastet.
pub async fn auto_match(
    metadata: &crate::models::TrackMetadata,
    duration_ms: Option<i64>,
    want_cover: bool,
    want_lyrics: bool,
) -> Option<crate::models::TrackMetadata> {
    let title = normalize_for_match(&metadata.title);
    if title.is_empty() {
        return None;
    }

    let query = format!("{} {}", metadata.artist, metadata.title);
    let candidates = search_metadata(&query).await.ok()?;

    let best = candidates.into_iter().find(|candidate| {
        looks_like_same(&candidate.title, &metadata.title)
            && (looks_like_same(&candidate.artist, &metadata.artist)
                // Bei „PA69, Drunken Masters“ steckt der Künstler im Feld drin.
                || contains_word_sequence(
                    &normalize_for_match(&metadata.artist),
                    &normalize_for_match(&candidate.artist),
                )
                || contains_word_sequence(
                    &normalize_for_match(&candidate.artist),
                    &normalize_for_match(&metadata.artist),
                ))
    })?;

    Some(enrich(&best, duration_ms, want_cover, want_lyrics).await)
}

/// Führt gefundene Angaben mit denen aus der Datei zusammen.
/// Künstler und Titel gewinnen von der Online-Quelle, weil sie dort sauber
/// getrennt sind; alles Übrige füllt nur Lücken.
pub fn merge_match(
    from_file: crate::models::TrackMetadata,
    found: crate::models::TrackMetadata,
) -> crate::models::TrackMetadata {
    let keep = |preferred: Option<String>, fallback: Option<String>| {
        preferred
            .filter(|value| !value.trim().is_empty())
            .or(fallback)
    };

    crate::models::TrackMetadata {
        title: if found.title.trim().is_empty() {
            from_file.title
        } else {
            found.title
        },
        artist: if found.artist.trim().is_empty() {
            from_file.artist
        } else {
            found.artist
        },
        featured_artists: keep(found.featured_artists, from_file.featured_artists),
        album: if found.album.trim().is_empty() {
            from_file.album
        } else {
            found.album
        },
        album_artist: keep(found.album_artist, from_file.album_artist),
        release_type: keep(found.release_type, from_file.release_type),
        year: found.year.or(from_file.year),
        track_no: found.track_no.or(from_file.track_no),
        disc_no: found.disc_no.or(from_file.disc_no),
        genre: keep(found.genre, from_file.genre),
        // Ein Cover aus der Datei ist meist das Videobild, das Online-Cover
        // ist besser, falls vorhanden.
        cover_base64: keep(found.cover_base64, from_file.cover_base64),
        cover_mime: keep(found.cover_mime, from_file.cover_mime),
        lyrics_synced: keep(found.lyrics_synced, from_file.lyrics_synced),
        lyrics_plain: keep(found.lyrics_plain, from_file.lyrics_plain),
    }
}

/// Macht aus einem Treffer vollständige Metadaten: Cover, Lyrics,
/// Release-Art und Titelnummer werden nachgeladen, soweit die Quelle sie
/// hergibt. Fehlschläge einzelner Schritte sind unkritisch, der Rest bleibt.
pub async fn enrich(
    candidate: &MetadataCandidate,
    duration_ms: Option<i64>,
    want_cover: bool,
    want_lyrics: bool,
) -> crate::models::TrackMetadata {
    use base64::Engine;

    let mut metadata = crate::models::TrackMetadata {
        title: candidate.title.clone(),
        artist: candidate.artist.clone(),
        featured_artists: candidate.featured_artists.clone(),
        album: candidate.album.clone(),
        album_artist: candidate.album_artist.clone(),
        release_type: candidate.release_type.clone(),
        year: candidate.year,
        track_no: candidate.track_no,
        disc_no: candidate.disc_no,
        genre: candidate.genre.clone(),
        cover_base64: None,
        cover_mime: None,
        lyrics_synced: None,
        lyrics_plain: None,
    };

    // Cover, Albumdaten und Lyrics lassen sich gleichzeitig holen.
    let album_lookup = async {
        match candidate.genius_album_id {
            Some(album_id) => genius_album_tracks(album_id, candidate.genius_song_id).await,
            None => None,
        }
    };
    let lyrics_lookup = async {
        match candidate.lyrics_url.as_deref().filter(|_| want_lyrics) {
            Some(url) => genius_lyrics(url).await.ok(),
            None => None,
        }
    };
    let cover_lookup = async {
        match candidate.cover_url.as_deref().filter(|_| want_cover) {
            Some(url) => fetch_image(url).await.ok(),
            None => None,
        }
    };
    let synced_lookup = async {
        if !want_lyrics {
            return Err(anyhow!(fehler!("Lyrics nicht angefordert")));
        }
        get_lyrics(
            &candidate.artist,
            &candidate.title,
            Some(&candidate.album),
            duration_ms,
        )
        .await
    };

    let (album_info, genius_text, cover, synced) =
        tokio::join!(album_lookup, lyrics_lookup, cover_lookup, synced_lookup);

    if let Some((track_count, track_no)) = album_info {
        if metadata.track_no.is_none() {
            metadata.track_no = track_no;
        }
        if metadata.release_type.is_none() && track_count > 0 {
            metadata.release_type = Some(
                crate::models::ReleaseType::from_track_count(track_count)
                    .as_str()
                    .to_string(),
            );
        }
    }

    // Ohne Albumangabe ist es eine Single.
    if metadata.release_type.is_none() && metadata.album.trim().is_empty() {
        metadata.release_type = Some("single".into());
    }
    // Der Albumkünstler entspricht sonst dem Hauptkünstler.
    if metadata.album_artist.is_none() && !metadata.album.trim().is_empty() {
        metadata.album_artist = Some(candidate.artist.clone());
    }

    if let Some((data, mime)) = cover {
        metadata.cover_base64 = Some(base64::engine::general_purpose::STANDARD.encode(&data));
        metadata.cover_mime = Some(mime);
    }

    // Zeitmarken gibt es nur bei LRCLIB, den Fließtext liefert Genius besser.
    if let Ok(found) = synced {
        metadata.lyrics_synced = found.synced_lyrics;
        metadata.lyrics_plain = found.plain_lyrics;
    }
    if let Some(text) = genius_text {
        metadata.lyrics_plain = Some(text);
    }

    metadata
}

/// Obergrenze für Cover und Künstlerbilder.
///
/// Sie landen als BLOB in der Datenbank und als Base64 im Arbeitsspeicher,
/// ohne Grenze reicht ein einziger übergroßer Verweis, um beides zu sprengen.
/// Ein Cover in Druckauflösung liegt weit darunter.
const MAX_IMAGE_BYTES: usize = 12 * 1024 * 1024;

pub async fn fetch_image(url: &str) -> Result<(Vec<u8>, String)> {
    let mut response = client().get(url).send().await?.error_for_status()?;
    let mime = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("image/jpeg")
        .split(';')
        .next()
        .unwrap_or("image/jpeg")
        .to_string();

    // Angekündigte Größe zuerst prüfen, dann trotzdem beim Lesen mitzählen:
    // Die Angabe ist freiwillig und kann fehlen oder lügen.
    if response.content_length().is_some_and(|len| len > MAX_IMAGE_BYTES as u64) {
        return Err(anyhow!(fehler!("Bild von {0} ist zu groß", url)));
    }

    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        if bytes.len() + chunk.len() > MAX_IMAGE_BYTES {
            return Err(anyhow!(fehler!("Bild von {0} ist zu groß", url)));
        }
        bytes.extend_from_slice(&chunk);
    }

    if bytes.is_empty() {
        return Err(anyhow!(fehler!("Leeres Bild von {0}", url)));
    }
    Ok((bytes, mime))
}

// ------------------------------------------------------------------ Lyrics

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LyricsCandidate {
    pub id: i64,
    pub track_name: String,
    pub artist_name: String,
    pub album_name: Option<String>,
    pub duration: Option<f64>,
    pub plain_lyrics: Option<String>,
    pub synced_lyrics: Option<String>,
}

/// Direkter Treffer über exakte Angaben, liefert die beste Synchronisation.
pub async fn get_lyrics(
    artist: &str,
    title: &str,
    album: Option<&str>,
    duration_ms: Option<i64>,
) -> Result<LyricsCandidate> {
    let mut url = format!(
        "https://lrclib.net/api/get?artist_name={}&track_name={}",
        urlencoding::encode(artist),
        urlencoding::encode(title)
    );
    if let Some(album) = album.filter(|a| !a.trim().is_empty()) {
        url.push_str(&format!("&album_name={}", urlencoding::encode(album)));
    }
    if let Some(ms) = duration_ms.filter(|ms| *ms > 0) {
        url.push_str(&format!("&duration={}", ms / 1000));
    }

    let response = client().get(&url).send().await?;
    if response.status().is_success() {
        return Ok(response.json().await?);
    }

    // Fallback: unscharfe Suche, dann den besten Treffer nehmen.
    search_lyrics(&format!("{artist} {title}"))
        .await?
        .into_iter()
        .find(|c| c.synced_lyrics.is_some() || c.plain_lyrics.is_some())
        .ok_or_else(|| anyhow!(fehler!("Keine Lyrics gefunden für „{0} · {1}“", artist, title)))
}

pub async fn search_lyrics(query: &str) -> Result<Vec<LyricsCandidate>> {
    let url = format!(
        "https://lrclib.net/api/search?q={}",
        urlencoding::encode(query.trim())
    );
    let candidates: Vec<LyricsCandidate> = client()
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    Ok(candidates)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raeumt_videotitel_auf() {
        assert_eq!(
            normalize_for_match("Die Welt zu Gast bei Feinden (Official Video)"),
            "die welt zu gast bei feinden"
        );
        assert_eq!(
            normalize_for_match("Monster [Official Audio] HD"),
            "monster"
        );
        assert_eq!(normalize_for_match("P.O.W.E.R."), "p o w e r");
    }

    #[test]
    fn erkennt_denselben_titel_trotz_zusaetzen() {
        assert!(looks_like_same(
            "Die Welt zu Gast bei Feinden",
            "Die Welt zu Gast bei Feinden (Official Video)"
        ));
        assert!(looks_like_same("Monster", "MONSTER"));
        // Der Künstler steckt im gemeinsamen Feld.
        assert!(normalize_for_match("PA69, Drunken Masters")
            .contains(&normalize_for_match("PA69")));
    }

    #[test]
    fn teiltreffer_nur_als_ganze_woerter() {
        assert!(contains_word_sequence("pa69 official", "pa69"));
        assert!(contains_word_sequence("kanye west jay z", "jay z"));
        // „a“ steckt zwar in „pa69“, ist aber ein anderer Künstler.
        assert!(!contains_word_sequence("pa69", "a"));
        assert!(!contains_word_sequence("powerless", "power"));
        assert!(!contains_word_sequence("kanye", "kanye west"));
    }

    #[test]
    fn weist_fremde_titel_ab() {
        assert!(!looks_like_same("Monster", "Monster Mash"));
        assert!(!looks_like_same("Power", "Powerless"));
        assert!(!looks_like_same("", "Monster"));
        // Ein kurzer Teiltreffer reicht nicht.
        assert!(!looks_like_same("Go", "Go Down Deh Remix Version"));
    }

    #[test]
    fn findet_das_ende_verschachtelter_divs() {
        let doc = r#"<div a><div b></div><span></span></div>REST"#;
        assert_eq!(&doc[skip_div(doc, 0)..], "REST");
    }

    #[test]
    fn unvollstaendiges_html_endet_sauber() {
        let doc = "<div a><div b>ohne Ende";
        assert_eq!(skip_div(doc, 0), doc.len());
    }

    #[test]
    fn wandelt_umbrueche_und_entitaeten() {
        let fragment = r##"Zeile eins<br/>Zeile <a href="#">zwei</a> &amp; drei&#x27;s"##;
        assert_eq!(strip_tags(fragment), "Zeile eins\nZeile zwei & drei's");
    }

    #[test]
    fn entfernt_von_genius_ausgeschlossene_bloecke() {
        // Nachbau des Seitenaufbaus: Kopfbereich im Lyrics-Container.
        let doc = concat!(
            r#"<div data-lyrics-container="true" class="x">"#,
            r#"<div data-exclude-from-selection="true"><div>571 Contributors</div>"#,
            r#"Beschreibung… Read More </div>"#,
            r#"[Intro]<br/>Erste Zeile"#,
            r#"</div>"#,
        );

        // Dieselben Schritte wie in `genius_lyrics`, ohne Netzzugriff.
        let marker = doc.find(r#"data-lyrics-container="true""#).unwrap();
        let content_start = marker + doc[marker..].find('>').unwrap() + 1;
        let container_end = skip_div(doc, marker);
        let mut fragment = doc[content_start..container_end - 6].to_string();

        while let Some(excluded) = fragment.find(r#"data-exclude-from-selection="true""#) {
            let open = fragment[..excluded].rfind("<div").unwrap();
            let end = skip_div(&fragment, open);
            fragment = format!("{}{}", &fragment[..open], &fragment[end..]);
        }

        let text = strip_tags(&fragment);
        assert_eq!(text.trim(), "[Intro]\nErste Zeile");
        assert!(!text.contains("Contributors"));
        assert!(!text.contains("Read More"));
    }
}
