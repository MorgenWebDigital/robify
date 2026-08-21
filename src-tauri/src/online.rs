//! metadata enrichment from open web services: itunes search (cover, release
//! type), musicbrainz (ids, release groups) and lrclib (lyrics, time-synced
//! too).
//! note: none of the three needs an api key.

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

/// a metadata suggestion the user can take over or discard.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetadataCandidate {
    pub source: String,
    pub title: String,
    /// lead artists, several of them separated by semicolons.
    pub artist: String,
    /// guest artists, separated by semicolons as well.
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
    /// genius page carrying the lyrics.
    #[serde(default)]
    pub lyrics_url: Option<String>,
    /// genius ids, for fetching track number and release type later.
    #[serde(default)]
    pub genius_song_id: Option<i64>,
    #[serde(default)]
    pub genius_album_id: Option<i64>,
}

// --- genius ---

/// searches genius, which keeps lead and guest artists apart and is therefore
/// the best source for tracks with several participants. its public web api
/// needs no key.
async fn search_genius(query: &str, limit: usize) -> Result<Vec<MetadataCandidate>> {
    let url = format!(
        "https://genius.com/api/search/song?q={}",
        urlencoding::encode(query)
    );
    let body: serde_json::Value = client().get(url).send().await?.json().await?;

    // depending on the endpoint the hits lie flat or in sections
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

    // album and genre appear only in the detail view. the queries run one
    // after another on purpose, genius throttles a flood of requests
    let details = run_sequentially(ids.iter().map(|id| genius_song(*id))).await;

    Ok(details.into_iter().flatten().collect())
}

// works the queries off one by one. genius answers too many simultaneous
// requests with errors, hence deliberately in sequence
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
        // the album object carries the artist as a plain text field
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
        // for tracks of an album the album cover is the fitting image
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

// --- artist data ---

/// a suggestion for an artist: image and description to take over.
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

// genius stores descriptions as a nested tree
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
        // short form as a fallback where the tree is empty
        return artist["description_preview"]
            .as_str()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
    }
    Some(cleaned)
}

// genius uses a placeholder motif for artists without an image, and that is
// not to be stored as a profile picture
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

// --- genius: lyrics and album ---

/// the best known tracks of an artist.
///
/// serves the comparison where names collide: genius lists several artists
/// under "Julia", and without a check the image of the wrong one lands in the
/// profile. whoever's work stands in the library is the one meant.
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

// skips past the `<div>` opening at `open_pos`, counting nested `div`s along
// the way
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

/// fetches the lyrics off a genius song page.
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

        // genius marks headers and notes as not belonging itself
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

/// number of tracks on the album and the position of the track looked for.
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

// --- itunes ---

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

// itunes appends the release type to the album name ("… - Single", "… - EP")
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

/// what kind of release an album is, and how many tracks it holds.
///
/// asked by the name of the album, not by the track. that is the whole point:
/// the match on a track needs title and artist to fit, and a video title
/// carries "(Official Audio)" and half a channel name along, so it fails
/// often. the album name comes from the file and is plain.
///
/// without it everything with an album name counted as an album, and
/// counting the tracks that happen to lie here counts nothing: two tracks out
/// of a record of twenty-two are no single.
pub async fn release_kind(artist: &str, album: &str) -> Option<(String, i64)> {
    let album = album.trim();
    if album.is_empty() {
        return None;
    }

    // three sources, the most exact one first.
    //
    // musicbrainz keeps the kind as a curated field and says outright what a
    // release is. deezer names it too, but out of a shop's catalogue. itunes
    // hides it in the album name and otherwise has to be counted — and
    // counting cannot settle it: "DANGEROUS SUMMER" holds eleven tracks and
    // is an ep all the same.
    if let Some(gefunden) = musicbrainz_release_kind(artist, album).await {
        return Some(gefunden);
    }
    if let Some(gefunden) = deezer_release_kind(artist, album).await {
        return Some(gefunden);
    }

    let url = format!(
        "https://itunes.apple.com/search?term={}&entity=album&limit=5",
        urlencoding::encode(&format!("{artist} {album}"))
    );
    let antwort = client().get(&url).send().await.ok()?.error_for_status().ok()?;
    let daten: serde_json::Value = antwort.json().await.ok()?;

    daten["results"].as_array()?.iter().find_map(|treffer| {
        let sammlung = treffer["collectionName"].as_str()?;
        let kuenstler = treffer["artistName"].as_str().unwrap_or_default();
        let anzahl = treffer["trackCount"].as_i64();

        // itunes writes the kind into the name ("… - EP"), so compare against
        // the name without it
        let (name, art) = itunes_release_type(sammlung, anzahl);
        if !looks_like_same(&name, album) {
            return None;
        }
        // the same album name exists under several artists: "Love Sick" by
        // Don Toliver and by Gemini stand next to each other in the answer
        if !passt_zum_kuenstler(kuenstler, artist) {
            return None;
        }
        Some((art, anzahl.unwrap_or(0)))
    })
}

/// the kind of a release as musicbrainz keeps it.
///
/// the most exact of the three: there the kind is a field of its own, curated
/// by hand, not derived from a shop's catalogue. `primary-type` says Album,
/// Single, EP, Broadcast or Other, and robify knows the first three.
///
/// the score of the answer decides whether it counts. musicbrainz answers a
/// query always, and below ninety it is guessing at the name.
async fn musicbrainz_release_kind(artist: &str, album: &str) -> Option<(String, i64)> {
    musicbrainz_ticket().await;
    let frage = format!(r#"artist:"{artist}" AND release:"{album}""#);
    let url = format!(
        "https://musicbrainz.org/ws/2/release-group?query={}&fmt=json&limit=3",
        urlencoding::encode(&frage)
    );
    let antwort = client().get(&url).send().await.ok()?.error_for_status().ok()?;
    let daten: serde_json::Value = antwort.json().await.ok()?;

    daten["release-groups"].as_array()?.iter().find_map(|gruppe| {
        if gruppe["score"].as_i64().unwrap_or(0) < 90 {
            return None;
        }
        if !looks_like_same(gruppe["title"].as_str()?, album) {
            return None;
        }
        let kuenstler = gruppe["artist-credit"]
            .as_array()
            .and_then(|liste| liste.first())
            .and_then(|eintrag| eintrag["name"].as_str())
            .unwrap_or_default();
        if !passt_zum_kuenstler(kuenstler, artist) {
            return None;
        }
        // a broadcast or anything else musicbrainz knows is none of the three
        // kinds robify tells apart. it is left to the next source rather than
        // pressed into one of them
        let art = match gruppe["primary-type"].as_str()? {
            "EP" => "ep",
            "Single" => "single",
            "Album" => "album",
            _ => return None,
        };
        Some((art.to_string(), 0))
    })
}

/// the kind of a release as deezer states it.
///
/// their catalogue reaches further than apple's into what is not a chart
/// release, and above all they carry the kind as a field of its own:
/// `record_type` says "album", "single", "ep" or "compilation". a collection
/// counts as an album here, robify knows no fourth kind.
async fn deezer_release_kind(artist: &str, album: &str) -> Option<(String, i64)> {
    let url = format!(
        "https://api.deezer.com/search/album?q={}&limit=5",
        urlencoding::encode(&format!("{artist} {album}"))
    );
    let antwort = client().get(&url).send().await.ok()?.error_for_status().ok()?;
    let daten: serde_json::Value = antwort.json().await.ok()?;

    daten["data"].as_array()?.iter().find_map(|treffer| {
        let name = treffer["title"].as_str()?;
        if !looks_like_same(name, album) {
            return None;
        }
        let kuenstler = treffer["artist"]["name"].as_str().unwrap_or_default();
        if !passt_zum_kuenstler(kuenstler, artist) {
            return None;
        }
        let art = match treffer["record_type"].as_str()? {
            "single" => "single",
            "ep" => "ep",
            _ => "album",
        };
        Some((art.to_string(), treffer["nb_tracks"].as_i64().unwrap_or(0)))
    })
}

/// whether two artist fields describe the same person.
///
/// not a plain comparison: a release stands under "Internet Money, Gunna &
/// Don Toliver" while the track knows only "Don Toliver", and the other way
/// round just as often.
fn passt_zum_kuenstler(gefunden: &str, gesucht: &str) -> bool {
    if gesucht.trim().is_empty() {
        return true;
    }
    looks_like_same(gefunden, gesucht)
        || contains_word_sequence(&normalize_for_match(gefunden), &normalize_for_match(gesucht))
        || contains_word_sequence(&normalize_for_match(gesucht), &normalize_for_match(gefunden))
}

/// tracks as deezer carries them.
///
/// the fourth source, and the one asked last for the kind of a release
/// already. what it brings here is the album: deezer names one for nearly
/// every track, where musicbrainz answers with a recording that belongs to no
/// release and genius knows the song but not where it appeared. the album is
/// the field that stood empty most often.
///
/// their track search gives no year and no track number — those come from the
/// other answers, which is the whole point of asking all of them.
async fn search_deezer(query: &str, limit: usize) -> Result<Vec<MetadataCandidate>> {
    let url = format!(
        "https://api.deezer.com/search/track?q={}&limit={}",
        urlencoding::encode(query),
        limit
    );
    let daten: serde_json::Value = client()
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    Ok(daten["data"]
        .as_array()
        .map(|liste| liste.as_slice())
        .unwrap_or_default()
        .iter()
        .filter_map(|treffer| {
            let title = treffer["title"].as_str()?.to_string();
            let artist = treffer["artist"]["name"].as_str().unwrap_or_default().to_string();
            let album = treffer["album"]["title"].as_str().unwrap_or_default().to_string();
            Some(MetadataCandidate {
                source: "Deezer".into(),
                title,
                artist,
                featured_artists: None,
                album,
                album_artist: None,
                release_type: None,
                year: None,
                track_no: None,
                disc_no: None,
                genre: None,
                cover_url: treffer["album"]["cover_xl"]
                    .as_str()
                    .or_else(|| treffer["album"]["cover_big"].as_str())
                    .map(str::to_string),
                mbid: None,
                // deezer counts in seconds
                duration_ms: treffer["duration"].as_i64().map(|s| s * 1000),
                lyrics_url: None,
                genius_song_id: None,
                genius_album_id: None,
            })
        })
        .collect())
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
                // artworkUrl100 scales up to any size
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

// --- musicbrainz ---

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

// musicbrainz allows one request per second by its own policy. with an album
// of thirty tracks this would otherwise run thirty times in parallel and end
// in a block
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
        // reserve the slot right away so waiters queue up instead of pushing
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
                // cover art archive serves the front cover for a release id
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

/// queries every service in parallel, one outage does not break the search.
/// genius comes first because it separates lead and guest artists cleanly.
pub async fn search_metadata(query: &str) -> Result<Vec<MetadataCandidate>> {
    let query = query.trim();
    if query.is_empty() {
        return Ok(Vec::new());
    }
    let (genius, itunes, musicbrainz, deezer) = tokio::join!(
        search_genius(query, 5),
        search_itunes(query, 8),
        search_musicbrainz(query, 8),
        search_deezer(query, 8)
    );

    let mut out = genius.unwrap_or_default();
    out.extend(itunes.unwrap_or_default());
    out.extend(musicbrainz.unwrap_or_default());
    out.extend(deezer.unwrap_or_default());
    if out.is_empty() {
        return Err(anyhow!(fehler!("Keine Metadaten gefunden für „{0}“", query)));
    }
    Ok(out)
}

// --- automatic matching ---
//
// what yt-dlp pulls out of a video description is often imprecise: suffixes
// such as "(Official Video)" in the title, every artist in one field, no
// album. the track is therefore looked up online after the download, but only
// where the hit is certainly the same one.

/// noise words that stand in video titles without belonging to the song title.
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

/// brings a title into comparable shape: no bracketed suffixes, no noise
/// words, no punctuation.
pub fn normalize_for_match(value: &str) -> String {
    let mut text = value.to_lowercase();

    // strip bracket contents, they hold almost nothing but suffixes
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

/// words in a bracket that say nothing about which recording it is.
///
/// "(Official Video)", "(HD)", "[Free Download]" stand behind the original
/// just as behind anything else.
const BEIWERK: [&str; 22] = [
    "official",
    "video",
    "music",
    "audio",
    "lyric",
    "lyrics",
    "visualizer",
    "visualiser",
    "hd",
    "hq",
    "4k",
    "explicit",
    "clean",
    "full",
    "stream",
    "premiere",
    "free",
    "download",
    "dl",
    "out",
    "now",
    "remastered",
];

/// words after which only a name follows, never a version.
const NAMENSWORT: [&str; 6] = ["feat", "ft", "featuring", "with", "prod", "by"];

/// whether a single word says nothing about which recording it is.
pub fn ist_beiwerk_wort(wort: &str) -> bool {
    BEIWERK.contains(&wort) || wort.chars().all(|z| z.is_ascii_digit())
}

/// whether only a name can follow this word.
pub fn ist_namenswort(wort: &str) -> bool {
    NAMENSWORT.contains(&wort)
}

/// whether a bracketed addition says nothing about the version.
///
/// "(Official Video)" and "(feat. Julian Casablancas)" leave the recording
/// what it is. "(Sunrise Cut)" does not.
///
/// needed because no list of markers is ever complete: "remix", "bootleg" and
/// "edit" are known, "sunrise cut" was not, and a bootleg edit of "Sonne"
/// went into the library as the original that way. whatever stands in a
/// bracket and is neither trivia nor a name is a version of its own — that
/// holds for the words nobody has written down yet.
pub fn ist_nur_beiwerk(text: &str) -> bool {
    let worte = normalize_words(text);
    let mut worte = worte.split(' ').filter(|w| !w.is_empty()).peekable();
    match worte.peek() {
        // "feat. Someone" — a name follows, no version
        Some(erstes) if NAMENSWORT.contains(erstes) => return true,
        None => return true,
        _ => {}
    }
    worte.all(|wort| BEIWERK.contains(&wort))
}

/// the bracketed additions of a title, in the order they stand.
pub fn klammerzusaetze(titel: &str) -> Vec<String> {
    let mut gefunden = Vec::new();
    for (auf, zu) in [('(', ')'), ('[', ']'), ('{', '}')] {
        let mut rest = titel;
        while let Some(start) = rest.find(auf) {
            let hinter = &rest[start + auf.len_utf8()..];
            let Some(ende) = hinter.find(zu) else { break };
            gefunden.push(hinter[..ende].to_string());
            rest = &hinter[ende + zu.len_utf8()..];
        }
    }
    gefunden
}

/// splits a text into comparable words without stripping bracket contents,
/// unlike `normalize_for_match`.
///
/// for suffixes such as "(Remix)" or "[Edit]" that is exactly what is needed:
/// they almost always stand in brackets and would be invisible otherwise.
pub fn normalize_words(value: &str) -> String {
    let cleaned: String = value
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect();
    cleaned.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// whether `needle` sits in `haystack` as a complete word sequence.
///
/// a plain substring comparison does not do: "A" sits inside "PA69" while a
/// different artist is meant.
pub fn contains_word_sequence(haystack: &str, needle: &str) -> bool {
    let hay: Vec<&str> = haystack.split(' ').filter(|w| !w.is_empty()).collect();
    let seek: Vec<&str> = needle.split(' ').filter(|w| !w.is_empty()).collect();
    if seek.is_empty() || seek.len() > hay.len() {
        return false;
    }
    hay.windows(seek.len()).any(|window| window == seek.as_slice())
}

/// whether two labels are the same with high confidence.
pub fn looks_like_same(a: &str, b: &str) -> bool {
    let (a, b) = (normalize_for_match(a), normalize_for_match(b));
    if a.is_empty() || b.is_empty() {
        return false;
    }
    if a == b {
        return true;
    }
    // a substring counts only where the shorter text covers the longer well
    let (short, long) = if a.len() <= b.len() { (&a, &b) } else { (&b, &a) };
    long.contains(short.as_str()) && short.len() * 10 >= long.len() * 6
}

/// whether two answers describe the same track.
fn gleiches_stueck(a: &MetadataCandidate, b: &MetadataCandidate) -> bool {
    looks_like_same(&a.title, &b.title) && passt_zum_kuenstler(&a.artist, &b.artist)
}

/// fills what the chosen hit leaves open out of the other answers.
///
/// all four services are asked at the same time, but until now only one of
/// them counted: whichever fitted first won the track whole, the rest was
/// dropped. musicbrainz often names no album, itunes no lyrics, deezer no
/// year — and each time the missing field lay ready in an answer next door.
///
/// title and artist stay untouched, they decided the hit. everything else is
/// only filled where it is empty, and only out of candidates describing the
/// same track.
fn luecken_fuellen(bester: &mut MetadataCandidate, andere: &[MetadataCandidate]) {
    let passend: Vec<&MetadataCandidate> = andere
        .iter()
        .filter(|kandidat| gleiches_stueck(kandidat, bester))
        .collect();

    for kandidat in &passend {
        if bester.featured_artists.is_none() {
            bester.featured_artists = kandidat.featured_artists.clone();
        }
        if bester.year.is_none() {
            bester.year = kandidat.year;
        }
        if bester.genre.is_none() {
            bester.genre = kandidat.genre.clone();
        }
        if bester.cover_url.is_none() {
            bester.cover_url = kandidat.cover_url.clone();
        }
        if bester.duration_ms.is_none() {
            bester.duration_ms = kandidat.duration_ms;
        }
        if bester.mbid.is_none() {
            bester.mbid = kandidat.mbid.clone();
        }
        // the lyrics lie at genius, and the ids belong to that same page
        if bester.lyrics_url.is_none() {
            bester.lyrics_url = kandidat.lyrics_url.clone();
            bester.genius_song_id = bester.genius_song_id.or(kandidat.genius_song_id);
            bester.genius_album_id = bester.genius_album_id.or(kandidat.genius_album_id);
        }
    }

    if bester.album.trim().is_empty() {
        if let Some(album) = album_mit_mehrheit(&passend) {
            bester.album = album;
        }
    }

    // track number, kind and album artist mean nothing without the album they
    // belong to: taken from a different record they name a wrong place. only
    // an answer carrying the same album may fill them
    if bester.album.trim().is_empty() {
        return;
    }
    for kandidat in &passend {
        if !looks_like_same(&kandidat.album, &bester.album) {
            continue;
        }
        if bester.album_artist.is_none() {
            bester.album_artist = kandidat.album_artist.clone();
        }
        if bester.release_type.is_none() {
            bester.release_type = kandidat.release_type.clone();
        }
        if bester.track_no.is_none() {
            bester.track_no = kandidat.track_no;
        }
        if bester.disc_no.is_none() {
            bester.disc_no = kandidat.disc_no;
        }
    }
}

/// the album the most answers agree on.
///
/// taking the first one that fits is not enough. next to every well-known
/// track stand bootlegs, cover versions and samplers, and they name an album
/// too: under "Rammstein - Sonne" a mashup collection called
/// "PLAY045 - THE MEME CUTS III" once won over "Mutter" that way.
///
/// what tells them apart needs no list of suspicious names: the real album is
/// the one several catalogues name independently, the sampler stands in one.
/// on a tie the answer that came first wins — the sources stand in the order
/// of how cleanly they keep their fields.
fn album_mit_mehrheit(kandidaten: &[&MetadataCandidate]) -> Option<String> {
    // normalised name, name as written, how often it was named
    let mut zaehlung: Vec<(String, String, usize)> = Vec::new();
    for kandidat in kandidaten {
        let album = kandidat.album.trim();
        let schluessel = normalize_for_match(album);
        if schluessel.is_empty() {
            continue;
        }
        match zaehlung.iter_mut().find(|(bekannt, _, _)| *bekannt == schluessel) {
            Some(eintrag) => eintrag.2 += 1,
            None => zaehlung.push((schluessel, album.to_string(), 1)),
        }
    }

    let mut bestes: Option<(String, usize)> = None;
    for (_, name, anzahl) in zaehlung {
        if bestes.as_ref().is_none_or(|(_, bisher)| anzahl > *bisher) {
            bestes = Some((name, anzahl));
        }
    }
    bestes.map(|(name, _)| name)
}

/// looks the track up online and takes the details over where the hit fits
/// unambiguously. returns `None` when nothing could be matched with
/// confidence, and the data from the file stays untouched then.
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
    let mut candidates = search_metadata(&query).await.ok()?;

    let stelle = candidates.iter().position(|candidate| {
        looks_like_same(&candidate.title, &metadata.title)
            && (looks_like_same(&candidate.artist, &metadata.artist)
                // with "PA69, Drunken Masters" the artist sits inside the field
                || contains_word_sequence(
                    &normalize_for_match(&metadata.artist),
                    &normalize_for_match(&candidate.artist),
                )
                || contains_word_sequence(
                    &normalize_for_match(&candidate.artist),
                    &normalize_for_match(&metadata.artist),
                ))
    })?;

    let mut best = candidates.remove(stelle);
    luecken_fuellen(&mut best, &candidates);
    Some(enrich(&best, duration_ms, want_cover, want_lyrics).await)
}

/// looks the track up by what the user typed.
///
/// the way out where the file names a channel as its artist. "7clouds",
/// "maukook", "trashpixels" — reuploaders, and `auto_match` needs the artist
/// to fit, so it finds nothing and the channel stays in the library as the
/// artist. with it fall the album and the kind of the release, for under a
/// wrong artist no catalogue answers.
///
/// what confirms the hit here is the input itself: it counts only where both
/// its title and its artist stand in what the user typed. whoever searches
/// for "Daft Punk Instant Crush" has named the artist, and a hit calling
/// itself that is no guess.
pub async fn match_aus_absicht(
    absicht: &str,
    duration_ms: Option<i64>,
    want_cover: bool,
    want_lyrics: bool,
) -> Option<crate::models::TrackMetadata> {
    let getippt = normalize_for_match(absicht);
    if getippt.trim().is_empty() {
        return None;
    }

    let mut candidates = search_metadata(absicht).await.ok()?;
    let stelle = candidates.iter().position(|candidate| {
        contains_word_sequence(&getippt, &normalize_for_match(&candidate.title))
            && contains_word_sequence(&getippt, &normalize_for_match(&candidate.artist))
    })?;

    let mut best = candidates.remove(stelle);
    luecken_fuellen(&mut best, &candidates);
    Some(enrich(&best, duration_ms, want_cover, want_lyrics).await)
}

/// merges the details found with those from the file.
///
/// artist and title win from the online source because they are cleanly
/// separated there, everything else only fills gaps.
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
        // a cover from the file is usually the video thumbnail, the online
        // cover is better where there is one
        cover_base64: keep(found.cover_base64, from_file.cover_base64),
        cover_mime: keep(found.cover_mime, from_file.cover_mime),
        lyrics_synced: keep(found.lyrics_synced, from_file.lyrics_synced),
        lyrics_plain: keep(found.lyrics_plain, from_file.lyrics_plain),
    }
}

/// turns a hit into complete metadata: cover, lyrics, release type and track
/// number are fetched as far as the source hands them out. individual steps
/// failing does no harm, the rest stays.
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

    // cover, album data and lyrics can be fetched at the same time
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

    // without an album it is a single
    if metadata.release_type.is_none() && metadata.album.trim().is_empty() {
        metadata.release_type = Some("single".into());
    }
    // otherwise the album artist equals the lead artist
    if metadata.album_artist.is_none() && !metadata.album.trim().is_empty() {
        metadata.album_artist = Some(candidate.artist.clone());
    }

    if let Some((data, mime)) = cover {
        metadata.cover_base64 = Some(base64::engine::general_purpose::STANDARD.encode(&data));
        metadata.cover_mime = Some(mime);
    }

    // timestamps come from lrclib only, genius delivers the running text better
    match synced {
        Ok(found) => {
            metadata.lyrics_synced = found.synced_lyrics;
            metadata.lyrics_plain = found.plain_lyrics;
        }
        // it used to fail without a word, and that is how it stayed unnoticed
        // that on android no lyrics arrived at all: everything else came
        // through, only this one thing quietly did not
        Err(fehler) if want_lyrics => {
            eprintln!(
                "Lyrics: für „{} – {}“ nichts gefunden: {fehler}",
                candidate.artist, candidate.title
            );
        }
        Err(_) => {}
    }
    if let Some(text) = genius_text {
        metadata.lyrics_plain = Some(text);
    }

    metadata
}

/// upper bound for covers and artist images.
///
/// they land in the database as a blob and in memory as base64, and without a
/// limit a single oversized link is enough to burst both. a cover at print
/// resolution stays far below it.
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

    // check the announced size first, then count while reading anyway: the
    // header is optional and can be missing or lying
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

// --- lyrics ---

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

// a direct hit through exact details, it yields the best synchronisation
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

    // fallback: fuzzy search, then take the best hit
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
        // the artist sits in the shared field
        assert!(normalize_for_match("PA69, Drunken Masters")
            .contains(&normalize_for_match("PA69")));
    }

    #[test]
    fn teiltreffer_nur_als_ganze_woerter() {
        assert!(contains_word_sequence("pa69 official", "pa69"));
        assert!(contains_word_sequence("kanye west jay z", "jay z"));
        // "a" does sit inside "pa69" and is still a different artist
        assert!(!contains_word_sequence("pa69", "a"));
        assert!(!contains_word_sequence("powerless", "power"));
        assert!(!contains_word_sequence("kanye", "kanye west"));
    }

    #[test]
    fn weist_fremde_titel_ab() {
        assert!(!looks_like_same("Monster", "Monster Mash"));
        assert!(!looks_like_same("Power", "Powerless"));
        assert!(!looks_like_same("", "Monster"));
        // a short partial hit does not do
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
        // rebuild of the page layout: header area inside the lyrics container
        let doc = concat!(
            r#"<div data-lyrics-container="true" class="x">"#,
            r#"<div data-exclude-from-selection="true"><div>571 Contributors</div>"#,
            r#"Beschreibung… Read More </div>"#,
            r#"[Intro]<br/>Erste Zeile"#,
            r#"</div>"#,
        );

        // the same steps as in `genius_lyrics`, without touching the network
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

    fn kandidat(quelle: &str, titel: &str, kuenstler: &str) -> MetadataCandidate {
        MetadataCandidate {
            source: quelle.into(),
            title: titel.into(),
            artist: kuenstler.into(),
            featured_artists: None,
            album: String::new(),
            album_artist: None,
            release_type: None,
            year: None,
            track_no: None,
            disc_no: None,
            genre: None,
            cover_url: None,
            mbid: None,
            duration_ms: None,
            lyrics_url: None,
            genius_song_id: None,
            genius_album_id: None,
        }
    }

    #[test]
    fn andere_quellen_fuellen_die_luecken() {
        // musicbrainz names the recording but no release
        let mut bester = kandidat("MusicBrainz", "Instant Crush", "Daft Punk");

        let mut itunes = kandidat("iTunes", "Instant Crush (Official Video)", "Daft Punk");
        itunes.album = "Random Access Memories".into();
        itunes.release_type = Some("album".into());
        itunes.track_no = Some(5);
        itunes.year = Some(2013);
        itunes.cover_url = Some("https://beispiel/cover.jpg".into());

        let mut genius = kandidat("Genius", "Instant Crush", "Daft Punk");
        genius.featured_artists = Some("Julian Casablancas".into());
        genius.lyrics_url = Some("https://genius.com/lied".into());
        genius.genius_song_id = Some(42);
        genius.genius_album_id = Some(7);

        luecken_fuellen(&mut bester, &[itunes, genius]);

        assert_eq!(bester.album, "Random Access Memories");
        assert_eq!(bester.release_type.as_deref(), Some("album"));
        assert_eq!(bester.track_no, Some(5));
        assert_eq!(bester.year, Some(2013));
        assert_eq!(bester.featured_artists.as_deref(), Some("Julian Casablancas"));
        assert_eq!(bester.genius_album_id, Some(7));
        assert!(bester.cover_url.is_some());
        // the hit itself decides title and artist
        assert_eq!(bester.title, "Instant Crush");
        assert_eq!(bester.source, "MusicBrainz");
    }

    #[test]
    fn das_album_das_mehrere_quellen_nennen_gewinnt() {
        // musicbrainz names the recording but no release
        let mut bester = kandidat("MusicBrainz", "Sonne", "Rammstein");

        // a mashup collection stands first and names an album too
        let mut sampler = kandidat("Deezer", "Sonne", "Rammstein");
        sampler.album = "PLAY045 - THE MEME CUTS III".into();

        let mut genius = kandidat("Genius", "Sonne", "Rammstein");
        genius.album = "Mutter".into();
        let mut itunes = kandidat("iTunes", "Sonne", "Rammstein");
        itunes.album = "Mutter".into();
        itunes.release_type = Some("album".into());
        itunes.track_no = Some(3);

        luecken_fuellen(&mut bester, &[sampler, genius, itunes]);

        assert_eq!(bester.album, "Mutter");
        // and what belongs to it comes from an answer naming that same album
        assert_eq!(bester.release_type.as_deref(), Some("album"));
        assert_eq!(bester.track_no, Some(3));
    }

    #[test]
    fn eine_einzige_antwort_reicht_wenn_sie_allein_steht() {
        let mut bester = kandidat("MusicBrainz", "Airwaves", "Pashanim");
        let mut deezer = kandidat("Deezer", "Airwaves", "Pashanim");
        deezer.album = "Airwaves".into();

        luecken_fuellen(&mut bester, &[deezer]);

        assert_eq!(bester.album, "Airwaves");
    }

    #[test]
    fn fremde_stuecke_fuellen_nichts() {
        let mut bester = kandidat("Deezer", "Monster", "Kanye West");
        let mut fremd = kandidat("iTunes", "Monster Mash", "Bobby Pickett");
        fremd.album = "Spooky Hits".into();
        fremd.year = Some(1962);

        luecken_fuellen(&mut bester, &[fremd]);

        assert!(bester.album.is_empty(), "fremdes Album übernommen");
        assert_eq!(bester.year, None);
    }

    #[test]
    fn was_der_treffer_kennt_bleibt_stehen() {
        let mut bester = kandidat("Genius", "Monster", "Kanye West");
        bester.album = "My Beautiful Dark Twisted Fantasy".into();
        bester.year = Some(2010);
        bester.track_no = Some(6);

        let mut anderer = kandidat("iTunes", "Monster", "Kanye West");
        anderer.album = "Monster - Single".into();
        anderer.year = Some(2020);
        anderer.track_no = Some(1);

        luecken_fuellen(&mut bester, &[anderer]);

        assert_eq!(bester.album, "My Beautiful Dark Twisted Fantasy");
        assert_eq!(bester.year, Some(2010));
        // a track number out of a different release must not slip in
        assert_eq!(bester.track_no, Some(6));
    }

    #[test]
    fn titelnummer_nur_aus_demselben_album() {
        let mut bester = kandidat("Deezer", "Monster", "Kanye West");
        bester.album = "My Beautiful Dark Twisted Fantasy".into();

        let mut single = kandidat("iTunes", "Monster", "Kanye West");
        single.album = "Monster".into();
        single.track_no = Some(1);
        single.release_type = Some("single".into());

        luecken_fuellen(&mut bester, &[single]);

        assert_eq!(bester.track_no, None, "Nummer aus fremdem Album");
        assert_eq!(bester.release_type, None, "Art aus fremdem Album");
    }
}
