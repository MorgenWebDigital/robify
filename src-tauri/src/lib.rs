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

/// Ordner im Musikordner, in den man eigene Dateien legt.
///
/// Der Name steht fest und wandert nicht mit der Sprache der Oberfläche mit:
/// Ein Ordner, der beim Umschalten auf Englisch plötzlich anders heißt, ließe
/// die darin abgelegten Dateien verwaist zurück.
pub(crate) const EIGENE_SONGS: &str = "Eigene Songs";

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
    daten_uebernehmen(&alt, neu);
}

/// Holt Robifys eigene Daten von einem alten Ort an den heutigen.
///
/// Jedes Stück wandert nur, wenn am Ziel noch keines liegt. Ein vorhandener
/// Bestand wird also unter keinen Umständen überschrieben.
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

/// Verschiebt eine Datei oder einen Ordner, auch über Dateisystemgrenzen.
///
/// Innerhalb eines Dateisystems ist das Umbenennen ein unteilbarer Schritt und
/// darum der bessere Weg. Zwischen zweien scheitert es mit `EXDEV`: Auf
/// Android liegt der eigene Ordner der App im inneren Speicher, der
/// Gerätespeicher auf einer anderen Einhängung. Dann bleibt nur kopieren und
/// hinterher wegräumen.
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
        // Erst wenn alles drüben ist. Bricht es mittendrin ab, bleibt der
        // alte Bestand vollständig liegen.
        std::fs::remove_dir_all(quelle)
    } else {
        std::fs::copy(quelle, ziel)?;
        std::fs::remove_file(quelle)
    }
}

/// Wo Robify seine Daten und wo es die Musik ablegt.
///
/// Auf dem Rechner liegen beide dort, wo das Betriebssystem sie erwartet, und
/// der Zielordner für die Musik lässt sich in den Einstellungen ändern.
///
/// Auf dem Telefon nicht. Dort stehen zwei feste Ordner im Gerätespeicher:
/// `Robify` für die Titel, `.robify` für Datenbank, Downloads und
/// Sicherungen. Der Punkt vor dem zweiten hält ihn aus der Galerie und aus
/// den Dateilisten heraus; es ist die übliche Schreibweise für „gehört der
/// App, nicht dir“. Beide sind sichtbar und bleiben liegen, wenn Robify
/// entfernt wird — anders als alles unter `Android/data`, das Android beim
/// Deinstallieren mitlöscht und in das seit Android 11 ohnehin kein
/// Dateimanager mehr hineinsieht.
///
/// Der Griff dorthin hängt an der Erlaubnis „Zugriff auf alle Dateien“, und
/// die kann fehlen: beim allerersten Start, oder weil der Nutzer sie
/// verweigert hat. Dann bleibt Robify im eigenen Ordner und arbeitet weiter,
/// statt gar nicht zu starten. Der Wechsel geschieht beim nächsten Start von
/// selbst, die Kotlin-Seite fragt danach und startet die App neu, sobald die
/// Erlaubnis erteilt ist.
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

/// Lässt sich in diesem Ordner wirklich schreiben?
///
/// Dass er sich anlegen lässt, genügt nicht: Ein bereits vorhandener Ordner
/// aus einem früheren Lauf bleibt lesbar, auch wenn die Erlaubnis inzwischen
/// entzogen wurde. Nur der Versuch selbst gibt Auskunft.
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

/// Liest ein, was seit dem letzten Mal im Ordner „Eigene Songs“ gelandet ist.
///
/// Der Ordner ist der Weg für Musik, die nicht über den Downloader kommt:
/// Dateien vom Rechner, aus einer anderen App, von einer Speicherkarte. Wer
/// etwas hineinlegt, soll es beim nächsten Öffnen in der Bibliothek finden,
/// ohne irgendwo einen Knopf zu suchen.
///
/// Nur das Neue: Für jede Datei wären sonst bei jedem Start die Tags zu lesen,
/// und das ist bei ein paar hundert Titeln eine spürbare Wartezeit. Was schon
/// in der Bibliothek steht, bleibt unangetastet.
///
/// Die Dateien bleiben liegen, wo sie sind. Sie in Künstler- und Albumordner
/// einzusortieren wäre ordentlicher, nähme aber jemandem, der seine Sammlung
/// selbst ordnet, genau diese Ordnung weg.
fn eigene_songs_einlesen(app: &tauri::AppHandle, ordner: &Path) -> usize {
    let state = app.state::<AppState>();
    let bekannt = {
        let conn = state.db.lock();
        library::known_paths(&conn).unwrap_or_default()
    };

    let neue: Vec<std::path::PathBuf> = scanner::collect_audio_files(&[ordner.to_path_buf()])
        .into_iter()
        .filter(|pfad| !bekannt.contains(pfad.to_string_lossy().as_ref()))
        .collect();

    let mut gelesen = 0;
    for pfad in neue {
        // Die Sperre je Datei nehmen und wieder abgeben: Der Player und die
        // Oberfläche greifen währenddessen weiter auf dieselbe Datenbank zu.
        let conn = state.db.lock();
        match scanner::import_file(&conn, &pfad, Some("lokal")) {
            Ok(_) => gelesen += 1,
            Err(fehler) => eprintln!("{} nicht eingelesen: {fehler}", pfad.display()),
        }
    }
    gelesen
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

            let (data_dir, default_library_dir, feste_orte) = speicherorte(&handle)?;
            std::fs::create_dir_all(&data_dir)?;

            // Vor allem anderen: Wer von einer älteren Fassung kommt, soll
            // seine Bibliothek wiederfinden.
            alten_datenordner_uebernehmen(&data_dir);
            // Und wer von der Fassung kommt, die auf dem Telefon noch im
            // eigenen Ordner der App lag, ebenso.
            if let Ok(eigener) = handle.path().app_data_dir() {
                daten_uebernehmen(&eigener, &data_dir);
            }
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

            // Titel, die noch am alten Ort liegen, wandern mit — Zeile für
            // Zeile, damit die Bibliothek zu keinem Zeitpunkt auf eine Datei
            // zeigt, die dort nicht mehr ist.
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

            // Der Ablageordner für eigene Dateien. Angelegt wird er auch dann,
            // wenn niemand ihn benutzt: Ein leerer Ordner mit klarem Namen
            // sagt, wohin die eigene Musik gehört; ein fehlender sagt nichts.
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

            // Im Hintergrund: Tags zu lesen dauert, und der Start soll darauf
            // nicht warten. Der Zustand steht schon, der Faden findet ihn.
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
            commands::reorder_favorites,
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
            commands::update_ytdlp,
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
