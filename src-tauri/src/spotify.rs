//! Spotify-Links auflösen.
//!
//! Spotify liefert seine Audiodaten ausschließlich verschlüsselt (DRM) aus,
//! herunterladen lässt sich davon nichts. Was öffentlich zugänglich ist, sind
//! die **Metadaten**: Titel, Künstler, Album, Länge und Cover. Genau die holen
//! wir hier über die Embed-Seite (kein API-Schlüssel nötig) und suchen die
//! passende Aufnahme anschließend über die normalen Quellen.

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

/// Erkennt `https://open.spotify.com/track/…`, Länderpfade wie `/intl-de/`
/// und `spotify:track:…`.
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

    // Query und Fragment abschneiden, dann den Pfad durchgehen.
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
    /// Kennung des Titels bei Spotify, falls bekannt. Damit lässt sich sein
    /// eigenes Cover nachschlagen, das die Titelliste nicht mitliefert.
    pub id: Option<String>,
    pub title: String,
    /// Alle Beteiligten, Hauptkünstler zuerst. Spotify unterscheidet keine
    /// Gastbeiträge, wer dort steht, steht gleichberechtigt in der Liste.
    pub artists: Vec<String>,
    /// Aus dem Titel gezogene Gastkünstler („… (feat. X)“).
    pub featured: Vec<String>,
    pub duration_ms: Option<i64>,
    pub track_no: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpotifyRelease {
    pub kind: SpotifyKind,
    /// Name des Albums, der Playlist oder des Titels.
    pub name: String,
    pub artist: Option<String>,
    pub cover_url: Option<String>,
    pub year: Option<i64>,
    pub tracks: Vec<SpotifyTrack>,
}

fn extract_next_data(html: &str) -> Result<serde_json::Value> {
    // Die Embed-Seite legt ihren Zustand in einem JSON-Script-Tag ab.
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

/// Spotify verbindet Künstlernamen mit Komma und geschütztem Leerzeichen.
/// Genau daran wird getrennt, ein gewöhnliches Komma gehört zum Namen
/// („Earth, Wind & Fire“).
pub fn split_spotify_artists(value: &str) -> Vec<String> {
    value
        .split(",\u{a0}")
        .flat_map(|part| part.split(", \u{a0}"))
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect()
}

/// Zieht „(feat. A, B & C)“ aus dem Titel und gibt Titel und Namen zurück.
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

/// Künstlernamen zusammenfassen. Spotify liefert sie als Liste.
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

/// Holt die Cover-Adresse eines einzelnen Titels.
///
/// Playlists geben in ihrer Titelliste kein Bild heraus, nur die Kennung.
/// Ohne diesen zweiten Griff trüge jeder Titel einer Playlist deren Bild,
/// obwohl die Stücke aus ganz verschiedenen Releases stammen.
pub async fn track_cover_url(track_id: &str) -> Option<String> {
    let url = format!(
        "https://open.spotify.com/oembed?url=https://open.spotify.com/track/{track_id}"
    );
    let response = online::client().get(url).send().await.ok()?;
    let json: serde_json::Value = response.json().await.ok()?;
    text(&json["thumbnail_url"])
}

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

    // Einzelner Titel: die Daten stehen direkt in der Entität.
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

    // Album, Playlist und Künstler bringen eine Titelliste mit.
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
                        // „spotify:track:ID“, nur der letzte Teil zählt.
                        id: text(&item["uri"])
                            .and_then(|uri| uri.rsplit(':').next().map(str::to_owned)),
                        title,
                        artists,
                        featured,
                        duration_ms: item["duration"].as_i64(),
                        // Playlists haben keine sinnvolle Titelnummer.
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
        // Spotify verbindet mit Komma + geschütztem Leerzeichen.
        assert_eq!(
            split_spotify_artists("Kanye West,\u{a0}JAŸ-Z,\u{a0}Bon Iver"),
            vec!["Kanye West", "JAŸ-Z", "Bon Iver"]
        );
        // Ein gewöhnliches Komma gehört zum Namen.
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

        // Ohne Zusatz bleibt der Titel unverändert.
        let (title, featured) = split_feature_suffix("Dark Fantasy");
        assert_eq!(title, "Dark Fantasy");
        assert!(featured.is_empty());

        // Klammern ohne Feature-Marker bleiben stehen.
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
