//! resolving spotify links.
//!
//! spotify serves its audio encrypted only, none of it can be downloaded.
//! what is publicly reachable is the metadata: title, artist, album, length
//! and cover. that is what is fetched here through the embed page, no api key
//! involved, and the matching recording is then searched through the ordinary
//! sources.

use crate::online;
use anyhow::{anyhow, bail, Result};
use serde::Serialize;
use crate::fehler;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SpotifyKind {
    Track,
    Album,
    Playlist,
    Artist,
}

impl SpotifyKind {
    fn from_segment(segment: &str) -> Option<SpotifyKind> {
        match segment {
            "track" => Some(SpotifyKind::Track),
            "album" => Some(SpotifyKind::Album),
            "playlist" => Some(SpotifyKind::Playlist),
            "artist" => Some(SpotifyKind::Artist),
            _ => None,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            SpotifyKind::Track => "track",
            SpotifyKind::Album => "album",
            SpotifyKind::Playlist => "playlist",
            SpotifyKind::Artist => "artist",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            SpotifyKind::Track => "Titel",
            SpotifyKind::Album => "Album",
            SpotifyKind::Playlist => "Playlist",
            SpotifyKind::Artist => "Künstler",
        }
    }
}

#[derive(Debug, Clone)]
pub struct SpotifyRef {
    pub kind: SpotifyKind,
    pub id: String,
}

/// recognises `https://open.spotify.com/track/…`, country paths such as
/// `/intl-de/`, and `spotify:track:…`.
pub fn parse_link(input: &str) -> Option<SpotifyRef> {
    let input = input.trim();

    if let Some(rest) = input.strip_prefix("spotify:") {
        let mut parts = rest.split(':');
        let kind = SpotifyKind::from_segment(parts.next()?)?;
        let id = parts.next()?.trim();
        return (!id.is_empty()).then(|| SpotifyRef {
            kind,
            id: id.to_string(),
        });
    }

    let lower = input.to_ascii_lowercase();
    if !lower.contains("spotify.com") {
        return None;
    }

    // cut off query and fragment, then walk the path
    let path = input
        .split(['?', '#'])
        .next()?
        .trim_end_matches('/');
    let segments: Vec<&str> = path.split('/').collect();

    for (index, segment) in segments.iter().enumerate() {
        if let Some(kind) = SpotifyKind::from_segment(&segment.to_ascii_lowercase()) {
            let id = segments.get(index + 1)?.trim();
            return (!id.is_empty()).then(|| SpotifyRef {
                kind,
                id: id.to_string(),
            });
        }
    }
    None
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpotifyTrack {
    /// id of the track at spotify where known. it allows looking up its own
    /// cover, which the track list does not supply.
    pub id: Option<String>,
    pub title: String,
    /// everyone involved, lead artist first. spotify draws no line around
    /// guest contributions, whoever is listed stands there as an equal.
    pub artists: Vec<String>,
    /// guest artists pulled out of the title ("… (feat. X)")
    pub featured: Vec<String>,
    pub duration_ms: Option<i64>,
    pub track_no: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpotifyRelease {
    pub kind: SpotifyKind,
    /// name of the album, the playlist or the track
    pub name: String,
    pub artist: Option<String>,
    pub cover_url: Option<String>,
    pub year: Option<i64>,
    pub tracks: Vec<SpotifyTrack>,
}

fn extract_next_data(html: &str) -> Result<serde_json::Value> {
    // the embed page stores its state in a json script tag
    let start_marker = r#"<script id="__NEXT_DATA__" type="application/json">"#;
    let start = html
        .find(start_marker)
        .ok_or_else(|| anyhow!(fehler!("Spotify hat die Seite unerwartet aufgebaut")))?
        + start_marker.len();
    let end = html[start..]
        .find("</script>")
        .ok_or_else(|| anyhow!(fehler!("Spotify hat die Seite unerwartet aufgebaut")))?;
    Ok(serde_json::from_str(&html[start..start + end])?)
}

/// splits artist names the way spotify joins them, at a comma followed by a
/// non-breaking space. an ordinary comma belongs to the name ("Earth, Wind &
/// Fire").
pub fn split_spotify_artists(value: &str) -> Vec<String> {
    value
        .split(",\u{a0}")
        .flat_map(|part| part.split(", \u{a0}"))
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect()
}

/// pulls "(feat. A, B & C)" out of the title and returns title and names
pub fn split_feature_suffix(title: &str) -> (String, Vec<String>) {
    const MARKERS: [&str; 4] = ["(feat. ", "(ft. ", "(featuring ", "(with "];
    let lower = title.to_lowercase();

    for marker in MARKERS {
        let Some(start) = lower.find(marker) else {
            continue;
        };
        let after = start + marker.len();
        let Some(offset) = title[after..].find(')') else {
            continue;
        };

        let names: Vec<String> = title[after..after + offset]
            .split([',', '&'])
            .flat_map(|part| part.split(" and "))
            .map(str::trim)
            .filter(|part| !part.is_empty())
            .map(str::to_string)
            .collect();

        let cleaned = format!("{}{}", &title[..start], &title[after + offset + 1..]);
        return (cleaned.split_whitespace().collect::<Vec<_>>().join(" "), names);
    }
    (title.to_string(), Vec::new())
}

fn text(value: &serde_json::Value) -> Option<String> {
    value
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

// joins artist names, spotify delivers them as a list
fn join_artists(entity: &serde_json::Value) -> Option<String> {
    let names: Vec<String> = entity["artists"]
        .as_array()?
        .iter()
        .filter_map(|artist| text(&artist["name"]))
        .collect();
    (!names.is_empty()).then(|| names.join(", "))
}

async fn fetch_cover_url(reference: &SpotifyRef) -> Option<String> {
    let url = format!(
        "https://open.spotify.com/oembed?url=https://open.spotify.com/{}/{}",
        reference.kind.as_str(),
        reference.id
    );
    let response = online::client().get(url).send().await.ok()?;
    let json: serde_json::Value = response.json().await.ok()?;
    text(&json["thumbnail_url"])
}

/// fetches the cover address of a single track.
///
/// playlists hand out no image in their track list, only the id. without this
/// second reach every track of a playlist would carry that playlist's image
/// although the pieces come from entirely different releases.
pub async fn track_cover_url(track_id: &str) -> Option<String> {
    let url = format!(
        "https://open.spotify.com/oembed?url=https://open.spotify.com/track/{track_id}"
    );
    let response = online::client().get(url).send().await.ok()?;
    let json: serde_json::Value = response.json().await.ok()?;
    text(&json["thumbnail_url"])
}

/// reads title, artists, cover and track list off the embed page
pub async fn resolve(reference: &SpotifyRef) -> Result<SpotifyRelease> {
    let url = format!(
        "https://open.spotify.com/embed/{}/{}",
        reference.kind.as_str(),
        reference.id
    );
    let response = online::client().get(&url).send().await?;
    if !response.status().is_success() {
        bail!(
            "Spotify-Link konnte nicht gelesen werden (HTTP {}). \
             Ist der Inhalt öffentlich?",
            response.status().as_u16()
        );
    }

    let entity = {
        let html = response.text().await?;
        let data = extract_next_data(&html)?;
        data["props"]["pageProps"]["state"]["data"]["entity"].clone()
    };
    if entity.is_null() {
        bail!(fehler!("Zu diesem Spotify-Link gibt es keine öffentlichen Daten."));
    }

    let name = text(&entity["name"])
        .or_else(|| text(&entity["title"]))
        .ok_or_else(|| anyhow!(fehler!("Spotify hat keinen Namen geliefert")))?;
    let year = text(&entity["releaseDate"]["isoString"])
        .and_then(|iso| iso.get(0..4).and_then(|y| y.parse().ok()));
    let cover_url = fetch_cover_url(reference).await;

    // a single track: the data sits in the entity directly
    if reference.kind == SpotifyKind::Track {
        let mut artists: Vec<String> = entity["artists"]
            .as_array()
            .map(|list| list.iter().filter_map(|a| text(&a["name"])).collect())
            .unwrap_or_default();
        if artists.is_empty() {
            artists = text(&entity["subtitle"])
                .map(|s| split_spotify_artists(&s))
                .unwrap_or_default();
        }
        if artists.is_empty() {
            artists.push("Unbekannter Künstler".into());
        }

        let (title, featured) = split_feature_suffix(&name);
        return Ok(SpotifyRelease {
            kind: reference.kind,
            name: title.clone(),
            artist: Some(artists[0].clone()),
            cover_url,
            year,
            tracks: vec![SpotifyTrack {
                id: Some(reference.id.clone()),
                title,
                artists,
                featured,
                duration_ms: entity["duration"].as_i64(),
                track_no: None,
            }],
        });
    }

    // album, playlist and artist bring a track list along
    let release_artist = join_artists(&entity).or_else(|| text(&entity["subtitle"]));
    let numbered = reference.kind == SpotifyKind::Album;

    let tracks: Vec<SpotifyTrack> = entity["trackList"]
        .as_array()
        .map(|list| {
            list.iter()
                .enumerate()
                .filter_map(|(index, item)| {
                    let raw_title = text(&item["title"])?;
                    let mut artists = text(&item["subtitle"])
                        .map(|s| split_spotify_artists(&s))
                        .unwrap_or_default();
                    if artists.is_empty() {
                        artists = release_artist
                            .clone()
                            .map(|a| vec![a])
                            .unwrap_or_else(|| vec!["Unbekannter Künstler".into()]);
                    }

                    let (title, featured) = split_feature_suffix(&raw_title);
                    Some(SpotifyTrack {
                        // "spotify:track:ID", only the last part counts
                        id: text(&item["uri"])
                            .and_then(|uri| uri.rsplit(':').next().map(str::to_owned)),
                        title,
                        artists,
                        featured,
                        duration_ms: item["duration"].as_i64(),
                        // playlists carry no meaningful track number
                        track_no: numbered.then_some(index as i64 + 1),
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    if tracks.is_empty() {
        bail!(fehler!("In diesem Spotify-{0} sind keine Titel sichtbar.", reference.kind.label()));
    }

    Ok(SpotifyRelease {
        kind: reference.kind,
        name,
        artist: release_artist,
        cover_url,
        year,
        tracks,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn erkennt_die_ueblichen_linkformen() {
        let cases = [
            "https://open.spotify.com/track/4cOdK2wGLETKBW3PvgPWqT",
            "https://open.spotify.com/track/4cOdK2wGLETKBW3PvgPWqT?si=abc123",
            "https://open.spotify.com/intl-de/track/4cOdK2wGLETKBW3PvgPWqT",
            "spotify:track:4cOdK2wGLETKBW3PvgPWqT",
            "  https://open.spotify.com/track/4cOdK2wGLETKBW3PvgPWqT/  ",
        ];
        for case in cases {
            let parsed = parse_link(case).unwrap_or_else(|| panic!("nicht erkannt: {case}"));
            assert_eq!(parsed.kind, SpotifyKind::Track);
            assert_eq!(parsed.id, "4cOdK2wGLETKBW3PvgPWqT");
        }
    }

    #[test]
    fn erkennt_alben_playlists_und_kuenstler() {
        assert_eq!(
            parse_link("https://open.spotify.com/album/abc").unwrap().kind,
            SpotifyKind::Album
        );
        assert_eq!(
            parse_link("https://open.spotify.com/playlist/xyz").unwrap().kind,
            SpotifyKind::Playlist
        );
        assert_eq!(
            parse_link("spotify:artist:qqq").unwrap().kind,
            SpotifyKind::Artist
        );
    }

    #[test]
    fn trennt_nur_an_spotifys_eigenem_trennzeichen() {
        // spotify joins with comma plus non-breaking space
        assert_eq!(
            split_spotify_artists("Kanye West,\u{a0}JAŸ-Z,\u{a0}Bon Iver"),
            vec!["Kanye West", "JAŸ-Z", "Bon Iver"]
        );
        // an ordinary comma belongs to the name
        assert_eq!(
            split_spotify_artists("Earth, Wind & Fire"),
            vec!["Earth, Wind & Fire"]
        );
        assert_eq!(split_spotify_artists("Adele"), vec!["Adele"]);
    }

    #[test]
    fn zieht_gastkuenstler_aus_dem_titel() {
        let (title, featured) = split_feature_suffix("Sunflower (feat. Swae Lee)");
        assert_eq!(title, "Sunflower");
        assert_eq!(featured, vec!["Swae Lee"]);

        let (title, featured) = split_feature_suffix("Monster (feat. Bon Iver, JAY-Z & Nicki Minaj)");
        assert_eq!(title, "Monster");
        assert_eq!(featured, vec!["Bon Iver", "JAY-Z", "Nicki Minaj"]);

        // without a suffix the title stays unchanged
        let (title, featured) = split_feature_suffix("Dark Fantasy");
        assert_eq!(title, "Dark Fantasy");
        assert!(featured.is_empty());

        // brackets without a feature marker stay
        let (title, featured) = split_feature_suffix("All Of The Lights (Interlude)");
        assert_eq!(title, "All Of The Lights (Interlude)");
        assert!(featured.is_empty());
    }

    #[test]
    fn ignoriert_fremde_links() {
        assert!(parse_link("https://www.youtube.com/watch?v=abc").is_none());
        assert!(parse_link("https://soundcloud.com/artist/track").is_none());
        assert!(parse_link("einfach nur ein suchbegriff").is_none());
        assert!(parse_link("https://open.spotify.com/track/").is_none());
    }
}
