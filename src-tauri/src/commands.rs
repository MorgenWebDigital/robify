//! every command the frontend can invoke.

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

// --- library ---

#[tauri::command]
pub fn library_stats(state: State<'_, AppState>) -> CmdResult<LibraryStats> {
    let conn = state.db();
    Ok(library::library_stats(&conn)?)
}

/// what the android file picker returns instead of a path.
const INHALTSADRESSE: &str = "content://";

// turns what the file picker delivers into readable paths.
//
// on a desktop a chosen entry is a path and stays one. on a phone it is not:
// an address such as
// `content://com.android.externalstorage.documents/document/primary%3A…`
// comes back, and an entry in a cloud may just as well stand behind it.
// treating it as a path finds nothing, and the picker used to end wordlessly
// with "0 tracks imported"
fn adressen_aufloesen(_state: &AppState, paths: Vec<String>) -> Vec<PathBuf> {
    paths
        .into_iter()
        .filter_map(|eintrag| {
            if !eintrag.starts_with(INHALTSADRESSE) {
                return Some(PathBuf::from(eintrag));
            }

            #[cfg(target_os = "android")]
            {
                // the same folder one drops music into by hand
                let ziel = _state.library_dir().join(crate::EIGENE_SONGS);
                crate::android::datei_holen(&eintrag, &ziel)
            }

            // such addresses do not exist elsewhere. were they to arrive
            // anyway, a skipped entry beats a path leading nowhere
            #[cfg(not(target_os = "android"))]
            None
        })
        .collect()
}

#[tauri::command]
pub fn scan_folders(
    app: AppHandle,
    state: State<'_, AppState>,
    paths: Vec<String>,
) -> CmdResult<ScanResult> {
    let roots: Vec<PathBuf> = adressen_aufloesen(&state, paths);
    let result = scanner::scan(&app, &state.db, roots)?;

    // a scan often brings many artists at once. the upper bound keeps a large
    // library from triggering hundreds of queries, the rest can still be
    // caught up one by one
    let offen = {
        let conn = state.db();
        if auto_fetch_artists(&conn) {
            library::all_artists_missing_metadata(&conn, 25).unwrap_or_default()
        } else {
            Vec::new()
        }
    };
    fetch_artists_in_background(&app, offen);

    // look up what is missing: uncertain details, cover, lyrics. the same
    // upper bound as above, so a large collection triggers no hundreds of
    // queries
    let offene_titel = {
        let conn = state.db();
        if auto_fetch_import(&conn) {
            library::tracks_needing_lookup(&conn, 40).unwrap_or_default()
        } else {
            Vec::new()
        }
    };
    enrich_tracks_in_background(&app, offene_titel);

    Ok(result)
}

/// result of comparing the folder against the database.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryCheck {
    /// audio files in the library folder that belong to no track.
    pub orphan_count: usize,
    pub orphan_samples: Vec<String>,
    /// tracks whose file no longer exists.
    pub missing_count: usize,
    pub missing_samples: Vec<String>,
}

/// compares the library folder against the database.
///
/// the two drift apart in daily use: files are moved outside the app, imports
/// break off. on one machine 19 files lay in the folder of which the database
/// knew 5. this command only reports, deleting or importing happens on
/// confirmation.
#[tauri::command]
pub fn check_library(state: State<'_, AppState>) -> CmdResult<LibraryCheck> {
    let conn = state.db();
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

/// removes tracks whose file has disappeared.
///
/// the ids come back, not merely their count: the deletion is soft and can be
/// undone, but only where the ui knows which rows to offer back.
#[tauri::command]
pub fn remove_missing_tracks(app: AppHandle, state: State<'_, AppState>) -> CmdResult<Vec<i64>> {
    let conn = state.db();
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
    let conn = state.db();
    Ok(library::list_tracks(
        &conn,
        search.as_deref(),
        limit.unwrap_or(2000),
    )?)
}

#[tauri::command]
pub fn get_track(state: State<'_, AppState>, id: i64) -> CmdResult<Track> {
    let conn = state.db();
    Ok(library::get_track(&conn, id)?)
}

#[tauri::command]
pub fn get_tracks(state: State<'_, AppState>, ids: Vec<i64>) -> CmdResult<Vec<Track>> {
    let conn = state.db();
    Ok(library::get_tracks(&conn, &ids)?)
}

#[tauri::command]
pub fn list_artists(state: State<'_, AppState>, search: Option<String>) -> CmdResult<Vec<Artist>> {
    let conn = state.db();
    Ok(library::list_artists(&conn, search.as_deref())?)
}

#[tauri::command]
pub fn get_artist(state: State<'_, AppState>, id: i64) -> CmdResult<Artist> {
    let conn = state.db();
    Ok(library::get_artist(&conn, id)?)
}

/// searches online for details about an artist.
#[tauri::command]
pub async fn search_artists_online(name: String) -> CmdResult<Vec<online::ArtistCandidate>> {
    Ok(online::search_artists(&name).await?)
}

/// takes a suggestion over: the image is fetched, the description stored.
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
        let conn = state.db();
        let current = library::get_artist(&conn, artist_id)?;
        // the name stays as it stands in the library, only the extra details
        // come along
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

// finds the hit most likely to be the artist looked for.
//
// two traps are guarded against here, both from practice:
//
// * no fallback pick. under "Julia" genius lists "Julia Michaels" and "Julian
//   Casablancas" but no "Julia". the first hit used to be taken over plainly,
//   and a stranger's face stood in the profile. where the name does not fit,
//   better nothing at all.
// * colliding names. where several hits remain, the work decides: whoever
//   carries a track from one's own library is the one meant. `known_titles`
//   stays empty where there is nothing to compare, and then only an
//   unambiguous name counts
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

    // without tracks of one's own only the name is left, and then it has to
    // be unambiguous at least
    if known_titles.is_empty() {
        return (passend.len() == 1).then(|| passend[0].clone());
    }

    // otherwise the work decides, and it always does: even a single hit can
    // be the wrong one. an artist from kansas is sometimes called the same as
    // a german rap group, and where only they stand at genius, they would be
    // left unchecked
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

    // no work backs the match, better no image than a stranger's
    None
}

/// one press: the best hit is searched for and taken over right away.
#[tauri::command]
pub async fn fetch_artist_metadata(
    app: AppHandle,
    state: State<'_, AppState>,
    artist_id: i64,
) -> CmdResult<Artist> {
    let (name, titel) = {
        let conn = state.db();
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

// fills in missing details on imported tracks.
//
// two cases in one pass:
// * correcting what is wrong. where the title looks like a filename or the
//   artist is missing, the search term is built from the filename and
//   everything the source hands out cleanly separated is overwritten.
// * filling gaps. with cleanly tagged files title and artist stay, and only
//   cover, lyrics, year and genre are fetched.
//
// only what fits unambiguously is taken over: `online::auto_match` demands a
// match in title and artist alike. in doubt the file stays as it is, a wrong
// artist would be worse than a missing one
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
                let conn = state.db();
                !track.has_lyrics && auto_fetch_lyrics(&conn)
            };
            if !unsicher && !will_cover && !will_lyrics {
                continue;
            }

            // build the best possible clue out of what is there. with
            // uncertain details `split_video_title` splits the filename into
            // artist and title and throws suffixes such as "(Official Video)"
            // away
            let ohne_kuenstler = track.artist_name == "Unbekannter Künstler";
            let (kuenstler, titel) = if unsicher {
                match library::split_video_title(
                    &track.title,
                    (!ohne_kuenstler).then_some(track.artist_name.as_str()),
                ) {
                    Some((k, t)) => (k, t),
                    // without an artist and without a separator there is nothing to search for
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

            // to the database only after the query, so the lock is not held
            // across the network
            let zusammengefuehrt = online::merge_match(vorlage, gefunden);
            {
                let state = app.state::<AppState>();
                let conn = state.db();
                if apply_track_metadata(&conn, track.id, &zusammengefuehrt).is_ok() {
                    geaendert += 1;
                }
            }
            // the file gets the details as well, otherwise they would be
            // gone again after a re-import
            let _ = tags::write(Path::new(&track.path), &zusammengefuehrt);
        }

        if geaendert > 0 {
            let _ = app.emit("library:changed", ());
        }
    });
}

// fetches missing artist details in the background.
//
// at the first track of an artist only the name stands in the library
// otherwise. the import does not wait for it: it reports itself done, and the
// ui gets a `library:changed` later, as soon as something arrived.
//
// one after another rather than at once, with an album full of guest artists
// that would be twenty queries in one go
fn fetch_artists_in_background(app: &AppHandle, artists: Vec<(i64, String, Vec<String>)>) {
    if artists.is_empty() {
        return;
    }

    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let mut changed = false;

        for (artist_id, name, titel) in artists {
            // do not take just anybody over: automatically only what fits
            // unambiguously counts. for everything else the button remains
            let Some(candidate) = best_artist_match(&name, &titel).await else {
                continue;
            };

            let image = match candidate.image_url.as_deref() {
                Some(url) => online::fetch_image(url).await.ok(),
                None => None,
            };

            // to the database only after every query, so the lock is not
            // held across the network
            let state = app.state::<AppState>();
            let conn = state.db();
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

/// changes the master data by hand: name, description and image.
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
    let conn = state.db();
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

/// changes title, year, classification and cover of a release.
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
    let conn = state.db();
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
    let conn = state.db();
    Ok(library::artist_releases(&conn, artist_id)?)
}

#[tauri::command]
pub fn artist_tracks(state: State<'_, AppState>, artist_id: i64) -> CmdResult<Vec<Track>> {
    let conn = state.db();
    Ok(library::artist_tracks(&conn, artist_id)?)
}

/// tracks the artist only appears on as a guest.
#[tauri::command]
pub fn artist_features(state: State<'_, AppState>, artist_id: i64) -> CmdResult<Vec<Track>> {
    let conn = state.db();
    Ok(library::artist_features(&conn, artist_id)?)
}

#[tauri::command]
pub fn list_albums(state: State<'_, AppState>, search: Option<String>) -> CmdResult<Vec<Album>> {
    let conn = state.db();
    Ok(library::list_albums(&conn, search.as_deref())?)
}

#[tauri::command]
pub fn get_album(state: State<'_, AppState>, id: i64) -> CmdResult<Album> {
    let conn = state.db();
    Ok(library::get_album(&conn, id)?)
}

#[tauri::command]
pub fn album_tracks(state: State<'_, AppState>, album_id: i64) -> CmdResult<Vec<Track>> {
    let conn = state.db();
    Ok(library::album_tracks(&conn, album_id)?)
}

#[tauri::command]
pub fn favorite_tracks(state: State<'_, AppState>) -> CmdResult<Vec<Track>> {
    let conn = state.db();
    Ok(library::favorite_tracks(&conn)?)
}

#[tauri::command]
pub fn set_favorite(state: State<'_, AppState>, track_id: i64, favorite: bool) -> CmdResult<()> {
    let conn = state.db();
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
        let conn = state.db();
        library::delete_track(&conn, track_id, delete_file, Some(&papierkorb))?;
    }
    let _ = app.emit("library:changed", ());
    Ok(())
}

/// undoes the removal of a track.
#[tauri::command]
pub fn restore_track(
    app: AppHandle,
    state: State<'_, AppState>,
    track_id: i64,
) -> CmdResult<()> {
    let papierkorb = state.trash_dir();
    {
        let conn = state.db();
        library::restore_track(&conn, track_id, Some(&papierkorb))?;
    }
    let _ = app.emit("library:changed", ());
    Ok(())
}

/// undoes the removal of a playlist.
#[tauri::command]
pub fn restore_playlist(state: State<'_, AppState>, id: i64) -> CmdResult<Playlist> {
    let conn = state.db();
    library::restore_playlist(&conn, id)?;
    Ok(library::get_playlist(&conn, id)?)
}

// --- metadata ---

#[tauri::command]
pub fn get_track_metadata(state: State<'_, AppState>, track_id: i64) -> CmdResult<TrackMetadata> {
    let conn = state.db();
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

/// takes edited metadata over into the file and the library alike.
#[tauri::command]
pub fn update_track_metadata(
    app: AppHandle,
    state: State<'_, AppState>,
    track_id: i64,
    metadata: TrackMetadata,
    write_to_file: bool,
) -> CmdResult<Track> {
    let path = {
        let conn = state.db();
        library::get_track(&conn, track_id)?.path
    };

    if write_to_file {
        tags::write(Path::new(&path), &metadata)?;
    }

    let conn = state.db();
    let track = apply_track_metadata(&conn, track_id, &metadata)?;
    drop(conn);

    let _ = app.emit("library:changed", ());
    Ok(track)
}

// writes metadata into the database: artist, album, cover, lyrics.
//
// a function of its own because two ways run into it, the edit dialog and the
// automatic lookup at import
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

/// fetches everything the source hands out for a hit: cover, lyrics, release
/// type, track number and album artist.
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

// --- lyrics ---

#[tauri::command]
pub fn get_lyrics(state: State<'_, AppState>, track_id: i64) -> CmdResult<Option<Lyrics>> {
    let conn = state.db();
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
        let conn = state.db();
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

/// fetches lyrics matching the track automatically and stores them.
#[tauri::command]
pub async fn fetch_lyrics_online(
    state: State<'_, AppState>,
    track_id: i64,
) -> CmdResult<Lyrics> {
    let (artist, title, album, duration_ms) = {
        let conn = state.db();
        let track = library::get_track(&conn, track_id)?;
        (
            track.artist_name,
            track.title,
            track.album_title,
            track.duration_ms,
        )
    };

    let found = online::get_lyrics(&artist, &title, Some(&album), Some(duration_ms)).await?;

    let conn = state.db();
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

// --- playlists ---

#[tauri::command]
pub fn list_playlists(state: State<'_, AppState>) -> CmdResult<Vec<Playlist>> {
    let conn = state.db();
    Ok(library::list_playlists(&conn)?)
}

#[tauri::command]
pub fn get_playlist(state: State<'_, AppState>, id: i64) -> CmdResult<Playlist> {
    let conn = state.db();
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
    let conn = state.db();
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
    let conn = state.db();
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

/// a track as the downloader knows it: artist and name.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaylistEntry {
    pub artist: String,
    pub title: String,
}

/// what came out of taking a playlist over.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaylistFill {
    pub playlist: Playlist,
    /// whether it was newly created or an existing one extended.
    pub created: bool,
    /// how many tracks came along this time.
    pub added: i64,
}

/// creates a playlist or extends an existing one of the same name.
///
/// meant for the downloader: after loading a playlist it is to be taken over
/// in one go. the lookup runs over artist and title, not over ids, so the
/// tracks that already lay in the library and were skipped for it end up in
/// there too.
///
/// a second call creates no copy, it only adds what came along since. where
/// nothing came along, nothing changes. the order of the original is kept.
#[tauri::command]
pub fn create_playlist_from_entries(
    state: State<'_, AppState>,
    name: String,
    entries: Vec<PlaylistEntry>,
) -> CmdResult<PlaylistFill> {
    let conn = state.db();
    let name = name.trim().to_string();

    // the whole library as key pairs once, instead of a lookup per entry
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

    // where it exists already it is extended instead of created a second time
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
    let conn = state.db();
    Ok(library::delete_playlist(&conn, id)?)
}

#[tauri::command]
pub fn playlist_tracks(state: State<'_, AppState>, playlist_id: i64) -> CmdResult<Vec<Track>> {
    let conn = state.db();
    Ok(library::playlist_tracks(&conn, playlist_id)?)
}

/// where these tracks already lie: playlist id and count.
#[tauri::command]
pub fn playlists_containing(
    state: State<'_, AppState>,
    track_ids: Vec<i64>,
) -> CmdResult<Vec<(i64, i64)>> {
    let conn = state.db();
    Ok(library::playlists_containing(&conn, &track_ids)?)
}

#[tauri::command]
pub fn add_to_playlist(
    state: State<'_, AppState>,
    playlist_id: i64,
    track_ids: Vec<i64>,
) -> CmdResult<()> {
    let conn = state.db();
    Ok(library::add_to_playlist(&conn, playlist_id, &track_ids)?)
}

#[tauri::command]
pub fn remove_from_playlist(
    state: State<'_, AppState>,
    playlist_id: i64,
    track_id: i64,
) -> CmdResult<()> {
    let conn = state.db();
    Ok(library::remove_from_playlist(&conn, playlist_id, track_id)?)
}

#[tauri::command]
pub fn reorder_playlist(
    state: State<'_, AppState>,
    playlist_id: i64,
    track_ids: Vec<i64>,
) -> CmdResult<()> {
    let conn = state.db();
    Ok(library::reorder_playlist(&conn, playlist_id, &track_ids)?)
}

/// order of the favourites as the user dragged them.
#[tauri::command]
pub fn reorder_favorites(state: State<'_, AppState>, track_ids: Vec<i64>) -> CmdResult<()> {
    let conn = state.db();
    Ok(library::reorder_favorites(&conn, &track_ids)?)
}

/// order of the collection itself, not of the tracks inside it.
#[tauri::command]
pub fn reorder_playlists(
    app: AppHandle,
    state: State<'_, AppState>,
    playlist_ids: Vec<i64>,
) -> CmdResult<()> {
    let conn = state.db();
    library::reorder_playlists(&conn, &playlist_ids)?;
    drop(conn);
    // the sidebar shows the same order and has to follow
    let _ = app.emit("library:changed", ());
    Ok(())
}

// --- player ---

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

/// `minutes` is ignored with `endOfTrack`.
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

// --- statistics ---

#[tauri::command]
pub fn weekly_mix(state: State<'_, AppState>, offset: Option<i64>) -> CmdResult<WeeklyMix> {
    let conn = state.db();
    Ok(stats::weekly_mix(&conn, offset.unwrap_or(0))?)
}

/// the last weekly mixes for the overview on the home page.
#[tauri::command]
pub fn weekly_mixes(
    state: State<'_, AppState>,
    limit: Option<usize>,
) -> CmdResult<Vec<stats::WeeklyMixSummary>> {
    let conn = state.db();
    Ok(stats::weekly_mixes(&conn, limit.unwrap_or(12))?)
}

/// takes a weekly mix over as a real playlist.
///
/// the mix itself stays a look back and changes with the listening data.
/// whoever wants to keep it gets a copy of their own: nameable, sortable,
/// deletable like any other playlist.
///
/// name and description come from the ui, not from here: they land in the
/// database as text and are to stand in the language the user has set.
#[tauri::command]
pub fn save_weekly_mix(
    app: AppHandle,
    state: State<'_, AppState>,
    offset: Option<i64>,
    name: String,
    description: Option<String>,
) -> CmdResult<Playlist> {
    let conn = state.db();
    let mix = stats::weekly_mix(&conn, offset.unwrap_or(0))?;
    if mix.items.is_empty() {
        return Err(Error(fehler!("Dieser Wochenmix ist noch leer.")));
    }

    // where the name is taken already, the copy gets a number appended
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

/// recently played tracks, each of them once.
#[tauri::command]
pub fn recently_played(state: State<'_, AppState>, limit: Option<i64>) -> CmdResult<Vec<Track>> {
    let conn = state.db();
    Ok(library::recently_played(&conn, limit.unwrap_or(20))?)
}

#[tauri::command]
pub fn wrapped(state: State<'_, AppState>, period: String, offset: i64) -> CmdResult<Wrapped> {
    let conn = state.db();
    Ok(stats::wrapped(&conn, &period, offset)?)
}

// --- downloader ---

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloaderStatus {
    pub ytdlp_path: Option<String>,
    pub ytdlp_version: Option<String>,
    pub ffmpeg_available: bool,
    /// needed for youtube, without it 403 errors come up.
    pub js_runtime: Option<String>,
    /// whether anything can be done about this missing runtime at all.
    ///
    /// on android it cannot: neither node nor deno exists there, and they
    /// cannot be installed either. a warning nobody can act on is not a
    /// warning but noise.
    pub js_runtime_relevant: bool,
    pub active_jobs: Vec<String>,
}

/// where yt-dlp lies and which version it carries.
///
/// on android there is no file to find, yt-dlp ships as a library. the
/// version is asked for over the same bridge the search uses, so the display
/// does not claim something is missing.
///
/// its own function because two callers need it: the downloader shows both
/// values, the update check compares the version.
async fn ytdlp_lage(state: &AppState) -> (Option<String>, Option<String>) {
    if cfg!(target_os = "android") {
        let fassung = crate::ytdlp::einmal(Path::new(""), &["--version".to_string()])
            .await
            .ok()
            .filter(|a| a.erfolg)
            .map(|a| a.stdout.trim().to_string());
        return (Some("eingebaut".to_string()), fassung);
    }

    let configured = {
        let conn = state.db();
        db::get_setting(&conn, "ytdlp_path").ok().flatten()
    };
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
}

#[tauri::command]
pub async fn downloader_status(state: State<'_, AppState>) -> CmdResult<DownloaderStatus> {
    let (path, version) = ytdlp_lage(&state).await;

    Ok(DownloaderStatus {
        ytdlp_path: path,
        ytdlp_version: version,
        ffmpeg_available: downloader::ffmpeg_available(),
        js_runtime: downloader::js_runtime().map(str::to_string),
        js_runtime_relevant: !cfg!(target_os = "android"),
        active_jobs: state.downloads.active(),
    })
}

/// fetches the newest version of yt-dlp and names it.
///
/// youtube keeps changing its player and turns old versions away with a 403.
/// on a desktop yt-dlp helps itself with `-U`, on android it sits in the
/// library in the state that library had when it was released, and there is
/// no other way there than this one.
/// what came of renewing yt-dlp.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct YtdlpErneuert {
    pub fassung: String,
    /// whether robify had to fetch a copy of its own for it.
    pub eigene_kopie: bool,
}

#[tauri::command]
pub async fn update_ytdlp(state: State<'_, AppState>) -> CmdResult<YtdlpErneuert> {
    let configured = {
        let conn = state.db();
        db::get_setting(&conn, "ytdlp_path").ok().flatten()
    };
    let werkzeuge = state.tools_dir();
    let ytdlp = downloader::ensure_ytdlp(configured.as_deref(), &werkzeuge).await?;

    match crate::ytdlp::aktualisieren(&ytdlp).await? {
        crate::ytdlp::Erneuert::Fassung(fassung) => Ok(YtdlpErneuert {
            fassung,
            eigene_kopie: false,
        }),

        // the yt-dlp that was found belongs to pip or to a package manager and
        // refuses to overwrite itself, rightly so. rather than sending the
        // user to a terminal, robify fetches a copy of its own into its tools
        // folder. `find_ytdlp` puts that one before the search path, so from
        // the next call on it is the one in use, and it renews itself with
        // `-U` from then on. the yt-dlp of the system stays untouched.
        //
        // one exception: whoever entered a path by hand meant that path.
        // there the refusal is passed on and nothing is exchanged behind
        // their back.
        #[cfg(not(target_os = "android"))]
        crate::ytdlp::Erneuert::Verweigert(text) => {
            if configured.is_some_and(|pfad| !pfad.trim().is_empty()) {
                return Err(text.into());
            }
            let eigenes = downloader::eigenes_holen(&werkzeuge).await?;
            let fassung = tokio::process::Command::new(&eigenes)
                .arg("--version")
                .output()
                .await
                .ok()
                .filter(|o| o.status.success())
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                .unwrap_or_default();
            Ok(YtdlpErneuert {
                fassung,
                eigene_kopie: true,
            })
        }
    }
}

/// looks whether a newer robify or a newer yt-dlp exists.
///
/// the interface asks this once at the start. it therefore never fails: a
/// network that is down, a rate limit at github, a repository that does not
/// exist yet — all of it ends as "nothing new", and the button stays away.
#[tauri::command]
pub async fn check_updates(
    state: State<'_, AppState>,
) -> CmdResult<crate::aktualisierung::Aktualisierungen> {
    let (_, fassung) = ytdlp_lage(&state).await;
    Ok(crate::aktualisierung::pruefen(fassung).await)
}

/// the notes of the newest robify release, for "show more".
///
/// separate from the check and not part of it: the check runs at every start
/// and is to stay cheap, the notes are fetched at a click and rarely.
#[tauri::command]
pub async fn update_notes() -> CmdResult<Option<String>> {
    Ok(crate::aktualisierung::notizen().await)
}

/// opens the release page of robify in the browser.
///
/// deliberately without an address as an argument: it is the one fixed page,
/// and a command that opens whatever it is handed would be a door out of the
/// web view into the system.
#[tauri::command]
pub fn open_release_page(app: tauri::AppHandle) -> CmdResult<()> {
    use tauri_plugin_opener::OpenerExt;
    app.opener()
        .open_url(crate::aktualisierung::APP_SEITE, None::<&str>)
        .map_err(|fehler| fehler.to_string())?;
    Ok(())
}

// lowercases the scheme of an entered address.
//
// keyboards on a phone capitalise the start of a sentence, so a typed link
// becomes "HTTPS://…". the detection saw no address in that and robify
// searched the whole link as a keyword. it did find the track anyway, but
// warned afterwards that what was loaded did not match the input, comparing
// it against the address.
//
// the scheme only, not the whole address: everything behind it carries
// meaning, youtube ids are case sensitive
fn schema_kleinschreiben(eingabe: &str) -> String {
    for schema in ["https://", "http://"] {
        if eingabe.len() >= schema.len() && eingabe[..schema.len()].eq_ignore_ascii_case(schema) {
            return format!("{schema}{}", &eingabe[schema.len()..]);
        }
    }
    eingabe.to_string()
}

/// takes in what was entered in the downloader and decides what to do:
///
/// * a spotify link resolves the metadata and searches the audio through the
///   remaining sources, spotify hands its recordings out drm-encrypted only
/// * any other address goes straight to yt-dlp, collections are unfolded
/// * everything else counts as a keyword, every search source at once
///
/// nobody has to pick a source beforehand this way.
#[tauri::command]
pub async fn resolve_input(
    state: State<'_, AppState>,
    input: String,
    limit: Option<usize>,
) -> CmdResult<LinkPlan> {
    let input = schema_kleinschreiben(input.trim());
    if input.is_empty() {
        return Err(Error(fehler!("Bitte etwas eingeben.")));
    }

    if let Some(reference) = spotify::parse_link(&input) {
        return resolve_spotify(&state, reference).await;
    }

    let configured = {
        let conn = state.db();
        db::get_setting(&conn, "ytdlp_path").ok().flatten()
    };
    let ytdlp = downloader::ensure_ytdlp(configured.as_deref(), &state.tools_dir()).await?;

    let is_link = input.starts_with("http://") || input.starts_with("https://");

    if is_link {
        // yt-dlp knows the title behind the address, nicer than the bare url.
        // playlists, albums and sets come back as several entries
        let found = downloader::search(&ytdlp, &input, SearchSource::Url, 1)
            .await
            .unwrap_or_default();

        let items: Vec<downloader::DownloadPlan> = if found.is_empty() {
            vec![downloader::DownloadPlan {
                url: input.clone(),
                fallbacks: Vec::new(),
                match_query: None,
                // a pasted link is the intent itself, nothing to check
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
        // the input only, not "hits for …": the ui builds that sentence in
        // its own language, it recognises the case by `kind`
        label: input.clone(),
        kind: "search".into(),
        // a thin result list can come from a blocked source as well
        notes: if ausgefallen.is_empty() {
            Vec::new()
        } else {
            vec![downloader::PlanHinweis {
                code: "quellen-ausgefallen".into(),
                args: vec![ausgefallen.join(", ")],
            }]
        },
        batch: false,
        // where the chosen source fails, the download moves on to the next
        // one by itself instead of stopping at "not possible". the input
        // travels along so the result can be checked against it
        items: downloader::plans_with_fallbacks(found)
            .into_iter()
            .map(|plan| downloader::DownloadPlan {
                intent: Some(input.clone()),
                ..plan
            })
            .collect(),
    })
}

// runs several queries concurrently and collects their results.
//
// deliberately over `tokio::spawn` rather than an extra dependency: for this
// one bundle of cover queries `futures` does not pay off
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

/// this many tracks at most come out of spotify's public embed.
///
/// established by live test (`tests/spotify_live.rs`).
const SPOTIFY_EMBED_LIMIT: usize = 100;

async fn resolve_spotify(
    state: &AppState,
    reference: spotify::SpotifyRef,
) -> CmdResult<LinkPlan> {
    let release = spotify::resolve(&reference).await?;

    // fetch every existing track as key pairs once, instead of reaching into
    // the database again for each entry of the playlist
    let bereits: std::collections::HashSet<(String, String)> = {
        let conn = state.db();
        library::list_tracks(&conn, None, 100_000)
            .unwrap_or_default()
            .into_iter()
            .map(|t| (crate::db::key_of(&t.artist_name), crate::db::key_of(&t.title)))
            .collect()
    };

    // load the cover once and use it for every track of the release
    let cover = match release.cover_url.as_deref() {
        Some(url) => online::fetch_image(url)
            .await
            .ok()
            .map(|(data, mime)| (b64(&data), mime)),
        None => None,
    };

    let is_album = matches!(reference.kind, spotify::SpotifyKind::Album);
    let album_title = if is_album { release.name.clone() } else { String::new() };

    // the image belongs to every single track only where it is an album or a
    // single track. with a playlist it is that playlist's own image, the
    // tracks inside come from entirely different releases and get their
    // proper cover during enrichment after the download
    let track_cover = matches!(
        reference.kind,
        spotify::SpotifyKind::Album | spotify::SpotifyKind::Track
    )
    .then_some(cover.as_ref())
    .flatten();

    // for playlists, look up the image of every track separately. the track
    // list hands out the id only, and without this reach every entry would
    // carry the image of the playlist. at most eight queries at a time so
    // spotify does not throttle, and with a timeout so one slow answer does
    // not hold up the resolving
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
                // spotify knows no roles, everyone involved is a lead artist
                // unless they stood in the title as "feat."
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
                // spotify knows the running time exactly, and the downloader
                // picks the best matching recording from it
                match_query: Some(format!("{} {}", primary_artist, track.title)),
                // spotify names title and artist authoritatively
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
            // the public embed never hands out more than 100 tracks. without
            // a hint one wonders why a long playlist stops exactly there
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
        let conn = state.db();
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

    // the settings decide what is fetched automatically
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

/// names windows reserves for devices.
///
/// a file called "CON.mp3" cannot be created there, so an artist named "Aux"
/// would make the import fail on a windows machine.
const RESERVED_NAMES: [&str; 22] = [
    "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8",
    "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
];

/// folder in the working directory for tracks taken over but not filed into
/// the music folder. exempt from the cleanup, see `downloader::cleanup_work_dir`.
pub const KEEP_DIR: &str = "behalten";

/// whether the directory belongs to a download job.
///
/// the downloader creates `job-<timestamp>-<no>` per job. the check keeps the
/// cleanup from catching a different folder.
pub fn is_job_dir(dir: &Path) -> bool {
    dir.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with("job-"))
}

/// moves a file into a target directory without overwriting anything there.
///
/// across drive boundaries `rename` fails, and it copies and removes the
/// source then.
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
    // cutting the trailing dots defuses "." and ".." along the way: both end
    // up empty and get the placeholder
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

/// moves the file into the library and creates the track.
///
/// called after confirming the metadata in the downloader.
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

    // the file leaves the job folder in any case. staying there, the library
    // would point into a working directory that has to be cleared out, and
    // any cleanup would tear such tracks along
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
        // "do not file it" means not into the music folder, not that the
        // file stays lying in the working directory
        state.work_dir.join(KEEP_DIR)
    };

    let final_path = move_file(&source_path, &dir, &file_name)?;

    // the job is done, its folder holds nothing but helper files
    // (result.txt, uploader.txt, musik.txt, thumbnails)
    if let Some(job_dir) = source_path.parent().filter(|dir| is_job_dir(dir)) {
        let _ = std::fs::remove_dir_all(job_dir);
    }

    let conn = state.db();
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
    } else {
        // without a detail from the net the track count decides, exactly as
        // when reading a folder. that used to run only there, and a
        // downloaded track without a recognised type stayed an album forever,
        // even standing on its own
        library::refresh_release_types(&conn)?;
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
    // artists arriving with this track carry no details yet
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

// --- settings ---

// whether lyrics are searched for automatically. defaults to yes
fn auto_fetch_lyrics(conn: &rusqlite::Connection) -> bool {
    db::get_setting(conn, "auto_fetch_lyrics")
        .ok()
        .flatten()
        .as_deref()
        != Some("0")
}

// whether details are looked up at import. defaults to yes
fn auto_fetch_import(conn: &rusqlite::Connection) -> bool {
    db::get_setting(conn, "auto_fetch_import")
        .ok()
        .flatten()
        .as_deref()
        != Some("0")
}

// whether artist details may be fetched automatically. defaults to yes
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
    /// fetch image and description at the first track of an artist.
    pub auto_fetch_artists: bool,
    pub move_downloads_into_library: bool,
    pub accent: String,
    /// "system", "light" or "dark".
    pub theme: String,
    /// playlists as tiles ("grid") or as a list ("list").
    pub playlist_view: String,
    /// tile size: "sm", "md" or "lg".
    pub playlist_size: String,
    /// ask before deleting. can be turned off in the dialog itself.
    pub confirm_delete: bool,
    /// turn a downloaded playlist into a playlist in the library.
    pub playlist_from_download: bool,
    /// how much of the review is shown: "all", "month", "year" or "off".
    pub wrapped_mode: String,
    /// look missing details up online at import.
    pub auto_fetch_import: bool,
    /// sorting of the library: "added", "title", "artist", "album", "year".
    pub library_sort: String,
    /// accent colours mixed by hand, comma separated ("#ff0000,#00ff00").
    ///
    /// as a string, because the settings table knows text only.
    pub accent_custom: String,
    /// ui language: "system", "de" or "en".
    pub language: String,
    /// whether the storage locations are fixed. then there is nothing to set.
    ///
    /// on a phone the tracks lie in `Robify` and everything else in
    /// `.robify`, both in the device storage. the ui hides the folder choice
    /// accordingly instead of offering a setting that does nothing.
    pub feste_orte: bool,
}

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> CmdResult<Settings> {
    // before locking: `library_dir()` reaches into the database itself, and
    // taking the same lock twice puts the call to sleep
    let library_dir = state.library_dir().to_string_lossy().into_owned();
    let conn = state.db();
    let get = |key: &str, fallback: &str| {
        db::get_setting(&conn, key)
            .ok()
            .flatten()
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| fallback.to_string())
    };
    let flag = |key: &str, fallback: bool| get(key, if fallback { "1" } else { "0" }) == "1";

    Ok(Settings {
        // the folder that actually applies: on a phone it is fixed and the
        // setting is not read there
        library_dir,
        download_format: get("download_format", "best"),
        download_quality: get("download_quality", "0"),
        ytdlp_path: get("ytdlp_path", ""),
        auto_fetch_lyrics: flag("auto_fetch_lyrics", true),
        auto_fetch_cover: flag("auto_fetch_cover", true),
        auto_fetch_artists: flag("auto_fetch_artists", true),
        move_downloads_into_library: flag("move_downloads_into_library", true),
        accent: get("accent", "#a8a8b3"),
        theme: get("theme", "system"),
        // a list instead of tiles, and in the large form: a playlist is
        // recognised by its name, not by a mosaic of four covers. as a row the
        // name stands next to it instead of cut off underneath
        playlist_view: get("playlist_view", "list"),
        playlist_size: get("playlist_size", "lg"),
        confirm_delete: flag("confirm_delete", true),
        playlist_from_download: flag("playlist_from_download", true),
        wrapped_mode: get("wrapped_mode", "all"),
        auto_fetch_import: flag("auto_fetch_import", true),
        // by artist is the order one walks a collection in
        library_sort: get("library_sort", "artist"),
        accent_custom: get("accent_custom", ""),
        // "system" follows the setting of the operating system
        language: get("language", "system"),
        feste_orte: state.feste_orte,
    })
}

#[tauri::command]
pub fn set_setting(state: State<'_, AppState>, key: String, value: String) -> CmdResult<()> {
    let conn = state.db();
    Ok(db::set_setting(&conn, &key, &value)?)
}

/// how many backups are kept.
const MAX_BACKUPS: usize = 5;

/// folder for the backups where the storage locations are fixed.
const SICHERUNGEN: &str = "saves";

/// writes a backup of the database and returns its path.
///
/// `VACUUM INTO` writes a copy consistent in itself even while the database
/// is open, where plainly copying the file could land in the middle of a
/// write.
#[tauri::command]
pub fn backup_database(state: State<'_, AppState>) -> CmdResult<String> {
    // where the locations are fixed, the backup lies visibly in the music
    // folder under "saves", not in the hidden ".robify". a backup is of use
    // only where it can be found and copied away, and the hidden folder is
    // meant for what the user is never to touch
    let dir = if state.feste_orte {
        state.library_dir().join(SICHERUNGEN)
    } else {
        state
            .db_path
            .parent()
            .map(|p| p.join("backups"))
            .ok_or_else(|| Error(fehler!("Kein Ort für die Sicherung gefunden.")))?
    };
    std::fs::create_dir_all(&dir)?;

    let stempel = chrono::Local::now().format("%Y-%m-%d-%H%M%S");
    let ziel = dir.join(format!("robify-{stempel}.db"));

    {
        let conn = state.db();
        // the path goes in as a string because vacuum allows no placeholders.
        // single quotes are doubled, as usual in sql
        let pfad = ziel.to_string_lossy().replace('\'', "''");
        conn.execute_batch(&format!("VACUUM INTO '{pfad}'"))?;
    }

    // clear older backups away so they do not pile up
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

/// resets robify to the state it ships in.
///
/// a backup of the database is always written first and its path returned.
/// without it the action would be final, and this is the one button where a
/// misgrasp costs everything.
///
/// `delete_files` concerns the audio files. even then they are not deleted
/// but moved into the app's trash, where they lie for 30 days before they go
/// for good.
#[tauri::command]
pub fn reset_app(
    app: AppHandle,
    state: State<'_, AppState>,
    delete_files: bool,
    keep_settings: bool,
) -> CmdResult<String> {
    // back up first. where that fails, nothing is touched
    let sicherung = backup_database(state.clone())?;

    // nothing is to keep running while its file disappears
    let _ = state.player.send(crate::player::Cmd::Stop);

    let papierkorb = state.trash_dir();
    let conn = state.db();

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

    // the order follows the dependencies even though the foreign keys
    // cascade: that way what gets emptied stays traceable
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
    // otherwise the file keeps its old size although nothing is left in it
    conn.execute_batch("VACUUM;")?;
    drop(conn);

    // leftover working folders along with it
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
    use super::{sanitize, schema_kleinschreiben};

    /// a typed link stays a link.
    ///
    /// on a phone the keyboard capitalises the start of a sentence, so
    /// "https://…" became "HTTPS://…" and robify took it for a keyword: it
    /// searched the whole link in every source and warned afterwards that
    /// what was loaded did not match the input, compared against the address.
    #[test]
    fn grossgeschriebenes_schema_wird_erkannt() {
        assert_eq!(
            schema_kleinschreiben("HTTPS://www.youtube.com/watch?v=7ccyYIfoRPg"),
            "https://www.youtube.com/watch?v=7ccyYIfoRPg"
        );
        assert_eq!(
            schema_kleinschreiben("Http://beispiel.test/Weg"),
            "http://beispiel.test/Weg"
        );
    }

    /// behind the scheme everything stays as it was.
    ///
    /// youtube ids are case sensitive, and lowercasing the whole address led
    /// to a different video or nowhere at all.
    #[test]
    fn nur_das_schema_wird_angefasst() {
        assert_eq!(
            schema_kleinschreiben("https://youtu.be/AbCdEfGhIjK"),
            "https://youtu.be/AbCdEfGhIjK"
        );
        // a keyword stays untouched even where it starts in capitals
        assert_eq!(schema_kleinschreiben("Yeat COMË N GO"), "Yeat COMË N GO");
        assert_eq!(schema_kleinschreiben(""), "");
    }

    #[test]
    fn dateinamen_bleiben_im_zielordner() {
        // an artist name must not trigger a change of folder
        assert_eq!(sanitize("../../etc"), ".._.._etc");
        assert_eq!(sanitize("/"), "_");
        assert_eq!(sanitize("C:\\Windows"), "C__Windows");

        // names of dots alone point at the folder itself or its parent, they
        // must never come out as a folder name
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
        // "CON.mp3" cannot be created under windows
        for name in ["CON", "con", "Aux", "NUL", "com1", "LPT9"] {
            let sicher = sanitize(name);
            assert!(sicher.starts_with('_'), "{name} blieb reserviert: {sicher}");
        }
        // ordinary names stay untouched
        assert_eq!(sanitize("Nina Chuba"), "Nina Chuba");
        assert_eq!(sanitize("Console"), "Console");
    }

    #[test]
    fn leere_und_unsinnige_namen_bekommen_einen_platzhalter() {
        assert_eq!(sanitize(""), "Unbenannt");
        assert_eq!(sanitize("   "), "Unbenannt");
        assert_eq!(sanitize("\u{0}\u{1}"), "Unbenannt");
        // excessively long names are cut but stay valid
        let lang = sanitize(&"ä".repeat(500));
        assert_eq!(lang.chars().count(), 120);
    }
}
