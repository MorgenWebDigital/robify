//! Alle vom Frontend aufrufbaren Befehle.

use crate::downloader::{
    self, DownloadOptions, DownloadOutcome, LinkPlan, SearchSource,
};
use crate::spotify;
use crate::library;
use crate::models::*;
use crate::online::{self, LyricsCandidate, MetadataCandidate};
use crate::player::{Cmd, PlayerState, RepeatMode, SleepTimerMode};
use crate::scanner::{self, ScanResult};
use crate::state::{AppState, CmdResult, Error};
use crate::stats::{self, WeeklyMix, Wrapped};
use crate::{db, tags};
use base64::Engine;
use serde::Serialize;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter, Manager, State};
use crate::fehler;

fn b64(data: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(data)
}

// ------------------------------------------------------------- Bibliothek

#[tauri::command]
pub fn library_stats(state: State<'_, AppState>) -> CmdResult<LibraryStats> {
    let conn = state.db.lock();
    Ok(library::library_stats(&conn)?)
}

#[tauri::command]
pub fn scan_folders(
    app: AppHandle,
    state: State<'_, AppState>,
    paths: Vec<String>,
) -> CmdResult<ScanResult> {
    let roots: Vec<PathBuf> = paths.into_iter().map(PathBuf::from).collect();
    let result = scanner::scan(&app, &state.db, roots)?;

    // Ein Scan bringt oft viele Künstler auf einmal mit. Die Obergrenze
    // verhindert, dass eine große Bibliothek Hunderte Abfragen auslöst,
    // der Rest lässt sich weiterhin einzeln nachholen.
    let offen = {
        let conn = state.db.lock();
        if auto_fetch_artists(&conn) {
            library::all_artists_missing_metadata(&conn, 25).unwrap_or_default()
        } else {
            Vec::new()
        }
    };
    fetch_artists_in_background(&app, offen);

    // Fehlendes nachschlagen: unsichere Angaben, Cover, Lyrics. Obergrenze
    // wie oben, damit eine große Sammlung nicht Hunderte Abfragen auslöst.
    let offene_titel = {
        let conn = state.db.lock();
        if auto_fetch_import(&conn) {
            library::tracks_needing_lookup(&conn, 40).unwrap_or_default()
        } else {
            Vec::new()
        }
    };
    enrich_tracks_in_background(&app, offene_titel);

    Ok(result)
}

/// Ergebnis des Abgleichs zwischen Ordner und Datenbank.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryCheck {
    /// Audiodateien im Bibliotheksordner, die keinem Titel zugeordnet sind.
    pub orphan_count: usize,
    pub orphan_samples: Vec<String>,
    /// Titel, deren Datei nicht mehr existiert.
    pub missing_count: usize,
    pub missing_samples: Vec<String>,
}

/// Vergleicht den Bibliotheksordner mit der Datenbank.
///
/// Beides läuft im Alltag auseinander: Dateien werden außerhalb der App
/// verschoben, Importe brechen ab. Beim Nutzer lagen 19 Dateien im Ordner,
/// von denen die Datenbank 5 kannte. Dieser Befehl berichtet nur, gelöscht
/// oder eingelesen wird erst auf Bestätigung.
#[tauri::command]
pub fn check_library(state: State<'_, AppState>) -> CmdResult<LibraryCheck> {
    let conn = state.db.lock();
    let bekannt = library::known_paths(&conn)?;
    let fehlend = library::tracks_without_file(&conn)?;
    drop(conn);

    let verwaist: Vec<String> = scanner::collect_audio_files(&[state.library_dir()])
        .into_iter()
        .map(|pfad| pfad.to_string_lossy().to_string())
        .filter(|pfad| !bekannt.contains(pfad))
        .collect();

    let kurz = |pfade: &[String]| -> Vec<String> {
        pfade
            .iter()
            .take(5)
            .map(|pfad| {
                Path::new(pfad)
                    .file_name()
                    .map(|name| name.to_string_lossy().to_string())
                    .unwrap_or_else(|| pfad.clone())
            })
            .collect()
    };

    let fehlende_pfade: Vec<String> = fehlend.iter().map(|(_, pfad)| pfad.clone()).collect();
    Ok(LibraryCheck {
        orphan_count: verwaist.len(),
        orphan_samples: kurz(&verwaist),
        missing_count: fehlend.len(),
        missing_samples: kurz(&fehlende_pfade),
    })
}

/// Entfernt Titel, deren Datei verschwunden ist.
///
/// Zurück kommen die Kennungen, nicht bloß ihre Anzahl: Gelöscht wird weich,
/// also lässt sich der Griff zurücknehmen, aber nur, wenn die Oberfläche
/// weiß, welche Einträge sie wiederholen soll.
#[tauri::command]
pub fn remove_missing_tracks(app: AppHandle, state: State<'_, AppState>) -> CmdResult<Vec<i64>> {
    let conn = state.db.lock();
    let fehlend = library::tracks_without_file(&conn)?;
    for (id, _) in &fehlend {
        library::delete_track(&conn, *id, false, None)?;
    }
    drop(conn);

    if !fehlend.is_empty() {
        let _ = app.emit("library:changed", ());
    }
    Ok(fehlend.into_iter().map(|(id, _)| id).collect())
}

#[tauri::command]
pub fn list_tracks(
    state: State<'_, AppState>,
    search: Option<String>,
    limit: Option<i64>,
) -> CmdResult<Vec<Track>> {
    let conn = state.db.lock();
    Ok(library::list_tracks(
        &conn,
        search.as_deref(),
        limit.unwrap_or(2000),
    )?)
}

#[tauri::command]
pub fn get_track(state: State<'_, AppState>, id: i64) -> CmdResult<Track> {
    let conn = state.db.lock();
    Ok(library::get_track(&conn, id)?)
}

#[tauri::command]
pub fn get_tracks(state: State<'_, AppState>, ids: Vec<i64>) -> CmdResult<Vec<Track>> {
    let conn = state.db.lock();
    Ok(library::get_tracks(&conn, &ids)?)
}

#[tauri::command]
pub fn list_artists(state: State<'_, AppState>, search: Option<String>) -> CmdResult<Vec<Artist>> {
    let conn = state.db.lock();
    Ok(library::list_artists(&conn, search.as_deref())?)
}

#[tauri::command]
pub fn get_artist(state: State<'_, AppState>, id: i64) -> CmdResult<Artist> {
    let conn = state.db.lock();
    Ok(library::get_artist(&conn, id)?)
}

/// Sucht online nach Angaben zu einem Künstler.
#[tauri::command]
pub async fn search_artists_online(name: String) -> CmdResult<Vec<online::ArtistCandidate>> {
    Ok(online::search_artists(&name).await?)
}

/// Übernimmt einen Vorschlag: Bild wird geladen, Beschreibung gespeichert.
#[tauri::command]
pub async fn apply_artist_metadata(
    app: AppHandle,
    state: State<'_, AppState>,
    artist_id: i64,
    candidate: online::ArtistCandidate,
) -> CmdResult<Artist> {
    let image = match candidate.image_url.as_deref() {
        Some(url) => online::fetch_image(url).await.ok(),
        None => None,
    };

    let artist = {
        let conn = state.db.lock();
        let current = library::get_artist(&conn, artist_id)?;
        // Der Name bleibt, wie er in der Bibliothek steht, nur die
        // Zusatzangaben kommen dazu.
        library::update_artist(
            &conn,
            artist_id,
            &current.name,
            candidate.bio.as_deref(),
            candidate.url.as_deref(),
        )?;
        if let Some((data, mime)) = image {
            library::set_artist_image(&conn, artist_id, &data, &mime)?;
        }
        library::get_artist(&conn, artist_id)?
    };

    let _ = app.emit("library:changed", ());
    Ok(artist)
}

/// Sucht den Treffer, der am ehesten der gesuchte Künstler ist.
///
/// Zwei Fallstricke sind hier eingebaut, beide aus der Praxis:
///
/// * **Kein Notnagel.** Zu „Julia“ führt Genius „Julia Michaels“ und
///   „Julian Casablancas“, aber keine „Julia“. Früher wurde einfach der
///   erste Treffer übernommen, und im Profil stand ein fremdes Gesicht.
///   Passt der Name nicht, gibt es lieber gar nichts.
/// * **Namensgleichheit.** Bleiben mehrere Treffer übrig, entscheidet das
///   Werk: Wer einen Titel aus der eigenen Bibliothek führt, ist gemeint.
///   `known_titles` bleibt leer, wenn nichts zum Vergleichen da ist, dann
///   zählt nur ein eindeutiger Name.
pub async fn best_artist_match(
    name: &str,
    known_titles: &[String],
) -> Option<online::ArtistCandidate> {
    let candidates = online::search_artists(name).await.ok()?;
    let passend: Vec<online::ArtistCandidate> = candidates
        .into_iter()
        .filter(|candidate| online::looks_like_same(&candidate.name, name))
        .collect();

    if passend.is_empty() {
        return None;
    }

    // Ohne eigene Titel bleibt nur der Name, dann muss er wenigstens
    // eindeutig sein.
    if known_titles.is_empty() {
        return (passend.len() == 1).then(|| passend[0].clone());
    }

    // Sonst entscheidet das Werk, und zwar immer: Auch ein einzelner Treffer
    // kann der Falsche sein. Ein Künstler aus Kansas heißt mitunter genauso
    // wie eine deutsche Rapgruppe, und wenn nur er bei Genius steht, bliebe
    // er ungeprüft übrig.
    for candidate in &passend {
        let Some(id) = candidate.genius_id else {
            continue;
        };
        let songs = online::artist_songs(id, 50).await;
        let bekannt = songs.iter().any(|song| {
            known_titles
                .iter()
                .any(|eigener| online::looks_like_same(song, eigener))
        });
        if bekannt {
            return Some(candidate.clone());
        }
    }

    // Kein Werk belegt die Zuordnung, lieber kein Bild als ein fremdes.
    None
}

/// Ein Klick: bester Treffer wird gesucht und gleich übernommen.
#[tauri::command]
pub async fn fetch_artist_metadata(
    app: AppHandle,
    state: State<'_, AppState>,
    artist_id: i64,
) -> CmdResult<Artist> {
    let (name, titel) = {
        let conn = state.db.lock();
        let name = library::get_artist(&conn, artist_id)?.name;
        let titel: Vec<String> = library::artist_tracks(&conn, artist_id)
            .unwrap_or_default()
            .into_iter()
            .map(|track| track.title)
            .take(20)
            .collect();
        (name, titel)
    };

    let best = best_artist_match(&name, &titel).await.ok_or_else(|| {
        Error(format!(
            "Zu „{name}“ ließ sich nichts eindeutig zuordnen: Keiner der \
             Vorschläge führt einen deiner Titel. Namensgleiche Künstler gibt \
             es häufig, wähle im Editor selbst aus."
        ))
    })?;

    apply_artist_metadata(app, state, artist_id, best).await
}

/// Lädt fehlende Künstlerangaben im Hintergrund nach.
///
/// Beim ersten Titel eines Künstlers steht sonst nur der Name in der
/// Bibliothek. Der Import wartet nicht darauf: er meldet sich fertig, und die
/// Oberfläche bekommt später ein `library:changed`, sobald etwas ankam.
///
/// Der Reihe nach, nicht gleichzeitig, bei einem Album mit vielen
/// Gastkünstlern wären das sonst zwanzig Abfragen auf einen Schlag.
/// Ergänzt fehlende Angaben zu importierten Titeln.
///
/// Zwei Fälle in einem Durchlauf:
/// * **Falsches richtigstellen**, sieht der Titel nach einem Dateinamen aus
///   oder fehlt der Künstler, wird der Suchbegriff aus dem Dateinamen gebaut
///   und alles überschrieben, was die Quelle sauber getrennt hergibt.
/// * **Lücken füllen**, bei sauber getaggten Dateien bleiben Titel und
///   Künstler stehen, geholt werden nur Cover, Lyrics, Jahr und Genre.
///
/// Übernommen wird nur, was eindeutig passt: `online::auto_match` verlangt
/// Übereinstimmung in Titel **und** Künstler. Bei Zweifeln bleibt die Datei,
/// wie sie ist, ein falscher Künstler wäre schlimmer als ein fehlender.
fn enrich_tracks_in_background(app: &AppHandle, tracks: Vec<crate::models::Track>) {
    if tracks.is_empty() {
        return;
    }

    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let mut geaendert = 0usize;

        for track in tracks {
            let unsicher = library::weak_metadata(&track);
            let will_cover = !track.has_cover;
            let will_lyrics = {
                let state = app.state::<AppState>();
                let conn = state.db.lock();
                !track.has_lyrics && auto_fetch_lyrics(&conn)
            };
            if !unsicher && !will_cover && !will_lyrics {
                continue;
            }

            // Aus dem, was da ist, den bestmöglichen Anhaltspunkt bauen. Bei
            // unsicheren Angaben zerlegt `split_video_title` den Dateinamen in
            // Künstler und Titel und wirft Zusätze wie „(Official Video)“ weg.
            let ohne_kuenstler = track.artist_name == "Unbekannter Künstler";
            let (kuenstler, titel) = if unsicher {
                match library::split_video_title(
                    &track.title,
                    (!ohne_kuenstler).then_some(track.artist_name.as_str()),
                ) {
                    Some((k, t)) => (k, t),
                    // Ohne Künstler und ohne Trennzeichen gibt es nichts zu suchen.
                    None if ohne_kuenstler => continue,
                    None => (track.artist_name.clone(), track.title.clone()),
                }
            } else {
                (track.artist_name.clone(), track.title.clone())
            };

            let vorlage = TrackMetadata {
                title: titel,
                artist: kuenstler,
                featured_artists: None,
                album: if unsicher { String::new() } else { track.album_title.clone() },
                album_artist: None,
                release_type: None,
                year: track.year,
                track_no: track.track_no,
                disc_no: track.disc_no,
                genre: track.genre.clone(),
                cover_base64: None,
                cover_mime: None,
                lyrics_synced: None,
                lyrics_plain: None,
            };

            let Some(gefunden) =
                online::auto_match(&vorlage, Some(track.duration_ms), will_cover, will_lyrics)
                    .await
            else {
                continue;
            };

            // Erst nach der Abfrage an die Datenbank, damit die Sperre nicht
            // über das Netz gehalten wird.
            let zusammengefuehrt = online::merge_match(vorlage, gefunden);
            {
                let state = app.state::<AppState>();
                let conn = state.db.lock();
                if apply_track_metadata(&conn, track.id, &zusammengefuehrt).is_ok() {
                    geaendert += 1;
                }
            }
            // Die Datei bekommt die Angaben ebenfalls, sonst wären sie nach
            // einem erneuten Einlesen wieder weg.
            let _ = tags::write(Path::new(&track.path), &zusammengefuehrt);
        }

        if geaendert > 0 {
            let _ = app.emit("library:changed", ());
        }
    });
}

fn fetch_artists_in_background(app: &AppHandle, artists: Vec<(i64, String, Vec<String>)>) {
    if artists.is_empty() {
        return;
    }

    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let mut changed = false;

        for (artist_id, name, titel) in artists {
            // Nicht irgendwen übernehmen: automatisch zählt nur, was
            // eindeutig passt. Für alles andere bleibt der Knopf.
            let Some(candidate) = best_artist_match(&name, &titel).await else {
                continue;
            };

            let image = match candidate.image_url.as_deref() {
                Some(url) => online::fetch_image(url).await.ok(),
                None => None,
            };

            // Erst nach allen Abfragen an die Datenbank, damit die Sperre
            // nicht über das Netz gehalten wird.
            let state = app.state::<AppState>();
            let conn = state.db.lock();
            let written = library::update_artist(
                &conn,
                artist_id,
                &name,
                candidate.bio.as_deref(),
                candidate.url.as_deref(),
            )
            .is_ok();
            if let Some((data, mime)) = image {
                let _ = library::set_artist_image(&conn, artist_id, &data, &mime);
            }
            drop(conn);

            changed |= written;
        }

        if changed {
            let _ = app.emit("library:changed", ());
        }
    });
}

/// Stammdaten von Hand ändern. Name, Beschreibung und Bild.
#[tauri::command]
pub fn update_artist(
    app: AppHandle,
    state: State<'_, AppState>,
    artist_id: i64,
    name: String,
    bio: Option<String>,
    image_base64: Option<String>,
    image_mime: Option<String>,
    remove_image: bool,
) -> CmdResult<Artist> {
    let conn = state.db.lock();
    let current = library::get_artist(&conn, artist_id)?;
    library::update_artist(
        &conn,
        artist_id,
        &name,
        bio.as_deref(),
        current.source_url.as_deref(),
    )?;

    if remove_image {
        conn.execute(
            "UPDATE artists SET image = NULL, image_mime = NULL WHERE id = ?1",
            [artist_id],
        )?;
    } else if let Some(encoded) = image_base64.as_deref().filter(|value| !value.is_empty()) {
        let data = base64::engine::general_purpose::STANDARD.decode(encoded)?;
        let mime = image_mime.as_deref().unwrap_or("image/jpeg");
        library::set_artist_image(&conn, artist_id, &data, mime)?;
    }

    let artist = library::get_artist(&conn, artist_id)?;
    drop(conn);
    let _ = app.emit("library:changed", ());
    Ok(artist)
}

/// Titel, Jahr, Einordnung und Cover eines Releases ändern.
#[tauri::command]
pub fn update_album(
    app: AppHandle,
    state: State<'_, AppState>,
    album_id: i64,
    title: String,
    year: Option<i64>,
    release_type: String,
    cover_base64: Option<String>,
    cover_mime: Option<String>,
    remove_cover: bool,
) -> CmdResult<Album> {
    let conn = state.db.lock();
    library::update_album(
        &conn,
        album_id,
        &title,
        year,
        ReleaseType::parse(&release_type),
    )?;

    if remove_cover {
        conn.execute(
            "UPDATE albums SET cover = NULL, cover_mime = NULL WHERE id = ?1",
            [album_id],
        )?;
    } else if let Some(encoded) = cover_base64.as_deref().filter(|value| !value.is_empty()) {
        let data = base64::engine::general_purpose::STANDARD.decode(encoded)?;
        let mime = cover_mime.as_deref().unwrap_or("image/jpeg");
        library::set_album_cover(&conn, album_id, &data, mime)?;
    }

    let album = library::get_album(&conn, album_id)?;
    drop(conn);
    let _ = app.emit("library:changed", ());
    Ok(album)
}

#[tauri::command]
pub fn artist_releases(state: State<'_, AppState>, artist_id: i64) -> CmdResult<Vec<Album>> {
    let conn = state.db.lock();
    Ok(library::artist_releases(&conn, artist_id)?)
}

#[tauri::command]
pub fn artist_tracks(state: State<'_, AppState>, artist_id: i64) -> CmdResult<Vec<Track>> {
    let conn = state.db.lock();
    Ok(library::artist_tracks(&conn, artist_id)?)
}

/// Titel, bei denen der Künstler nur als Gast auftritt.
#[tauri::command]
pub fn artist_features(state: State<'_, AppState>, artist_id: i64) -> CmdResult<Vec<Track>> {
    let conn = state.db.lock();
    Ok(library::artist_features(&conn, artist_id)?)
}

#[tauri::command]
pub fn list_albums(state: State<'_, AppState>, search: Option<String>) -> CmdResult<Vec<Album>> {
    let conn = state.db.lock();
    Ok(library::list_albums(&conn, search.as_deref())?)
}

#[tauri::command]
pub fn get_album(state: State<'_, AppState>, id: i64) -> CmdResult<Album> {
    let conn = state.db.lock();
    Ok(library::get_album(&conn, id)?)
}

#[tauri::command]
pub fn album_tracks(state: State<'_, AppState>, album_id: i64) -> CmdResult<Vec<Track>> {
    let conn = state.db.lock();
    Ok(library::album_tracks(&conn, album_id)?)
}

#[tauri::command]
pub fn favorite_tracks(state: State<'_, AppState>) -> CmdResult<Vec<Track>> {
    let conn = state.db.lock();
    Ok(library::favorite_tracks(&conn)?)
}

#[tauri::command]
pub fn set_favorite(state: State<'_, AppState>, track_id: i64, favorite: bool) -> CmdResult<()> {
    let conn = state.db.lock();
    Ok(library::set_favorite(&conn, track_id, favorite)?)
}

#[tauri::command]
pub fn delete_track(
    app: AppHandle,
    state: State<'_, AppState>,
    track_id: i64,
    delete_file: bool,
) -> CmdResult<()> {
    let papierkorb = state.trash_dir();
    {
        let conn = state.db.lock();
        library::delete_track(&conn, track_id, delete_file, Some(&papierkorb))?;
    }
    let _ = app.emit("library:changed", ());
    Ok(())
}

/// Nimmt das Entfernen eines Titels zurück.
#[tauri::command]
pub fn restore_track(
    app: AppHandle,
    state: State<'_, AppState>,
    track_id: i64,
) -> CmdResult<()> {
    let papierkorb = state.trash_dir();
    {
        let conn = state.db.lock();
        library::restore_track(&conn, track_id, Some(&papierkorb))?;
    }
    let _ = app.emit("library:changed", ());
    Ok(())
}

/// Nimmt das Entfernen einer Playlist zurück.
#[tauri::command]
pub fn restore_playlist(state: State<'_, AppState>, id: i64) -> CmdResult<Playlist> {
    let conn = state.db.lock();
    library::restore_playlist(&conn, id)?;
    Ok(library::get_playlist(&conn, id)?)
}

// -------------------------------------------------------------- Metadaten

#[tauri::command]
pub fn get_track_metadata(state: State<'_, AppState>, track_id: i64) -> CmdResult<TrackMetadata> {
    let conn = state.db.lock();
    let track = library::get_track(&conn, track_id)?;
    let lyrics = library::get_lyrics(&conn, track_id)?;
    let cover = library::track_cover(&conn, track_id)?;
    drop(conn);

    let main: Vec<String> = track
        .artists
        .iter()
        .filter(|a| a.role == "main")
        .map(|a| a.name.clone())
        .collect();
    let featured: Vec<String> = track
        .artists
        .iter()
        .filter(|a| a.role == "feature")
        .map(|a| a.name.clone())
        .collect();

    Ok(TrackMetadata {
        title: track.title,
        artist: if main.is_empty() {
            track.artist_name
        } else {
            library::join_artists(&main)
        },
        featured_artists: (!featured.is_empty()).then(|| library::join_artists(&featured)),
        album: track.album_title,
        album_artist: None,
        release_type: Some(track.release_type.as_str().to_string()),
        year: track.year,
        track_no: track.track_no,
        disc_no: track.disc_no,
        genre: track.genre,
        cover_base64: cover.as_ref().map(|(d, _)| b64(d)),
        cover_mime: cover.map(|(_, m)| m),
        lyrics_synced: lyrics.as_ref().and_then(|l| l.synced.clone()),
        lyrics_plain: lyrics.and_then(|l| l.plain),
    })
}

/// Übernimmt bearbeitete Metadaten in Datei **und** Bibliothek.
#[tauri::command]
pub fn update_track_metadata(
    app: AppHandle,
    state: State<'_, AppState>,
    track_id: i64,
    metadata: TrackMetadata,
    write_to_file: bool,
) -> CmdResult<Track> {
    let path = {
        let conn = state.db.lock();
        library::get_track(&conn, track_id)?.path
    };

    if write_to_file {
        tags::write(Path::new(&path), &metadata)?;
    }

    let conn = state.db.lock();
    let track = apply_track_metadata(&conn, track_id, &metadata)?;
    drop(conn);

    let _ = app.emit("library:changed", ());
    Ok(track)
}

/// Schreibt Metadaten in die Datenbank: Künstler, Album, Cover, Lyrics.
///
/// Als eigene Funktion, weil zwei Wege hier hineinlaufen, der Bearbeiten-
/// Dialog und das automatische Nachschlagen beim Import.
fn apply_track_metadata(
    conn: &rusqlite::Connection,
    track_id: i64,
    metadata: &TrackMetadata,
) -> CmdResult<Track> {
    let (mut main, mut featured) = library::parse_artist_field(&metadata.artist);
    if main.is_empty() {
        main.push("Unbekannter Künstler".into());
    }
    for name in metadata
        .featured_artists
        .as_deref()
        .map(library::split_artists)
        .unwrap_or_default()
    {
        if !featured.contains(&name) {
            featured.push(name);
        }
    }
    featured.retain(|name| !main.contains(name));

    let artist_id = library::set_track_artists(conn, track_id, &main, &featured)?;
    let album_artist_id = match metadata.album_artist.as_deref() {
        Some(a) if !a.trim().is_empty() => library::upsert_artist(conn, a)?,
        _ => artist_id,
    };
    let release_type = metadata.release_type.as_deref().map(ReleaseType::parse);
    let album_title = if metadata.album.trim().is_empty() {
        metadata.title.clone()
    } else {
        metadata.album.clone()
    };
    let album_id = library::upsert_album(
        conn,
        album_artist_id,
        &album_title,
        metadata.year,
        release_type,
    )?;

    conn.execute(
        "UPDATE tracks SET title = ?2, artist_id = ?3, album_id = ?4, track_no = ?5,
                disc_no = ?6, genre = ?7, year = ?8 WHERE id = ?1",
        rusqlite::params![
            track_id,
            metadata.title,
            artist_id,
            album_id,
            metadata.track_no,
            metadata.disc_no,
            metadata.genre,
            metadata.year
        ],
    )?;

    if let Some(cover) = metadata.cover_base64.as_deref().filter(|c| !c.is_empty()) {
        let data = base64::engine::general_purpose::STANDARD.decode(cover)?;
        let mime = metadata.cover_mime.as_deref().unwrap_or("image/jpeg");
        library::set_album_cover(conn, album_id, &data, mime)?;
    }
    if metadata.lyrics_synced.is_some() || metadata.lyrics_plain.is_some() {
        library::set_lyrics(
            conn,
            track_id,
            metadata.lyrics_synced.as_deref(),
            metadata.lyrics_plain.as_deref(),
            Some("manuell"),
        )?;
    }

    library::prune_empty(conn)?;
    Ok(library::get_track(conn, track_id)?)
}

#[tauri::command]
pub async fn search_metadata_online(query: String) -> CmdResult<Vec<MetadataCandidate>> {
    Ok(online::search_metadata(&query).await?)
}

/// Lädt zu einem Treffer alles nach, was die Quelle hergibt: Cover, Lyrics,
/// Release-Art, Titelnummer und Albumkünstler.
#[tauri::command]
pub async fn enrich_candidate(
    candidate: MetadataCandidate,
    duration_ms: Option<i64>,
) -> CmdResult<TrackMetadata> {
    Ok(online::enrich(&candidate, duration_ms, true, true).await)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteImage {
    pub base64: String,
    pub mime: String,
}

#[tauri::command]
pub async fn fetch_cover(url: String) -> CmdResult<RemoteImage> {
    let (data, mime) = online::fetch_image(&url).await?;
    Ok(RemoteImage {
        base64: b64(&data),
        mime,
    })
}

// ----------------------------------------------------------------- Lyrics

#[tauri::command]
pub fn get_lyrics(state: State<'_, AppState>, track_id: i64) -> CmdResult<Option<Lyrics>> {
    let conn = state.db.lock();
    Ok(library::get_lyrics(&conn, track_id)?)
}

#[tauri::command]
pub fn save_lyrics(
    state: State<'_, AppState>,
    track_id: i64,
    synced: Option<String>,
    plain: Option<String>,
    write_to_file: bool,
) -> CmdResult<()> {
    let path = {
        let conn = state.db.lock();
        library::set_lyrics(
            &conn,
            track_id,
            synced.as_deref(),
            plain.as_deref(),
            Some("manuell"),
        )?;
        library::get_track(&conn, track_id)?.path
    };

    if write_to_file {
        let mut meta = tags::read(Path::new(&path))?.metadata;
        meta.lyrics_synced = synced;
        meta.lyrics_plain = plain;
        tags::write(Path::new(&path), &meta)?;
    }
    Ok(())
}

/// Holt Lyrics automatisch passend zum Titel und speichert sie.
#[tauri::command]
pub async fn fetch_lyrics_online(
    state: State<'_, AppState>,
    track_id: i64,
) -> CmdResult<Lyrics> {
    let (artist, title, album, duration_ms) = {
        let conn = state.db.lock();
        let track = library::get_track(&conn, track_id)?;
        (
            track.artist_name,
            track.title,
            track.album_title,
            track.duration_ms,
        )
    };

    let found = online::get_lyrics(&artist, &title, Some(&album), Some(duration_ms)).await?;

    let conn = state.db.lock();
    library::set_lyrics(
        &conn,
        track_id,
        found.synced_lyrics.as_deref(),
        found.plain_lyrics.as_deref(),
        Some("LRCLIB"),
    )?;
    let lyrics = library::get_lyrics(&conn, track_id)?;
    lyrics.ok_or_else(|| Error(fehler!("Lyrics konnten nicht gespeichert werden")))
}

#[tauri::command]
pub async fn search_lyrics_online(query: String) -> CmdResult<Vec<LyricsCandidate>> {
    Ok(online::search_lyrics(&query).await?)
}

// -------------------------------------------------------------- Playlists

#[tauri::command]
pub fn list_playlists(state: State<'_, AppState>) -> CmdResult<Vec<Playlist>> {
    let conn = state.db.lock();
    Ok(library::list_playlists(&conn)?)
}

#[tauri::command]
pub fn get_playlist(state: State<'_, AppState>, id: i64) -> CmdResult<Playlist> {
    let conn = state.db.lock();
    Ok(library::get_playlist(&conn, id)?)
}

#[tauri::command]
pub fn create_playlist(
    state: State<'_, AppState>,
    name: String,
    description: Option<String>,
    cover_base64: Option<String>,
    cover_mime: Option<String>,
) -> CmdResult<Playlist> {
    let conn = state.db.lock();
    let id = library::create_playlist(&conn, &name, description.as_deref())?;
    if let Some(encoded) = cover_base64.as_deref().filter(|value| !value.is_empty()) {
        let data = base64::engine::general_purpose::STANDARD.decode(encoded)?;
        let mime = cover_mime.as_deref().unwrap_or("image/jpeg");
        library::set_playlist_cover(&conn, id, Some((&data, mime)))?;
    }
    Ok(library::get_playlist(&conn, id)?)
}

#[tauri::command]
pub fn update_playlist(
    state: State<'_, AppState>,
    id: i64,
    name: String,
    description: Option<String>,
    cover_base64: Option<String>,
    cover_mime: Option<String>,
    remove_cover: bool,
) -> CmdResult<Playlist> {
    let conn = state.db.lock();
    library::update_playlist(&conn, id, &name, description.as_deref())?;

    if remove_cover {
        library::set_playlist_cover(&conn, id, None)?;
    } else if let Some(encoded) = cover_base64.as_deref().filter(|value| !value.is_empty()) {
        let data = base64::engine::general_purpose::STANDARD.decode(encoded)?;
        let mime = cover_mime.as_deref().unwrap_or("image/jpeg");
        library::set_playlist_cover(&conn, id, Some((&data, mime)))?;
    }
    Ok(library::get_playlist(&conn, id)?)
}

/// Ein Titel, wie ihn der Downloader kennt: Künstler und Name.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaylistEntry {
    pub artist: String,
    pub title: String,
}

/// Was beim Übernehmen einer Playlist herauskam.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaylistFill {
    pub playlist: Playlist,
    /// Neu angelegt oder eine vorhandene ergänzt?
    pub created: bool,
    /// Wie viele Titel diesmal dazugekommen sind.
    pub added: i64,
}

/// Legt eine Playlist an oder ergänzt eine gleichnamige vorhandene.
///
/// Gedacht für den Downloader: Nach dem Laden einer Playlist soll sie sich mit
/// einem Griff übernehmen lassen. Gesucht wird über Künstler und Titel, nicht
/// über Kennungen, so landen auch die Titel darin, die schon vorher in der
/// Bibliothek lagen und deshalb übersprungen wurden.
///
/// Ein zweiter Aufruf legt keine Kopie an, sondern trägt nur nach, was
/// inzwischen dazugekommen ist. Ist nichts dazugekommen, ändert sich nichts.
/// Die Reihenfolge der Vorlage bleibt erhalten.
#[tauri::command]
pub fn create_playlist_from_entries(
    state: State<'_, AppState>,
    name: String,
    entries: Vec<PlaylistEntry>,
) -> CmdResult<PlaylistFill> {
    let conn = state.db.lock();
    let name = name.trim().to_string();

    // Einmal die ganze Bibliothek als Schlüsselpaare, statt je Eintrag zu suchen.
    let vorhanden: std::collections::HashMap<(String, String), i64> =
        library::list_tracks(&conn, None, 100_000)
            .unwrap_or_default()
            .into_iter()
            .map(|t| ((db::key_of(&t.artist_name), db::key_of(&t.title)), t.id))
            .collect();

    let mut ids = Vec::new();
    for entry in &entries {
        let schluessel = (db::key_of(&entry.artist), db::key_of(&entry.title));
        if let Some(id) = vorhanden.get(&schluessel) {
            if !ids.contains(id) {
                ids.push(*id);
            }
        }
    }

    if ids.is_empty() {
        return Err(Error(
            "Keiner der Titel liegt in der Bibliothek, die Playlist bliebe leer.".into(),
        ));
    }

    // Gibt es sie schon, wird ergänzt statt ein zweites Mal angelegt.
    let bestehend: Option<i64> = library::list_playlists(&conn)?
        .into_iter()
        .find(|p| db::key_of(&p.name) == db::key_of(&name))
        .map(|p| p.id);

    let created = bestehend.is_none();
    let playlist_id = match bestehend {
        Some(id) => id,
        None => library::create_playlist(&conn, &name, None)?,
    };

    let vorher = library::playlist_tracks(&conn, playlist_id)?.len() as i64;
    library::add_to_playlist(&conn, playlist_id, &ids)?;
    let playlist = library::get_playlist(&conn, playlist_id)?;

    Ok(PlaylistFill {
        added: playlist.track_count - vorher,
        created,
        playlist,
    })
}

#[tauri::command]
pub fn delete_playlist(state: State<'_, AppState>, id: i64) -> CmdResult<()> {
    let conn = state.db.lock();
    Ok(library::delete_playlist(&conn, id)?)
}

#[tauri::command]
pub fn playlist_tracks(state: State<'_, AppState>, playlist_id: i64) -> CmdResult<Vec<Track>> {
    let conn = state.db.lock();
    Ok(library::playlist_tracks(&conn, playlist_id)?)
}

/// Wo liegen diese Titel bereits? Playlist-Kennung und Anzahl.
#[tauri::command]
pub fn playlists_containing(
    state: State<'_, AppState>,
    track_ids: Vec<i64>,
) -> CmdResult<Vec<(i64, i64)>> {
    let conn = state.db.lock();
    Ok(library::playlists_containing(&conn, &track_ids)?)
}

#[tauri::command]
pub fn add_to_playlist(
    state: State<'_, AppState>,
    playlist_id: i64,
    track_ids: Vec<i64>,
) -> CmdResult<()> {
    let conn = state.db.lock();
    Ok(library::add_to_playlist(&conn, playlist_id, &track_ids)?)
}

#[tauri::command]
pub fn remove_from_playlist(
    state: State<'_, AppState>,
    playlist_id: i64,
    track_id: i64,
) -> CmdResult<()> {
    let conn = state.db.lock();
    Ok(library::remove_from_playlist(&conn, playlist_id, track_id)?)
}

#[tauri::command]
pub fn reorder_playlist(
    state: State<'_, AppState>,
    playlist_id: i64,
    track_ids: Vec<i64>,
) -> CmdResult<()> {
    let conn = state.db.lock();
    Ok(library::reorder_playlist(&conn, playlist_id, &track_ids)?)
}

/// Reihenfolge der Sammlung selbst, nicht der Titel darin.
#[tauri::command]
pub fn reorder_playlists(
    app: AppHandle,
    state: State<'_, AppState>,
    playlist_ids: Vec<i64>,
) -> CmdResult<()> {
    let conn = state.db.lock();
    library::reorder_playlists(&conn, &playlist_ids)?;
    drop(conn);
    // Die Seitenleiste zeigt dieselbe Ordnung und muss mitziehen.
    let _ = app.emit("library:changed", ());
    Ok(())
}

// ----------------------------------------------------------------- Player

#[tauri::command]
pub fn player_state(state: State<'_, AppState>) -> CmdResult<PlayerState> {
    Ok(state.player.state())
}

#[tauri::command]
pub fn play_tracks(
    state: State<'_, AppState>,
    track_ids: Vec<i64>,
    start_index: Option<usize>,
) -> CmdResult<()> {
    state.player.send(Cmd::SetQueue {
        track_ids,
        start: start_index.unwrap_or(0),
    })?;
    Ok(())
}

#[tauri::command]
pub fn player_toggle(state: State<'_, AppState>) -> CmdResult<()> {
    Ok(state.player.send(Cmd::TogglePlay)?)
}

#[tauri::command]
pub fn player_play(state: State<'_, AppState>) -> CmdResult<()> {
    Ok(state.player.send(Cmd::Resume)?)
}

#[tauri::command]
pub fn player_pause(state: State<'_, AppState>) -> CmdResult<()> {
    Ok(state.player.send(Cmd::Pause)?)
}

#[tauri::command]
pub fn player_next(state: State<'_, AppState>) -> CmdResult<()> {
    Ok(state.player.send(Cmd::Next)?)
}

#[tauri::command]
pub fn player_previous(state: State<'_, AppState>) -> CmdResult<()> {
    Ok(state.player.send(Cmd::Prev)?)
}

#[tauri::command]
pub fn player_stop(state: State<'_, AppState>) -> CmdResult<()> {
    Ok(state.player.send(Cmd::Stop)?)
}

#[tauri::command]
pub fn player_seek(state: State<'_, AppState>, position_ms: u64) -> CmdResult<()> {
    Ok(state.player.send(Cmd::Seek(position_ms))?)
}

#[tauri::command]
pub fn player_set_volume(state: State<'_, AppState>, volume: f32) -> CmdResult<()> {
    Ok(state.player.send(Cmd::SetVolume(volume))?)
}

#[tauri::command]
pub fn player_set_muted(state: State<'_, AppState>, muted: bool) -> CmdResult<()> {
    Ok(state.player.send(Cmd::SetMuted(muted))?)
}

#[tauri::command]
pub fn player_set_repeat(state: State<'_, AppState>, mode: RepeatMode) -> CmdResult<()> {
    Ok(state.player.send(Cmd::SetRepeat(mode))?)
}

#[tauri::command]
pub fn player_set_shuffle(state: State<'_, AppState>, shuffle: bool) -> CmdResult<()> {
    Ok(state.player.send(Cmd::SetShuffle(shuffle))?)
}

#[tauri::command]
pub fn queue_add(state: State<'_, AppState>, track_ids: Vec<i64>) -> CmdResult<()> {
    Ok(state.player.send(Cmd::Enqueue(track_ids))?)
}

#[tauri::command]
pub fn queue_play_next(state: State<'_, AppState>, track_ids: Vec<i64>) -> CmdResult<()> {
    Ok(state.player.send(Cmd::PlayNext(track_ids))?)
}

#[tauri::command]
pub fn queue_remove(state: State<'_, AppState>, index: usize) -> CmdResult<()> {
    Ok(state.player.send(Cmd::RemoveFromQueue(index))?)
}

#[tauri::command]
pub fn queue_clear(state: State<'_, AppState>) -> CmdResult<()> {
    Ok(state.player.send(Cmd::ClearQueue)?)
}

/// `minutes` wird bei `endOfTrack` ignoriert.
#[tauri::command]
pub fn set_sleep_timer(
    state: State<'_, AppState>,
    mode: Option<SleepTimerMode>,
    minutes: Option<u64>,
) -> CmdResult<()> {
    let payload = match mode {
        Some(SleepTimerMode::Duration) => {
            let minutes = minutes.unwrap_or(30).max(1);
            Some((SleepTimerMode::Duration, minutes * 60_000))
        }
        Some(SleepTimerMode::EndOfTrack) => Some((SleepTimerMode::EndOfTrack, 0)),
        None => None,
    };
    Ok(state.player.send(Cmd::SetSleepTimer(payload))?)
}

// ------------------------------------------------------------ Statistiken

#[tauri::command]
pub fn weekly_mix(state: State<'_, AppState>, offset: Option<i64>) -> CmdResult<WeeklyMix> {
    let conn = state.db.lock();
    Ok(stats::weekly_mix(&conn, offset.unwrap_or(0))?)
}

/// Die letzten Wochenmixe für die Übersicht auf der Startseite.
#[tauri::command]
pub fn weekly_mixes(
    state: State<'_, AppState>,
    limit: Option<usize>,
) -> CmdResult<Vec<stats::WeeklyMixSummary>> {
    let conn = state.db.lock();
    Ok(stats::weekly_mixes(&conn, limit.unwrap_or(12))?)
}

/// Übernimmt einen Wochenmix als richtige Playlist.
///
/// Der Mix selbst bleibt ein Rückblick und ändert sich mit den Hördaten.
/// Wer ihn behalten will, bekommt eine Kopie, die ihm gehört, benennbar,
/// sortierbar, löschbar wie jede andere Playlist.
#[tauri::command]
/// Name und Beschreibung kommen aus der Oberfläche, nicht von hier: Sie landen
/// als Text in der Datenbank und sollen in der Sprache stehen, die der Nutzer
/// eingestellt hat.
pub fn save_weekly_mix(
    app: AppHandle,
    state: State<'_, AppState>,
    offset: Option<i64>,
    name: String,
    description: Option<String>,
) -> CmdResult<Playlist> {
    let conn = state.db.lock();
    let mix = stats::weekly_mix(&conn, offset.unwrap_or(0))?;
    if mix.items.is_empty() {
        return Err(Error(fehler!("Dieser Wochenmix ist noch leer.")));
    }

    // Gibt es den Namen schon, bekommt die Kopie eine Nummer angehängt.
    let mut vergeben = name.clone();
    let mut zaehler = 2;
    while library::playlist_name_taken(&conn, &vergeben)? {
        vergeben = format!("{name} ({zaehler})");
        zaehler += 1;
    }

    let playlist_id = library::create_playlist(&conn, &vergeben, description.as_deref())?;
    let ids: Vec<i64> = mix.items.iter().map(|item| item.track.id).collect();
    library::add_to_playlist(&conn, playlist_id, &ids)?;
    let playlist = library::get_playlist(&conn, playlist_id)?;
    drop(conn);

    let _ = app.emit("library:changed", ());
    Ok(playlist)
}

/// Zuletzt gespielte Titel, jeder nur einmal.
#[tauri::command]
pub fn recently_played(state: State<'_, AppState>, limit: Option<i64>) -> CmdResult<Vec<Track>> {
    let conn = state.db.lock();
    Ok(library::recently_played(&conn, limit.unwrap_or(20))?)
}

#[tauri::command]
pub fn wrapped(state: State<'_, AppState>, period: String, offset: i64) -> CmdResult<Wrapped> {
    let conn = state.db.lock();
    Ok(stats::wrapped(&conn, &period, offset)?)
}

// ------------------------------------------------------------- Downloader

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloaderStatus {
    pub ytdlp_path: Option<String>,
    pub ytdlp_version: Option<String>,
    pub ffmpeg_available: bool,
    /// Für YouTube nötig; ohne sie kommt es zu 403-Fehlern.
    pub js_runtime: Option<String>,
    /// Lässt sich an dieser fehlenden Laufzeit überhaupt etwas ändern?
    ///
    /// Auf Android nicht: Dort gibt es weder Node noch Deno, und installieren
    /// kann man sie auch nicht. Eine Warnung, der niemand abhelfen kann, ist
    /// keine Warnung, sondern Lärm.
    pub js_runtime_relevant: bool,
    pub active_jobs: Vec<String>,
}

#[tauri::command]
pub async fn downloader_status(state: State<'_, AppState>) -> CmdResult<DownloaderStatus> {
    let configured = {
        let conn = state.db.lock();
        db::get_setting(&conn, "ytdlp_path").ok().flatten()
    };

    // Auf Android gibt es keine Datei zu finden: yt-dlp liegt als Bibliothek
    // bei. Die Fassung erfragen wir über dieselbe Brücke, die auch die Suche
    // benutzt, damit die Anzeige nicht behauptet, es fehle etwas.
    let (path, version) = if cfg!(target_os = "android") {
        let fassung = crate::ytdlp::einmal(Path::new(""), &["--version".to_string()])
            .await
            .ok()
            .filter(|a| a.erfolg)
            .map(|a| a.stdout.trim().to_string());
        (Some("eingebaut".to_string()), fassung)
    } else {
        let gefunden = downloader::find_ytdlp(configured.as_deref(), &state.tools_dir());
        let fassung = match &gefunden {
            Some(p) => tokio::process::Command::new(p)
                .arg("--version")
                .output()
                .await
                .ok()
                .filter(|o| o.status.success())
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()),
            None => None,
        };
        (gefunden.map(|p| p.to_string_lossy().to_string()), fassung)
    };

    Ok(DownloaderStatus {
        ytdlp_path: path,
        ytdlp_version: version,
        ffmpeg_available: downloader::ffmpeg_available(),
        js_runtime: downloader::js_runtime().map(str::to_string),
        js_runtime_relevant: !cfg!(target_os = "android"),
        active_jobs: state.downloads.active(),
    })
}

/// Nimmt entgegen, was im Downloader eingegeben wurde, und entscheidet selbst,
/// was zu tun ist:
///
/// * Spotify-Link → Metadaten auflösen, Audio über die übrigen Quellen suchen
///   (Spotify gibt seine Aufnahmen nur DRM-verschlüsselt heraus)
/// * jede andere Adresse → direkt an yt-dlp, Sammlungen werden aufgeklappt
/// * alles Übrige → Suchbegriff, alle Suchquellen gleichzeitig
///
/// Damit muss niemand vorher eine Quelle auswählen.
#[tauri::command]
pub async fn resolve_input(
    state: State<'_, AppState>,
    input: String,
    limit: Option<usize>,
) -> CmdResult<LinkPlan> {
    let input = input.trim().to_string();
    if input.is_empty() {
        return Err(Error(fehler!("Bitte etwas eingeben.")));
    }

    if let Some(reference) = spotify::parse_link(&input) {
        return resolve_spotify(&state, reference).await;
    }

    let configured = {
        let conn = state.db.lock();
        db::get_setting(&conn, "ytdlp_path").ok().flatten()
    };
    let ytdlp = downloader::ensure_ytdlp(configured.as_deref(), &state.tools_dir()).await?;

    let is_link = input.starts_with("http://") || input.starts_with("https://");

    if is_link {
        // yt-dlp kennt den Titel hinter der Adresse, schöner als die nackte URL.
        // Playlists, Alben und Sets kommen als mehrere Einträge zurück.
        let found = downloader::search(&ytdlp, &input, SearchSource::Url, 1)
            .await
            .unwrap_or_default();

        let items: Vec<downloader::DownloadPlan> = if found.is_empty() {
            vec![downloader::DownloadPlan {
                url: input.clone(),
                fallbacks: Vec::new(),
                match_query: None,
                // Ein eingefügter Link ist die Absicht selbst, nichts zu prüfen.
                intent: None,
                title: input.clone(),
                subtitle: None,
                thumbnail: None,
                duration_ms: None,
                source: "Link".into(),
                metadata: None,
                already_in_library: false,
            }]
        } else {
            found.into_iter().map(Into::into).collect()
        };

        return Ok(LinkPlan {
            label: if items.len() > 1 {
                input.clone()
            } else {
                items[0].title.clone()
            },
            kind: "link".into(),
            notes: Vec::new(),
            batch: items.len() > 1,
            items,
        });
    }

    let (found, ausgefallen) =
        downloader::search_all_sources(&ytdlp, &input, limit.unwrap_or(8)).await?;
    if found.is_empty() {
        return Err(Error(fehler!("Keine Treffer für „{0}“.", input)));
    }

    Ok(LinkPlan {
        // Nur die Eingabe, nicht „Treffer für …“: Den Satz baut die Oberfläche
        // in ihrer Sprache, sie erkennt den Fall an `kind`.
        label: input.clone(),
        kind: "search".into(),
        // Eine dünne Trefferliste kann auch an einer gesperrten Quelle liegen.
        notes: if ausgefallen.is_empty() {
            Vec::new()
        } else {
            vec![downloader::PlanHinweis {
                code: "quellen-ausgefallen".into(),
                args: vec![ausgefallen.join(", ")],
            }]
        },
        batch: false,
        // Scheitert die gewählte Quelle, wandert der Download automatisch
        // zur nächsten, statt mit „nicht möglich“ stehen zu bleiben.
        // Die Eingabe reist mit, damit sich das Ergebnis gegenprüfen lässt.
        items: downloader::plans_with_fallbacks(found)
            .into_iter()
            .map(|plan| downloader::DownloadPlan {
                intent: Some(input.clone()),
                ..plan
            })
            .collect(),
    })
}

/// Führt mehrere Abfragen nebenläufig aus und sammelt ihre Ergebnisse ein.
///
/// Bewusst über `tokio::spawn` statt über eine zusätzliche Abhängigkeit: Für
/// dieses eine Bündel Cover-Abfragen lohnt sich `futures` nicht.
async fn nebenlaeufig_sammeln<F>(
    aufgaben: impl Iterator<Item = F>,
) -> Vec<(String, Option<String>)>
where
    F: std::future::Future<Output = (String, Option<String>)> + Send + 'static,
{
    let laufend: Vec<_> = aufgaben.map(tokio::spawn).collect();
    let mut ergebnisse = Vec::with_capacity(laufend.len());
    for aufgabe in laufend {
        if let Ok(paar) = aufgabe.await {
            ergebnisse.push(paar);
        }
    }
    ergebnisse
}

/// So viele Titel gibt Spotifys öffentliche Einbettung höchstens heraus.
/// Durch Live-Test belegt (`tests/spotify_live.rs`).
const SPOTIFY_EMBED_LIMIT: usize = 100;

async fn resolve_spotify(
    state: &AppState,
    reference: spotify::SpotifyRef,
) -> CmdResult<LinkPlan> {
    let release = spotify::resolve(&reference).await?;

    // Einmal alle vorhandenen Titel als Schlüsselpaare holen, statt für jeden
    // Eintrag der Playlist erneut in die Datenbank zu fassen.
    let bereits: std::collections::HashSet<(String, String)> = {
        let conn = state.db.lock();
        library::list_tracks(&conn, None, 100_000)
            .unwrap_or_default()
            .into_iter()
            .map(|t| (crate::db::key_of(&t.artist_name), crate::db::key_of(&t.title)))
            .collect()
    };

    // Das Cover einmal laden und für alle Titel des Releases verwenden.
    let cover = match release.cover_url.as_deref() {
        Some(url) => online::fetch_image(url)
            .await
            .ok()
            .map(|(data, mime)| (b64(&data), mime)),
        None => None,
    };

    let is_album = matches!(reference.kind, spotify::SpotifyKind::Album);
    let album_title = if is_album { release.name.clone() } else { String::new() };

    // Das Bild gehört nur dann zu jedem einzelnen Titel, wenn es ein Album
    // oder ein einzelner Titel ist. Bei einer Playlist ist es deren eigenes
    // Bild, die Titel darin stammen aus ganz verschiedenen Releases und
    // bekommen ihr richtiges Cover erst beim Anreichern nach dem Laden.
    let track_cover = matches!(
        reference.kind,
        spotify::SpotifyKind::Album | spotify::SpotifyKind::Track
    )
    .then_some(cover.as_ref())
    .flatten();

    // Für Playlists das Bild jedes Titels einzeln nachschlagen. Die Titelliste
    // gibt nur die Kennung her; ohne diesen Griff trüge jeder Eintrag das Bild
    // der Playlist. Höchstens acht Abfragen gleichzeitig, damit Spotify nicht
    // drosselt, und mit Zeitlimit, damit eine lahme Antwort das Auflösen nicht
    // aufhält.
    let einzelbilder: std::collections::HashMap<String, String> =
        if is_album || reference.kind == spotify::SpotifyKind::Track {
            std::collections::HashMap::new()
        } else {
            let ids: Vec<String> = release.tracks.iter().filter_map(|t| t.id.clone()).collect();
            let mut gefunden = std::collections::HashMap::new();
            for gruppe in ids.chunks(8) {
                let abfragen = gruppe.iter().map(|id| {
                    let id = id.clone();
                    async move {
                        let url = tokio::time::timeout(
                            std::time::Duration::from_secs(8),
                            spotify::track_cover_url(&id),
                        )
                        .await
                        .ok()
                        .flatten();
                        (id, url)
                    }
                });
                for (id, url) in nebenlaeufig_sammeln(abfragen).await {
                    if let Some(url) = url {
                        gefunden.insert(id, url);
                    }
                }
            }
            gefunden
        };

    let items = release
        .tracks
        .iter()
        .map(|track| {
            let primary_artist = track
                .artists
                .first()
                .cloned()
                .unwrap_or_else(|| "Unbekannter Künstler".into());
            let metadata = TrackMetadata {
                title: track.title.clone(),
                // Spotify kennt keine Rollen, alle Beteiligten sind
                // Hauptkünstler, außer sie standen als „feat.“ im Titel.
                artist: library::join_artists(&track.artists),
                featured_artists: (!track.featured.is_empty())
                    .then(|| library::join_artists(&track.featured)),
                album: album_title.clone(),
                album_artist: is_album.then(|| release.artist.clone()).flatten(),
                release_type: is_album.then(|| "album".to_string()),
                year: release.year,
                track_no: track.track_no,
                disc_no: None,
                genre: None,
                cover_base64: track_cover.map(|(data, _)| data.clone()),
                cover_mime: track_cover.map(|(_, mime)| mime.clone()),
                lyrics_synced: None,
                lyrics_plain: None,
            };

            downloader::DownloadPlan {
                // Spotify kennt die Laufzeit genau, daraus sucht der
                // Downloader die am besten passende Aufnahme heraus.
                match_query: Some(format!("{} {}", primary_artist, track.title)),
                // Spotify nennt Titel und Künstler verbindlich.
                intent: Some(format!("{} {}", primary_artist, track.title)),
                url: format!("scsearch1:{} {}", primary_artist, track.title),
                fallbacks: vec![format!(
                    "ytsearch1:{} {} audio",
                    primary_artist, track.title
                )],
                title: track.title.clone(),
                subtitle: Some(library::join_artists(&track.artists)),
                thumbnail: track
                    .id
                    .as_deref()
                    .and_then(|id| einzelbilder.get(id).cloned())
                    .or_else(|| release.cover_url.clone()),
                duration_ms: track.duration_ms,
                source: "Spotify".into(),
                metadata: Some(metadata),
                already_in_library: bereits.contains(&(
                    crate::db::key_of(&primary_artist),
                    crate::db::key_of(&track.title),
                )),
            }
        })
        .collect();

    Ok(LinkPlan {
        label: match &release.artist {
            Some(artist) => format!("{} · {}", artist, release.name),
            None => release.name.clone(),
        },
        batch: release.tracks.len() > 1,
        kind: format!("spotify-{}", reference.kind.label().to_lowercase()),
        notes: {
            let mut hinweise = vec![downloader::PlanHinweis {
                code: "spotify-nur-metadaten".into(),
                args: Vec::new(),
            }];
            // Die öffentliche Einbettung rückt nie mehr als 100 Titel heraus.
            // Ohne Hinweis wundert man sich, warum eine lange Playlist genau
            // dort aufhört.
            if release.tracks.len() >= SPOTIFY_EMBED_LIMIT {
                hinweise.push(downloader::PlanHinweis {
                    code: "spotify-grenze".into(),
                    args: vec![SPOTIFY_EMBED_LIMIT.to_string()],
                });
            }
            hinweise
        },
        items,
    })
}

#[tauri::command]
pub async fn start_download(
    app: AppHandle,
    state: State<'_, AppState>,
    job_id: String,
    options: DownloadOptions,
) -> CmdResult<DownloadOutcome> {
    let (configured, work_dir, registry, auto_cover, auto_lyrics) = {
        let conn = state.db.lock();
        let flag = |key: &str| db::get_setting(&conn, key).ok().flatten().as_deref() != Some("0");
        (
            db::get_setting(&conn, "ytdlp_path").ok().flatten(),
            state.work_dir.clone(),
            state.downloads.clone(),
            flag("auto_fetch_cover"),
            flag("auto_fetch_lyrics"),
        )
    };
    let ytdlp = downloader::ensure_ytdlp(configured.as_deref(), &state.tools_dir()).await?;

    // Die Einstellungen entscheiden, was automatisch nachgeladen wird.
    let options = DownloadOptions {
        auto_cover,
        auto_lyrics,
        ..options
    };

    Ok(downloader::download(app, registry, ytdlp, work_dir, job_id, options).await?)
}

#[tauri::command]
pub fn cancel_download(state: State<'_, AppState>, job_id: String) -> CmdResult<bool> {
    Ok(state.downloads.cancel(&job_id))
}

/// Ersetzt alles, was in Dateinamen Ärger macht.
/// Unter Windows belegt das Betriebssystem diese Namen für Geräte, eine
/// Datei „CON.mp3“ lässt sich dort nicht anlegen. Ein Künstler namens „Aux“
/// würde den Import sonst auf einem Windows-Rechner scheitern lassen.
const RESERVED_NAMES: [&str; 22] = [
    "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8",
    "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
];

/// Ordner im Arbeitsverzeichnis für übernommene, aber nicht einsortierte
/// Titel. Vom Aufräumen ausgenommen, siehe [`crate::db::cleanup_work_dir`].
pub const KEEP_DIR: &str = "behalten";

/// Gehört das Verzeichnis zu einem Download-Auftrag?
///
/// Der Downloader legt je Auftrag `job-<zeitstempel>-<nr>` an. Die Prüfung
/// verhindert, dass beim Aufräumen ein anderer Ordner erwischt wird.
pub fn is_job_dir(dir: &Path) -> bool {
    dir.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with("job-"))
}

/// Verschiebt eine Datei in ein Zielverzeichnis, ohne Bestehendes zu
/// überschreiben. Über Laufwerksgrenzen hinweg scheitert `rename`, dann
/// wird kopiert und die Quelle entfernt.
fn move_file(source: &Path, dir: &Path, file_name: &str) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;

    let mut target = dir.join(file_name);
    let mut counter = 2;
    while target.exists() {
        target = dir.join(format!("{counter} - {file_name}"));
        counter += 1;
    }

    if std::fs::rename(source, &target).is_err() {
        std::fs::copy(source, &target)?;
        let _ = std::fs::remove_file(source);
    }
    Ok(target)
}

fn sanitize(value: &str) -> String {
    let cleaned: String = value
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c if c.is_control() => ' ',
            c => c,
        })
        .collect();
    // Das Abschneiden der Punkte am Ende entschärft nebenbei „.“ und „..“:
    // Beide werden dadurch leer und bekommen den Platzhalter.
    let trimmed = cleaned.trim().trim_end_matches('.').trim();
    if trimmed.is_empty() {
        return "Unbenannt".into();
    }

    let gekuerzt: String = trimmed.chars().take(120).collect();
    if RESERVED_NAMES.contains(&gekuerzt.to_lowercase().as_str()) {
        return format!("_{gekuerzt}");
    }
    gekuerzt
}

/// Verschiebt die Datei in die Bibliothek und legt den Titel an.
/// Wird nach dem Bestätigen der Metadaten im Downloader aufgerufen.
#[tauri::command]
pub fn import_download(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
    metadata: TrackMetadata,
    source_url: Option<String>,
    move_into_library: bool,
) -> CmdResult<Track> {
    let source_path = PathBuf::from(&path);
    if !source_path.exists() {
        return Err(Error(fehler!("Datei nicht gefunden: {0}", path)));
    }

    tags::write(&source_path, &metadata)?;

    let extension = source_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("mp3")
        .to_string();
    let file_name = match metadata.track_no {
        Some(n) if n > 0 => format!("{:02} - {}.{}", n, sanitize(&metadata.title), extension),
        _ => format!("{}.{}", sanitize(&metadata.title), extension),
    };

    // Die Datei verlässt in jedem Fall den Job-Ordner. Bleibt sie dort, zeigt
    // die Bibliothek in ein Arbeitsverzeichnis, das aufgeräumt werden muss,
    // jede Aufräumlogik würde solche Titel mitreißen.
    let dir = if move_into_library {
        let artist_folder = sanitize(
            metadata
                .album_artist
                .as_deref()
                .filter(|a| !a.trim().is_empty())
                .unwrap_or(&metadata.artist),
        );
        let album_folder = sanitize(if metadata.album.trim().is_empty() {
            &metadata.title
        } else {
            &metadata.album
        });
        state.library_dir().join(artist_folder).join(album_folder)
    } else {
        // „Nicht einsortieren“ heißt: nicht in den Musikordner, nicht, dass
        // die Datei im Arbeitsverzeichnis liegen bleibt.
        state.work_dir.join(KEEP_DIR)
    };

    let final_path = move_file(&source_path, &dir, &file_name)?;

    // Der Job ist erledigt, sein Ordner enthält nur noch Hilfsdateien
    // (result.txt, uploader.txt, musik.txt, Vorschaubilder).
    if let Some(job_dir) = source_path.parent().filter(|dir| is_job_dir(dir)) {
        let _ = std::fs::remove_dir_all(job_dir);
    }

    let conn = state.db.lock();
    let track_id = scanner::import_file(&conn, &final_path, Some("download"))?;

    if let Some(url) = source_url {
        conn.execute(
            "UPDATE tracks SET source_url = ?2 WHERE id = ?1",
            rusqlite::params![track_id, url],
        )?;
    }

    let album_id: i64 = conn.query_row(
        "SELECT album_id FROM tracks WHERE id = ?1",
        [track_id],
        |r| r.get(0),
    )?;
    if let Some(rt) = metadata.release_type.as_deref() {
        conn.execute(
            "UPDATE albums SET release_type = ?2, release_type_locked = 1 WHERE id = ?1",
            rusqlite::params![album_id, ReleaseType::parse(rt).as_str()],
        )?;
    }
    if let Some(cover) = metadata.cover_base64.as_deref().filter(|c| !c.is_empty()) {
        let data = base64::engine::general_purpose::STANDARD.decode(cover)?;
        let mime = metadata.cover_mime.as_deref().unwrap_or("image/jpeg");
        library::set_album_cover(&conn, album_id, &data, mime)?;
    }
    if metadata.lyrics_synced.is_some() || metadata.lyrics_plain.is_some() {
        library::set_lyrics(
            &conn,
            track_id,
            metadata.lyrics_synced.as_deref(),
            metadata.lyrics_plain.as_deref(),
            Some("download"),
        )?;
    }

    let track = library::get_track(&conn, track_id)?;
    // Künstler, die mit diesem Titel neu dazukommen, haben noch keine Angaben.
    let neue_kuenstler = if auto_fetch_artists(&conn) {
        library::artists_missing_metadata(&conn, track_id).unwrap_or_default()
    } else {
        Vec::new()
    };
    drop(conn);

    let _ = app.emit("library:changed", ());
    fetch_artists_in_background(&app, neue_kuenstler);
    Ok(track)
}

// ---------------------------------------------------------- Einstellungen

/// Dürfen Künstlerangaben automatisch nachgeladen werden?
/// Lyrics automatisch suchen? Vorgabe ja.
fn auto_fetch_lyrics(conn: &rusqlite::Connection) -> bool {
    db::get_setting(conn, "auto_fetch_lyrics")
        .ok()
        .flatten()
        .as_deref()
        != Some("0")
}

/// Beim Import nachschlagen? Vorgabe ja.
fn auto_fetch_import(conn: &rusqlite::Connection) -> bool {
    db::get_setting(conn, "auto_fetch_import")
        .ok()
        .flatten()
        .as_deref()
        != Some("0")
}

fn auto_fetch_artists(conn: &rusqlite::Connection) -> bool {
    db::get_setting(conn, "auto_fetch_artists")
        .ok()
        .flatten()
        .as_deref()
        != Some("0")
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub library_dir: String,
    pub download_format: String,
    pub download_quality: String,
    pub ytdlp_path: String,
    pub auto_fetch_lyrics: bool,
    pub auto_fetch_cover: bool,
    /// Bild und Beschreibung beim ersten Titel eines Künstlers holen.
    pub auto_fetch_artists: bool,
    pub move_downloads_into_library: bool,
    pub accent: String,
    /// „system“, „light“ oder „dark“.
    pub theme: String,
    /// Playlists als Kacheln („grid“) oder als Liste („list“).
    pub playlist_view: String,
    /// Kachelgröße: „sm“, „md“ oder „lg“.
    pub playlist_size: String,
    /// Vor dem Löschen nachfragen. Lässt sich im Dialog selbst abstellen.
    pub confirm_delete: bool,
    /// Aus einer geladenen Playlist eine Playlist in der Bibliothek machen.
    pub playlist_from_download: bool,
    /// Wie viel vom Rückblick gezeigt wird: „all“, „month“, „year“ oder „off“.
    pub wrapped_mode: String,
    /// Beim Import fehlende Angaben online nachschlagen.
    pub auto_fetch_import: bool,
    /// Sortierung der Bibliothek: „added“, „title“, „artist“, „album“, „year“.
    pub library_sort: String,
    /// Selbst gemischte Akzentfarben, mit Komma getrennt („#ff0000,#00ff00“).
    /// Als Zeichenkette, weil die Einstellungstabelle nur Text kennt.
    pub accent_custom: String,
    /// Oberflächensprache: „system“, „de“ oder „en“.
    pub language: String,
}

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> CmdResult<Settings> {
    let conn = state.db.lock();
    let get = |key: &str, fallback: &str| {
        db::get_setting(&conn, key)
            .ok()
            .flatten()
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| fallback.to_string())
    };
    let flag = |key: &str, fallback: bool| get(key, if fallback { "1" } else { "0" }) == "1";

    Ok(Settings {
        library_dir: get(
            "library_dir",
            &state.default_library_dir.to_string_lossy(),
        ),
        download_format: get("download_format", "best"),
        download_quality: get("download_quality", "0"),
        ytdlp_path: get("ytdlp_path", ""),
        auto_fetch_lyrics: flag("auto_fetch_lyrics", true),
        auto_fetch_cover: flag("auto_fetch_cover", true),
        auto_fetch_artists: flag("auto_fetch_artists", true),
        move_downloads_into_library: flag("move_downloads_into_library", true),
        accent: get("accent", "#a8a8b3"),
        theme: get("theme", "system"),
        playlist_view: get("playlist_view", "grid"),
        playlist_size: get("playlist_size", "md"),
        confirm_delete: flag("confirm_delete", true),
        playlist_from_download: flag("playlist_from_download", true),
        wrapped_mode: get("wrapped_mode", "all"),
        auto_fetch_import: flag("auto_fetch_import", true),
        // Nach Künstler ist die Ordnung, in der man eine Sammlung durchgeht.
        library_sort: get("library_sort", "artist"),
        accent_custom: get("accent_custom", ""),
        // „system“ folgt der Einstellung des Betriebssystems.
        language: get("language", "system"),
    })
}

#[tauri::command]
pub fn set_setting(state: State<'_, AppState>, key: String, value: String) -> CmdResult<()> {
    let conn = state.db.lock();
    Ok(db::set_setting(&conn, &key, &value)?)
}

/// Wie viele Sicherungen aufgehoben werden.
const MAX_BACKUPS: usize = 5;

/// Legt eine Sicherung der Datenbank an und gibt deren Pfad zurück.
///
/// `VACUUM INTO` schreibt eine in sich stimmige Kopie, auch während die
/// Datenbank geöffnet ist, ein einfaches Kopieren der Datei könnte mitten
/// in einer Schreiboperation landen.
#[tauri::command]
pub fn backup_database(state: State<'_, AppState>) -> CmdResult<String> {
    let dir = state
        .db_path
        .parent()
        .map(|p| p.join("backups"))
        .ok_or_else(|| Error(fehler!("Kein Ort für die Sicherung gefunden.")))?;
    std::fs::create_dir_all(&dir)?;

    let stempel = chrono::Local::now().format("%Y-%m-%d-%H%M%S");
    let ziel = dir.join(format!("robify-{stempel}.db"));

    {
        let conn = state.db.lock();
        // Der Pfad wird als Zeichenkette eingesetzt, weil VACUUM keine
        // Platzhalter erlaubt. Hochkommas verdoppeln, wie in SQL üblich.
        let pfad = ziel.to_string_lossy().replace('\'', "''");
        conn.execute_batch(&format!("VACUUM INTO '{pfad}'"))?;
    }

    // Ältere Sicherungen abräumen, damit sie sich nicht anhäufen.
    let mut vorhanden: Vec<PathBuf> = std::fs::read_dir(&dir)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .map(|eintrag| eintrag.path())
        .filter(|pfad| {
            pfad.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("robify-") && name.ends_with(".db"))
        })
        .collect();
    vorhanden.sort();
    while vorhanden.len() > MAX_BACKUPS {
        let alt = vorhanden.remove(0);
        let _ = std::fs::remove_file(alt);
    }

    Ok(ziel.to_string_lossy().to_string())
}

/// Setzt Robify auf den Auslieferungszustand zurück.
///
/// Vorher wird **immer** eine Sicherung der Datenbank angelegt und ihr Pfad
/// zurückgegeben. Ohne die wäre der Griff endgültig, und es ist der eine
/// Knopf, bei dem ein Fehlgriff alles kostet.
///
/// `delete_files` betrifft die Audiodateien. Auch dann werden sie nicht
/// gelöscht, sondern in den Papierkorb der App verschoben, dort liegen sie
/// 30 Tage, bevor sie endgültig verschwinden.
#[tauri::command]
pub fn reset_app(
    app: AppHandle,
    state: State<'_, AppState>,
    delete_files: bool,
    keep_settings: bool,
) -> CmdResult<String> {
    // Erst sichern. Scheitert das, wird nichts angefasst.
    let sicherung = backup_database(state.clone())?;

    // Nichts soll weiterlaufen, während seine Datei verschwindet.
    let _ = state.player.send(crate::player::Cmd::Stop);

    let papierkorb = state.trash_dir();
    let conn = state.db.lock();

    if delete_files {
        let pfade: Vec<String> = {
            let mut stmt = conn.prepare("SELECT path FROM tracks")?;
            let liste = stmt
                .query_map([], |r| r.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            liste
        };
        let _ = std::fs::create_dir_all(&papierkorb);
        for (nummer, pfad) in pfade.iter().enumerate() {
            let quelle = Path::new(pfad);
            let endung = quelle.extension().and_then(|e| e.to_str()).unwrap_or("dat");
            let ziel = papierkorb.join(format!("reset-{nummer}.{endung}"));
            let _ = std::fs::rename(quelle, &ziel);
        }
    }

    // Reihenfolge nach Abhängigkeiten, auch wenn die Fremdschlüssel kaskadieren:
    // So bleibt nachvollziehbar, was geleert wird.
    conn.execute_batch(
        "DELETE FROM plays;
         DELETE FROM playlist_tracks;
         DELETE FROM playlists;
         DELETE FROM lyrics;
         DELETE FROM track_artists;
         DELETE FROM recommendations;
         DELETE FROM tracks;
         DELETE FROM albums;
         DELETE FROM artists;",
    )?;
    if !keep_settings {
        conn.execute_batch("DELETE FROM settings;")?;
    }
    // Die Datei behält sonst ihre alte Größe, obwohl nichts mehr darin steht.
    conn.execute_batch("VACUUM;")?;
    drop(conn);

    // Liegengebliebene Arbeitsordner gleich mit.
    let _ = std::fs::remove_dir_all(&state.work_dir);
    let _ = std::fs::create_dir_all(&state.work_dir);

    let _ = app.emit("library:changed", ());
    Ok(sicherung)
}

#[tauri::command]
pub fn open_path(app: AppHandle, path: String) -> CmdResult<()> {
    use tauri_plugin_opener::OpenerExt;
    app.opener()
        .open_path(path, None::<&str>)
        .map_err(|e| Error(e.to_string()))
}

#[tauri::command]
pub fn app_paths(app: AppHandle, state: State<'_, AppState>) -> CmdResult<serde_json::Value> {
    Ok(serde_json::json!({
        "database": state.db_path.to_string_lossy(),
        "downloads": state.work_dir.to_string_lossy(),
        "library": state.library_dir().to_string_lossy(),
        "appData": app.path().app_data_dir().ok().map(|p| p.to_string_lossy().to_string()),
    }))
}

#[cfg(test)]
mod tests {
    use super::sanitize;

    #[test]
    fn dateinamen_bleiben_im_zielordner() {
        // Ein Künstlername darf keinen Ordnerwechsel auslösen.
        assert_eq!(sanitize("../../etc"), ".._.._etc");
        assert_eq!(sanitize("/"), "_");
        assert_eq!(sanitize("C:\\Windows"), "C__Windows");

        // Reine Punktnamen zeigen auf den eigenen oder den übergeordneten
        // Ordner, sie dürfen nie als Ordnername herauskommen.
        for gefaehrlich in [".", "..", "...", " .. ", "..\t"] {
            let sicher = sanitize(gefaehrlich);
            assert!(
                sicher.chars().any(|c| c != '.'),
                "{gefaehrlich:?} blieb ein Punktname: {sicher:?}"
            );
        }
    }

    #[test]
    fn reservierte_windows_namen_werden_entschaerft() {
        // „CON.mp3“ lässt sich unter Windows nicht anlegen.
        for name in ["CON", "con", "Aux", "NUL", "com1", "LPT9"] {
            let sicher = sanitize(name);
            assert!(sicher.starts_with('_'), "{name} blieb reserviert: {sicher}");
        }
        // Normale Namen bleiben unangetastet.
        assert_eq!(sanitize("Nina Chuba"), "Nina Chuba");
        assert_eq!(sanitize("Console"), "Console");
    }

    #[test]
    fn leere_und_unsinnige_namen_bekommen_einen_platzhalter() {
        assert_eq!(sanitize(""), "Unbenannt");
        assert_eq!(sanitize("   "), "Unbenannt");
        assert_eq!(sanitize("\u{0}\u{1}"), "Unbenannt");
        // Übermäßig lange Namen werden gekappt, bleiben aber gültig.
        let lang = sanitize(&"ä".repeat(500));
        assert_eq!(lang.chars().count(), 120);
    }
}
