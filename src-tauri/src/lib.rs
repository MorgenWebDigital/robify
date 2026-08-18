#[cfg(target_os = "android")]
mod android;
pub mod commands;
pub mod db;
pub mod downloader;
pub mod library;
pub mod meldung;
pub mod models;
pub mod online;
pub mod player;
pub mod scanner;
pub mod spotify;
pub mod state;
pub mod stats;
pub mod tags;

use state::AppState;
use std::path::Path;
use std::sync::Arc;
use tauri::http::{Response, StatusCode};
use tauri::Manager;

/// Kennung vor Version 0.1.0.
///
/// Der Datenordner heißt nach der Kennung der App. Wird sie geändert, sucht
/// Tauri an einem neuen Ort, und die gesamte Sammlung wäre für den Nutzer
/// verschwunden, obwohl sie unberührt daneben liegt.
const ALTE_KENNUNG: &str = "de.robify.app";

/// Was Robify selbst im Datenordner ablegt.
///
/// Alles Übrige dort stammt von WebKit, Zwischenspeicher, lokaler Speicher,
/// Medienschlüssel. Das ist jederzeit neu erzeugbar und wandert nicht mit.
const EIGENE_DATEN: [&str; 6] = [
    "robify.db",
    "robify.db-wal",
    "robify.db-shm",
    "papierkorb",
    "downloads",
    "backups",
];

/// Holt die Daten einer früheren Kennung an den heutigen Ort.
///
/// Der Datenordner heißt nach der Kennung der App. Wird sie geändert, sucht
/// Tauri an einem neuen Ort, und die gesamte Sammlung wäre für den Nutzer
/// verschwunden, obwohl sie unberührt daneben liegt.
///
/// Umgezogen wird Stück für Stück, nicht der Ordner als Ganzes: WebKit legt
/// den neuen Ordner bereits an, bevor dieser Startvorgang überhaupt läuft, und
/// füllt ihn mit seinem Zwischenspeicher. Ein Umbenennen scheiterte daran, und
/// eine Prüfung auf „Ordner ist leer" ginge ebenfalls fehl.
///
/// Jedes Stück wandert nur, wenn am Ziel noch keines liegt. Ein vorhandener
/// Bestand wird also unter keinen Umständen überschrieben.
fn alten_datenordner_uebernehmen(neu: &Path) {
    let Some(alt) = neu.parent().map(|eltern| eltern.join(ALTE_KENNUNG)) else {
        return;
    };
    if !alt.is_dir() {
        return;
    }

    let mut umgezogen = 0;
    for name in EIGENE_DATEN {
        let quelle = alt.join(name);
        let ziel = neu.join(name);
        if !quelle.exists() || ziel.exists() {
            continue;
        }
        // Beide liegen im selben Elternverzeichnis, das Umbenennen ist darum
        // ein unteilbarer Schritt: Es gelingt ganz oder gar nicht.
        match std::fs::rename(&quelle, &ziel) {
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

/// Bedient `robify://localhost/cover/album/<id>` bzw. `/cover/track/<id>`.
/// Cover werden so direkt aus der Datenbank ausgeliefert, ohne sie als
/// Base64 durch die IPC-Brücke zu schicken.
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
        let conn = state.db.lock();
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

/// WebKitGTK zeichnet auf manchen Linux-Systemen, vor allem mit NVIDIA-Treiber
///, ein schwarzes Fenster, weil es keinen Grafikpuffer bekommt
/// („Failed to create GBM buffer“). Ohne diese beiden Schalter bleibt die
/// Oberfläche leer.
///
/// Muss vor dem Start von GTK gesetzt werden. Eigene Vorgaben aus der Umgebung
/// bleiben unangetastet, damit sich das Verhalten notfalls erzwingen lässt.
#[cfg(target_os = "linux")]
fn apply_webkit_workarounds() {
    for key in [
        "WEBKIT_DISABLE_DMABUF_RENDERER",
        "WEBKIT_DISABLE_COMPOSITING_MODE",
    ] {
        if std::env::var_os(key).is_none() {
            // SAFETY: läuft vor dem Start aller weiteren Threads.
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

            let data_dir = handle.path().app_data_dir()?;
            // Vor allem anderen: Wer von einer älteren Fassung kommt, soll
            // seine Bibliothek wiederfinden.
            alten_datenordner_uebernehmen(&data_dir);
            std::fs::create_dir_all(&data_dir)?;
            let db_path = data_dir.join("robify.db");
            let work_dir = data_dir.join("downloads");
            std::fs::create_dir_all(&work_dir)?;

            // Reste vom letzten Mal: abgebrochene oder nie übernommene
            // Downloads. Nach einem Tag ist die Entscheidung gefallen.
            let entfernt = downloader::cleanup_work_dir(
                &work_dir,
                std::time::Duration::from_secs(24 * 60 * 60),
            );
            if entfernt > 0 {
                eprintln!("{entfernt} liegengebliebene Download-Ordner entfernt");
            }

            // Gelöschte Dateien nach 30 Tagen endgültig wegräumen. Bis dahin
            // lässt sich das Löschen zurücknehmen, danach wächst der Ordner
            // sonst unbegrenzt.
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

            // Das Änderungsprotokoll (WAL) wächst zwischen den Prüfpunkten und
            // wurde bisher nie zurückgeschnitten, bei 2,8 MB Datenbank lagen
            // 4,2 MB Protokoll daneben.
            let _ = conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);");

            let default_library_dir = handle
                .path()
                .audio_dir()
                .unwrap_or_else(|_| data_dir.join("Musik"))
                .join("Robify");

            let player = player::spawn(handle.clone(), db_path.clone())?;

            app.manage(AppState {
                db: parking_lot::Mutex::new(conn),
                db_path,
                work_dir,
                default_library_dir,
                player,
                downloads: Arc::new(downloader::DownloadRegistry::default()),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // Bibliothek
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
            // Metadaten & Lyrics
            commands::get_track_metadata,
            commands::update_track_metadata,
            commands::search_metadata_online,
            commands::enrich_candidate,
            commands::fetch_cover,
            commands::get_lyrics,
            commands::save_lyrics,
            commands::fetch_lyrics_online,
            commands::search_lyrics_online,
            // Playlists
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
            commands::reorder_playlists,
            // Player
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
            // Statistiken
            commands::weekly_mix,
            commands::weekly_mixes,
            commands::save_weekly_mix,
            commands::recently_played,
            commands::wrapped,
            // Downloader
            commands::downloader_status,
            commands::resolve_input,
            commands::start_download,
            commands::cancel_download,
            commands::import_download,
            // Einstellungen
            commands::get_settings,
            commands::set_setting,
            commands::open_path,
            commands::app_paths,
        ])
        .build(tauri::generate_context!())
        .expect("Robify konnte nicht gestartet werden")
        .run(|app, event| {
            // Beim Beenden noch die angefangene Hördauer wegschreiben.
            if matches!(event, tauri::RunEvent::Exit) {
                if let Some(state) = app.try_state::<AppState>() {
                    let _ = state.player.send(player::Cmd::Shutdown);
                    std::thread::sleep(std::time::Duration::from_millis(150));
                }
            }
        });
}
