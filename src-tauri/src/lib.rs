//! wiring of the whole app: storage locations, the custom `robify://` scheme
//! and the tauri setup.

#[cfg(target_os = "android")]
mod android;
pub mod commands;
pub mod db;
pub mod downloader;
pub mod library;
pub mod medien;
pub mod meldung;
pub mod models;
pub mod online;
pub mod player;
pub mod scanner;
pub mod spotify;
pub mod state;
pub mod stats;
pub mod tags;
pub mod ytdlp;

use state::AppState;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::http::{Response, StatusCode};
use tauri::{Emitter, Manager};

/// folder inside the music folder for files brought in by hand.
///
/// the name is fixed and does not travel with the ui language: a folder that
/// suddenly carries a different name after switching to english would leave
/// the files inside it orphaned.
pub(crate) const EIGENE_SONGS: &str = "Eigene Songs";

/// the bundle id before version 0.1.0.
const ALTE_KENNUNG: &str = "de.robify.app";

/// what robify itself stores in the data folder.
///
/// everything else there comes from webkit: caches, local storage, media
/// keys. all of it can be recreated at any time and does not travel along.
const EIGENE_DATEN: [&str; 6] = [
    "robify.db",
    "robify.db-wal",
    "robify.db-shm",
    "papierkorb",
    "downloads",
    "backups",
];

// fetches the data of an earlier bundle id to today's place.
//
// the data folder is named after the bundle id of the app. change it and
// tauri looks in a new place, leaving the entire collection gone as far as
// the user can tell although it lies untouched next to it.
//
// it moves piece by piece, not the folder as a whole: webkit creates the new
// folder before this startup even runs and fills it with its cache. a rename
// failed on that, and a check for "folder is empty" would fail as well.
//
// each piece only travels where nothing lies at the target yet, so an
// existing collection is never overwritten
fn alten_datenordner_uebernehmen(neu: &Path) {
    let Some(alt) = neu.parent().map(|eltern| eltern.join(ALTE_KENNUNG)) else {
        return;
    };
    daten_uebernehmen(&alt, neu);
}

// fetches robify's own data from an old place to today's.
//
// each piece only travels where nothing lies at the target yet, so an
// existing collection is never overwritten
fn daten_uebernehmen(alt: &Path, neu: &Path) {
    if !alt.is_dir() || alt == neu {
        return;
    }

    let mut umgezogen = 0;
    for name in EIGENE_DATEN {
        let quelle = alt.join(name);
        let ziel = neu.join(name);
        if !quelle.exists() || ziel.exists() {
            continue;
        }
        match verschieben(&quelle, &ziel) {
            Ok(()) => umgezogen += 1,
            Err(fehler) => eprintln!("{name} konnte nicht übernommen werden: {fehler}"),
        }
    }

    if umgezogen > 0 {
        eprintln!(
            "{umgezogen} Einträge aus {} übernommen. Der alte Ordner kann weg.",
            alt.display()
        );
    }
}

// moves a file or a folder, across filesystem boundaries too.
//
// within one filesystem a rename is an indivisible step and therefore the
// better way. between two it fails with `EXDEV`: on android the app's own
// folder sits in internal storage, the device storage on a different mount.
// then only copying and clearing up afterwards is left
fn verschieben(quelle: &Path, ziel: &Path) -> std::io::Result<()> {
    if std::fs::rename(quelle, ziel).is_ok() {
        return Ok(());
    }

    if quelle.is_dir() {
        std::fs::create_dir_all(ziel)?;
        for eintrag in std::fs::read_dir(quelle)? {
            let eintrag = eintrag?;
            verschieben(&eintrag.path(), &ziel.join(eintrag.file_name()))?;
        }
        // only once everything is across. breaking off midway leaves the
        // old collection complete
        std::fs::remove_dir_all(quelle)
    } else {
        std::fs::copy(quelle, ziel)?;
        std::fs::remove_file(quelle)
    }
}

// where robify puts its data and where it puts the music.
//
// on a desktop both sit where the operating system expects them, and the
// target folder for the music can be changed in the settings.
//
// on a phone it cannot. two fixed folders stand in the device storage there:
// `Robify` for the tracks, `.robify` for database, downloads and backups. the
// dot in front of the second keeps it out of the gallery and out of file
// listings, it is the usual spelling for "belongs to the app, not to you".
// both are visible and stay behind when robify is removed, unlike everything
// under `Android/data`, which android deletes on uninstall and which no file
// manager has looked into since android 11 anyway.
//
// reaching them hangs on the "access to all files" permission, and that can
// be missing: at the very first start, or because the user refused it. robify
// then stays in its own folder and keeps working instead of not starting at
// all. the switch happens by itself at the next start, the kotlin side asks
// for the permission and restarts the app once it is granted
fn speicherorte(handle: &tauri::AppHandle) -> tauri::Result<(PathBuf, PathBuf, bool)> {
    #[cfg(target_os = "android")]
    if let Some(stamm) = android::geraetespeicher() {
        let daten = stamm.join(".robify");
        let musik = stamm.join("Robify");
        if beschreibbar(&daten) && beschreibbar(&musik) {
            return Ok((daten, musik, true));
        }
        eprintln!(
            "Kein Schreibrecht in {}, Robify bleibt in seinem eigenen Ordner.",
            stamm.display()
        );
    }

    let daten = handle.path().app_data_dir()?;
    let musik = handle
        .path()
        .audio_dir()
        .unwrap_or_else(|_| daten.join("Musik"))
        .join("Robify");
    Ok((daten, musik, false))
}

// whether this folder can actually be written to.
//
// being able to create it is not enough: a folder already there from an
// earlier run stays readable even after the permission has been withdrawn.
// only the attempt itself tells
#[cfg(target_os = "android")]
fn beschreibbar(ordner: &Path) -> bool {
    if std::fs::create_dir_all(ordner).is_err() {
        return false;
    }
    let probe = ordner.join(".schreibprobe");
    let gelungen = std::fs::write(&probe, b"").is_ok();
    let _ = std::fs::remove_file(&probe);
    gelungen
}

// reads in whatever landed in the "Eigene Songs" folder since last time.
//
// that folder is the way in for music which does not come through the
// downloader: files from a computer, from another app, from a memory card.
// whoever drops something in is to find it in the library at the next opening
// without hunting for a button.
//
// only the new ones: otherwise the tags of every file would have to be read
// at every start, and with a few hundred tracks that is a noticeable wait.
// what is in the library already stays untouched.
//
// the files stay where they are. sorting them into artist and album folders
// would be tidier, but it would take exactly that order away from somebody
// who arranges their collection themselves
fn eigene_songs_einlesen(app: &tauri::AppHandle, ordner: &Path) -> usize {
    let state = app.state::<AppState>();
    let bekannt = {
        let conn = state.db();
        library::known_paths(&conn).unwrap_or_default()
    };

    let neue: Vec<std::path::PathBuf> = scanner::collect_audio_files(&[ordner.to_path_buf()])
        .into_iter()
        .filter(|pfad| !bekannt.contains(pfad.to_string_lossy().as_ref()))
        .collect();

    let mut gelesen = 0;
    for pfad in neue {
        // take the lock per file and give it back: player and ui keep
        // reaching for the same database meanwhile
        let conn = state.db();
        match scanner::import_file(&conn, &pfad, Some("lokal")) {
            Ok(_) => gelesen += 1,
            Err(fehler) => eprintln!("{} nicht eingelesen: {fehler}", pfad.display()),
        }
    }
    gelesen
}

// serves `robify://localhost/cover/album/<id>` and `/cover/track/<id>`.
// covers go out of the database directly this way, without being pushed
// through the ipc bridge as base64
fn serve_cover(app: &tauri::AppHandle, path: &str) -> Response<Vec<u8>> {
    let not_found = || {
        Response::builder()
            .status(StatusCode::NOT_FOUND)
            .header("Access-Control-Allow-Origin", "*")
            .body(Vec::new())
            .expect("statische Antwort")
    };

    let segments: Vec<&str> = path.trim_matches('/').split('/').collect();
    if segments.len() != 3 || segments[0] != "cover" {
        return not_found();
    }
    let Ok(id) = segments[2].parse::<i64>() else {
        return not_found();
    };
    let Some(state) = app.try_state::<AppState>() else {
        return not_found();
    };

    let cover = {
        let conn = state.db();
        match segments[1] {
            "album" => library::album_cover(&conn, id).ok().flatten(),
            "track" => library::track_cover(&conn, id).ok().flatten(),
            "artist" => library::artist_image(&conn, id).ok().flatten(),
            "playlist" => library::playlist_cover(&conn, id).ok().flatten(),
            _ => None,
        }
    };

    match cover {
        Some((data, mime)) => Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", mime)
            .header("Cache-Control", "max-age=86400")
            .header("Access-Control-Allow-Origin", "*")
            .body(data)
            .unwrap_or_else(|_| not_found()),
        None => not_found(),
    }
}

// on some linux systems, above all with the nvidia driver, webkitgtk draws a
// black window because it gets no graphics buffer ("Failed to create GBM
// buffer"). without these two switches the ui stays blank.
//
// has to be set before gtk starts. values already present in the environment
// are left alone so the behaviour can be forced if need be
#[cfg(target_os = "linux")]
fn apply_webkit_workarounds() {
    for key in [
        "WEBKIT_DISABLE_DMABUF_RENDERER",
        "WEBKIT_DISABLE_COMPOSITING_MODE",
    ] {
        if std::env::var_os(key).is_none() {
            // SAFETY: runs before any further thread is started
            unsafe { std::env::set_var(key, "1") };
        }
    }
}

#[cfg(not(target_os = "linux"))]
fn apply_webkit_workarounds() {}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    apply_webkit_workarounds();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .register_uri_scheme_protocol("robify", |ctx, request| {
            serve_cover(ctx.app_handle(), request.uri().path())
        })
        .setup(|app| {
            let handle = app.handle().clone();

            let (data_dir, default_library_dir, feste_orte) = speicherorte(&handle)?;
            std::fs::create_dir_all(&data_dir)?;

            // before anything else: whoever comes from an older version is
            // to find their library again
            alten_datenordner_uebernehmen(&data_dir);
            // and whoever comes from the version that still lived in the
            // app's own folder on the phone, likewise
            if let Ok(eigener) = handle.path().app_data_dir() {
                daten_uebernehmen(&eigener, &data_dir);
            }
            let db_path = data_dir.join("robify.db");
            let work_dir = data_dir.join("downloads");
            std::fs::create_dir_all(&work_dir)?;

            // leftovers from last time: cancelled downloads or ones never
            // taken over. after a day the decision has been made
            let entfernt = downloader::cleanup_work_dir(
                &work_dir,
                std::time::Duration::from_secs(24 * 60 * 60),
            );
            if entfernt > 0 {
                eprintln!("{entfernt} liegengebliebene Download-Ordner entfernt");
            }

            // clear deleted files out for good after 30 days. until then the
            // deletion can be undone, after that the folder would grow
            // without bound
            let papierkorb = data_dir.join("papierkorb");
            let alte = library::cleanup_trash(
                &papierkorb,
                std::time::Duration::from_secs(30 * 24 * 60 * 60),
            );
            if alte > 0 {
                eprintln!("{alte} Dateien endgültig aus dem Papierkorb entfernt");
            }

            let conn = db::open(&db_path)?;
            db::migrate(&conn)?;

            // the write-ahead log grows between checkpoints and was never
            // truncated: next to a 2.8 mb database lay 4.2 mb of log
            let _ = conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);");

            // tracks still lying in the old place travel along, row by row,
            // so the library never points at a file that is no longer there
            #[cfg(target_os = "android")]
            if let Ok(alte) = handle.path().audio_dir() {
                let alte = alte.join("Robify");
                let gewandert = library::bibliothek_umziehen(&conn, &alte, &default_library_dir);
                if gewandert > 0 {
                    eprintln!(
                        "{gewandert} Titel nach {} geholt",
                        default_library_dir.display()
                    );
                }
            }

            let player = player::spawn(handle.clone(), db_path.clone())?;

            // the drop folder for files brought in by hand. it is created
            // even where nobody uses it: an empty folder with a clear name
            // says where one's own music belongs, a missing one says nothing
            let eigene = feste_orte.then(|| default_library_dir.join(EIGENE_SONGS));
            if let Some(ordner) = &eigene {
                let _ = std::fs::create_dir_all(ordner);
            }

            app.manage(AppState {
                db: parking_lot::Mutex::new(conn),
                db_path,
                work_dir,
                default_library_dir,
                feste_orte,
                player,
                downloads: Arc::new(downloader::DownloadRegistry::default()),
            });

            // in the background: reading tags takes time and the start is
            // not to wait for it. the state stands already, the thread finds
            // it
            if let Some(ordner) = eigene {
                let nebenher = handle.clone();
                std::thread::spawn(move || {
                    let gelesen = eigene_songs_einlesen(&nebenher, &ordner);
                    if gelesen > 0 {
                        eprintln!("{gelesen} eigene Titel eingelesen");
                        let _ = nebenher.emit("library:changed", ());
                    }
                });
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // --- library ---
            commands::library_stats,
            commands::scan_folders,
            commands::check_library,
            commands::remove_missing_tracks,
            commands::backup_database,
            commands::reset_app,
            commands::list_tracks,
            commands::get_track,
            commands::get_tracks,
            commands::list_artists,
            commands::get_artist,
            commands::artist_releases,
            commands::artist_tracks,
            commands::artist_features,
            commands::search_artists_online,
            commands::apply_artist_metadata,
            commands::fetch_artist_metadata,
            commands::update_artist,
            commands::update_album,
            commands::list_albums,
            commands::get_album,
            commands::album_tracks,
            commands::favorite_tracks,
            commands::set_favorite,
            commands::delete_track,
            commands::restore_track,
            // --- metadata and lyrics ---
            commands::get_track_metadata,
            commands::update_track_metadata,
            commands::search_metadata_online,
            commands::enrich_candidate,
            commands::fetch_cover,
            commands::get_lyrics,
            commands::save_lyrics,
            commands::fetch_lyrics_online,
            commands::search_lyrics_online,
            // --- playlists ---
            commands::list_playlists,
            commands::get_playlist,
            commands::create_playlist,
            commands::create_playlist_from_entries,
            commands::update_playlist,
            commands::delete_playlist,
            commands::restore_playlist,
            commands::playlist_tracks,
            commands::playlists_containing,
            commands::add_to_playlist,
            commands::remove_from_playlist,
            commands::reorder_playlist,
            commands::reorder_favorites,
            commands::reorder_playlists,
            // --- player ---
            commands::player_state,
            commands::play_tracks,
            commands::player_toggle,
            commands::player_play,
            commands::player_pause,
            commands::player_next,
            commands::player_previous,
            commands::player_stop,
            commands::player_seek,
            commands::player_set_volume,
            commands::player_set_muted,
            commands::player_set_repeat,
            commands::player_set_shuffle,
            commands::queue_add,
            commands::queue_play_next,
            commands::queue_remove,
            commands::queue_clear,
            commands::set_sleep_timer,
            // --- statistics ---
            commands::weekly_mix,
            commands::weekly_mixes,
            commands::save_weekly_mix,
            commands::recently_played,
            commands::wrapped,
            // --- downloader ---
            commands::downloader_status,
            commands::update_ytdlp,
            commands::resolve_input,
            commands::start_download,
            commands::cancel_download,
            commands::import_download,
            // --- settings ---
            commands::get_settings,
            commands::set_setting,
            commands::open_path,
            commands::app_paths,
        ])
        .build(tauri::generate_context!())
        .expect("Robify konnte nicht gestartet werden")
        .run(|app, event| {
            // on exit, write away the listening time started but not stored
            if matches!(event, tauri::RunEvent::Exit) {
                if let Some(state) = app.try_state::<AppState>() {
                    let _ = state.player.send(player::Cmd::Shutdown);
                    std::thread::sleep(std::time::Duration::from_millis(150));
                }
            }
        });
}
